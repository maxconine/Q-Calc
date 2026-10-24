//! The engine's threads (ARCHITECTURE.md §5): one evaluation thread for the per-keystroke calls and a small
//! pool for long work. Each thread has a 32 MB stack, because a dispatch thread's 512 KB can't hold parsing and
//! evaluating near the parser's nesting limit. A sync call sends its closure over and blocks for the answer. An
//! async call returns a future that the pool's thread completes and wakes, so it needs no async runtime: UniFFI
//! polls it from Swift's executor.

use std::any::Any;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex, Once};
use std::task::{Context, Poll, Waker};
use std::thread;

use qcalc_core::work::{Cancelled, with_cancel};

use crate::records::CoreError;

/// The evaluation threads' stack.
pub const STACK_BYTES: usize = 32 * 1024 * 1024;
/// Threads for async work, so one long job never waits for another.
pub const LONG_THREADS: usize = 2;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// A queue of jobs served by `n` threads with 32 MB stacks.
#[derive(Debug)]
struct Pool {
    tx: Mutex<Sender<Job>>,
}

impl Pool {
    fn new(name: &str, n: usize) -> Pool {
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..n {
            spawn_worker(format!("{name}-{i}"), rx.clone());
        }
        Pool { tx: Mutex::new(tx) }
    }

    /// Queues a job; false when the pool is gone (it never is while the engine lives).
    fn send(&self, job: Job) -> bool {
        self.tx.lock().map(|tx| tx.send(job).is_ok()).unwrap_or(false)
    }
}

fn spawn_worker(name: String, rx: Arc<Mutex<Receiver<Job>>>) {
    let spawned = thread::Builder::new()
        .name(name)
        .stack_size(STACK_BYTES)
        .spawn(move || {
            loop {
                // the lock is held only while waiting for the next job, never while running it
                let job = match rx.lock() {
                    Ok(rx) => rx.recv(),
                    Err(_) => return,
                };
                match job {
                    // a job catches its own panics, so the thread outlives every job
                    Ok(job) => job(),
                    Err(_) => return,
                }
            }
        });
    // a thread the system refuses leaves the queue to the others; `run` falls back to a fresh thread when none
    drop(spawned);
}

/// The engine's threads.
#[derive(Debug)]
pub struct Workers {
    live: Pool,
    long: Pool,
}

impl Default for Workers {
    fn default() -> Self {
        Workers::new()
    }
}

impl Workers {
    pub fn new() -> Workers {
        quiet_cancel_panics();
        Workers {
            live: Pool::new("qcalc-eval", 1),
            long: Pool::new("qcalc-long", LONG_THREADS),
        }
    }

    /// Runs `f` on the evaluation thread and waits for it.
    pub fn run<R: Send + 'static>(&self, f: impl FnOnce() -> R + Send + 'static) -> Result<R, CoreError> {
        let (tx, rx) = sync_channel(1);
        let job: Job = Box::new(move || {
            let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
        });
        if !self.live.send(job) {
            return Err(internal("the evaluation thread is gone"));
        }
        match rx.recv() {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(panic)) => Err(from_panic(panic)),
            Err(_) => Err(internal("the evaluation thread stopped")),
        }
    }

    /// Runs `f` on the long-work pool with `cancel` as its cancel flag. The future resolves to `f`'s answer, or
    /// `Cancelled` when the flag was set first. Dropping the future cancels the job too.
    pub fn spawn<R: Send + 'static>(
        &self,
        cancel: Arc<AtomicBool>,
        f: impl FnOnce() -> R + Send + 'static,
    ) -> Pending<R> {
        let slot = Arc::new(Mutex::new(Slot { value: None, waker: None }));
        let pending = Pending {
            slot: slot.clone(),
            cancel: cancel.clone(),
            done: false,
        };
        if cancel.load(Ordering::Relaxed) {
            fill(&slot, Err(CoreError::Cancelled));
            return pending;
        }
        let job_slot = slot.clone();
        let job: Job = Box::new(move || {
            let out = if cancel.load(Ordering::Relaxed) {
                Err(CoreError::Cancelled)
            } else {
                match catch_unwind(AssertUnwindSafe(|| with_cancel(cancel.clone(), f))) {
                    Ok(r) => Ok(r),
                    Err(panic) => Err(from_panic(panic)),
                }
            };
            fill(&job_slot, out);
        });
        if !self.long.send(job) {
            fill(&slot, Err(internal("the long-work threads are gone")));
        }
        pending
    }
}

#[derive(Debug)]
struct Slot<R> {
    value: Option<Result<R, CoreError>>,
    waker: Option<Waker>,
}

fn fill<R>(slot: &Mutex<Slot<R>>, out: Result<R, CoreError>) {
    let waker = match slot.lock() {
        Ok(mut s) => {
            s.value = Some(out);
            s.waker.take()
        }
        Err(_) => None,
    };
    if let Some(w) = waker {
        w.wake();
    }
}

/// An async engine call's answer, filled by the long-work thread.
#[derive(Debug)]
pub struct Pending<R> {
    slot: Arc<Mutex<Slot<R>>>,
    cancel: Arc<AtomicBool>,
    done: bool,
}

impl<R> Future for Pending<R> {
    type Output = Result<R, CoreError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let taken = match self.slot.lock() {
            Ok(mut s) => match s.value.take() {
                Some(v) => Some(v),
                None => {
                    s.waker = Some(cx.waker().clone());
                    None
                }
            },
            Err(_) => Some(Err(internal("the answer slot was poisoned"))),
        };
        match taken {
            Some(v) => {
                self.done = true;
                Poll::Ready(v)
            }
            None => Poll::Pending,
        }
    }
}

impl<R> Drop for Pending<R> {
    // nobody is waiting for the answer any more, so stop working on it
    fn drop(&mut self) {
        if !self.done {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn internal(message: &str) -> CoreError {
    CoreError::Internal {
        message: message.to_string(),
    }
}

fn from_panic(panic: Box<dyn Any + Send>) -> CoreError {
    if panic.downcast_ref::<Cancelled>().is_some() {
        return CoreError::Cancelled;
    }
    let message = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into());
    CoreError::Internal { message }
}

/// A cancelled job unwinds with a `Cancelled` payload. That is the normal way a job stops, so it shouldn't print
/// a panic message. Every other panic still goes to the previous hook.
fn quiet_cancel_panics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if info.payload().downcast_ref::<Cancelled>().is_none() {
                previous(info);
            }
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Wake;

    struct Flag(AtomicBool);
    impl Wake for Flag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn block_on<R>(mut f: Pending<R>) -> Result<R, CoreError> {
        let flag = Arc::new(Flag(AtomicBool::new(false)));
        let waker = Waker::from(flag.clone());
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(v) = Pin::new(&mut f).poll(&mut cx) {
                return v;
            }
            while !flag.0.swap(false, Ordering::SeqCst) {
                thread::yield_now();
            }
        }
    }

    // recursion a 2 MB test thread can't hold
    fn deep(n: u64) -> u64 {
        let pad = [n; 512];
        if n == 0 { 0 } else { std::hint::black_box(pad)[0] % 7 + deep(n - 1) }
    }

    #[test]
    fn runs_on_a_big_stack() {
        let w = Workers::new();
        assert_eq!(w.run(|| deep(5_000)).unwrap(), deep_value(5_000));
        assert!(matches!(w.run(|| panic!("boom")), Err(CoreError::Internal { message }) if message == "boom"));
        // the thread survives a panic
        assert_eq!(w.run(|| 2 + 2).unwrap(), 4);
    }

    fn deep_value(n: u64) -> u64 {
        (0..=n).map(|i| if i == 0 { 0 } else { i % 7 }).sum()
    }

    #[test]
    fn async_jobs_answer_and_cancel() {
        let w = Workers::new();
        assert_eq!(block_on(w.spawn(Arc::new(AtomicBool::new(false)), || 7)).unwrap(), 7);
        let done = Arc::new(AtomicBool::new(true));
        assert!(matches!(block_on(w.spawn(done, || 7)), Err(CoreError::Cancelled)));
        // cancelled mid-job: the next work::spend unwinds
        let flag = Arc::new(AtomicBool::new(false));
        let f2 = flag.clone();
        let job = w.spawn(flag.clone(), move || -> u32 {
            loop {
                f2.store(true, Ordering::Relaxed);
                qcalc_core::work::spend(1);
            }
        });
        assert!(matches!(block_on(job), Err(CoreError::Cancelled)));
    }
}
