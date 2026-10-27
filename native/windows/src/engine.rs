//! The engine thread: the one place the UI reaches the maths from, so the UI thread never evaluates.
//!
//! One long-lived thread with a 32 MB stack owns the `qcalc_ffi::Engine` (which itself hops each call to its own
//! 32 MB evaluation thread; ARCHITECTURE.md §5). Work comes in *lanes* (`Lane`), as in the Mac's
//! `EngineThread.swift`: a lane holds at most one waiting job, a newer one replaces it, so a burst of keystrokes
//! evaluates only the latest text. Every job carries its lane's generation; its result goes back to the UI thread
//! as a boxed closure (`Done`) through `PostMessageW(WM_ENGINE)`, and app.rs drops it unless its generation is still
//! the lane's newest, so a slow answer never overwrites a newer one.
//!
//! ```ignore
//! app.engine.submit(Lane::Forms, move |cx| {
//!     let forms = cx.engine.answer_forms(source);         // on the engine thread
//!     done(move |app: &mut App| app.model.set_forms(forms)) // back on the UI thread, if still the newest
//! });
//! ```
//!
//! `Cx` also keeps per-thread caches: the history's `EvalContext` (recomputed only when the rows or the history
//! settings change, or a second has passed, since `recent` depends on the clock).
//!
//! The typeset preview's `TypstRenderer` lives on its own thread (`typst()`), warmed at launch, so a Typst compile
//! never delays a keystroke's answer. typst_row.rs owns what is drawn from it.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use qcalc_ffi::{Engine, EngineConfig, EvalContext, HistoryRow, HistoryView, TypstRenderer};

use crate::app::App;
use crate::qlog;

pub const STACK_BYTES: usize = 32 * 1024 * 1024;

/// What the UI thread runs with a result.
pub type Done = Box<dyn FnOnce(&mut App) + Send>;

/// A result for the UI thread.
pub fn done(f: impl FnOnce(&mut App) + Send + 'static) -> Option<Done> {
    Some(Box::new(f))
}

/// Lanes: one waiting job each; a newer job in a lane replaces the waiting one and makes running ones stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
    /// a keystroke's live answer
    Live,
    /// Enter
    Commit,
    /// Tab's answer forms
    Forms,
    /// graph sampling (graph.rs)
    Graph,
    /// the typeset row's source (typst_row.rs)
    Typeset,
    /// history load / save, warm-up and anything that must not be dropped: never replaced, never stale
    Misc,
}

/// The engine thread's side: the engine and its caches.
pub struct Cx {
    pub engine: Arc<Engine>,
    context: Option<ContextKey>,
}

struct ContextKey {
    rows_gen: u64,
    view: (u8, u8, u32),
    at: Instant,
    context: EvalContext,
}

impl Cx {
    /// `Engine::context_from_history`, cached by the rows' generation and the view.
    pub fn context(&mut self, rows: &Rows, view: HistoryView) -> EvalContext {
        let key = (view.show as u8, view.answer_form as u8, view.sig_figs);
        if let Some(c) = &self.context
            && c.rows_gen == rows.generation
            && c.view == key
            && c.at.elapsed().as_millis() < 1000
        {
            return c.context.clone();
        }
        let context = self.engine.context_from_history(rows.rows.to_vec(), view);
        self.context = Some(ContextKey {
            rows_gen: rows.generation,
            view: key,
            at: Instant::now(),
            context: context.clone(),
        });
        context
    }
}

/// The history as the engine thread sees it: shared, with a generation that changes with every edit.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub rows: Arc<Vec<HistoryRow>>,
    pub generation: u64,
}

static ROWS_GEN: AtomicU64 = AtomicU64::new(1);

impl Rows {
    pub fn new(rows: Vec<HistoryRow>) -> Rows {
        Rows {
            rows: Arc::new(rows),
            generation: ROWS_GEN.fetch_add(1, Ordering::Relaxed),
        }
    }
}

/// Work for the Typst thread.
type TypstJob = Box<dyn FnOnce(&TypstRenderer) -> Option<Done> + Send>;

type Work = Box<dyn FnOnce(&mut Cx) -> Option<Done> + Send>;

struct Job {
    lane: Lane,
    generation: u64,
    work: Work,
}

#[derive(Default)]
struct Queue {
    waiting: HashMap<Lane, Job>,
    misc: VecDeque<Job>,
    order: VecDeque<Lane>,
    newest: HashMap<Lane, u64>,
    quit: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    results: Mutex<Vec<(Lane, u64, Done)>>,
}

/// The UI's handle on the engine thread.
pub struct EngineThread {
    shared: Arc<Shared>,
    /// the window that gets WM_ENGINE (as an integer: HWND isn't Send)
    typst: Sender<TypstJob>,
}

impl EngineThread {
    /// Starts the engine thread (and the Typst thread) and warms both off the UI thread. `notify` is called on
    /// those threads whenever a result is ready (app.rs posts WM_ENGINE).
    pub fn start(config: EngineConfig, notify: impl Fn() + Send + Sync + 'static) -> EngineThread {
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(notify);
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            results: Mutex::new(Vec::new()),
        });
        let s = shared.clone();
        let n = notify.clone();
        std::thread::Builder::new()
            .name("qcalc-engine".into())
            .stack_size(STACK_BYTES)
            .spawn(move || {
                let t = Instant::now();
                let engine = Engine::new(config);
                qlog!(
                    "engine created in {:.1} ms",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                let mut cx = Cx {
                    engine,
                    context: None,
                };
                run(&s, &mut cx, &*n);
            })
            .expect("the engine thread");

        let (tx, rx) = channel::<TypstJob>();
        let s = shared.clone();
        std::thread::Builder::new()
            .name("qcalc-typst".into())
            .stack_size(STACK_BYTES)
            .spawn(move || {
                let t = Instant::now();
                let typst = TypstRenderer::new();
                let ms = typst.warm_up();
                qlog!(
                    "typst warmed in {:.1} ms (warm_up says {ms:.1} ms)",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                while let Ok(job) = rx.recv() {
                    if let Some(d) = job(&typst) {
                        if let Ok(mut r) = s.results.lock() {
                            r.push((Lane::Misc, 0, d));
                        }
                        notify();
                    }
                }
            })
            .expect("the typst thread");
        EngineThread { shared, typst: tx }
    }

    /// Queues `work` on `lane`; its `Done` runs on the UI thread if it is still the lane's newest by then.
    pub fn submit(
        &self,
        lane: Lane,
        work: impl FnOnce(&mut Cx) -> Option<Done> + Send + 'static,
    ) -> u64 {
        let Ok(mut q) = self.shared.queue.lock() else {
            return 0;
        };
        let generation = q.newest.get(&lane).copied().unwrap_or(0) + 1;
        q.newest.insert(lane, generation);
        let job = Job {
            lane,
            generation,
            work: Box::new(work),
        };
        if lane == Lane::Misc {
            q.misc.push_back(job);
        } else {
            if !q.waiting.contains_key(&lane) {
                q.order.push_back(lane);
            }
            q.waiting.insert(lane, job);
        }
        drop(q);
        self.shared.wake.notify_one();
        generation
    }

    /// Makes a lane's waiting and running jobs stale (the line was cleared, a command took over).
    pub fn cancel(&self, lane: Lane) {
        if let Ok(mut q) = self.shared.queue.lock() {
            *q.newest.entry(lane).or_insert(0) += 1;
            q.waiting.remove(&lane);
            q.order.retain(|l| *l != lane);
        }
    }

    /// Work for the Typst thread (typst_row.rs). Its results are never stale-checked; check your own state.
    pub fn typst(&self, work: impl FnOnce(&TypstRenderer) -> Option<Done> + Send + 'static) {
        let _ = self.typst.send(Box::new(work));
    }

    /// On the UI thread (WM_ENGINE): the results whose generation is still their lane's newest.
    pub fn take_results(&self) -> Vec<Done> {
        let results = match self.shared.results.lock() {
            Ok(mut r) => std::mem::take(&mut *r),
            Err(_) => return Vec::new(),
        };
        let newest = match self.shared.queue.lock() {
            Ok(q) => q.newest.clone(),
            Err(_) => return Vec::new(),
        };
        results
            .into_iter()
            .filter(|(lane, generation, _)| {
                *lane == Lane::Misc || newest.get(lane).copied().unwrap_or(0) == *generation
            })
            .map(|(_, _, d)| d)
            .collect()
    }

    pub fn shutdown(&self) {
        if let Ok(mut q) = self.shared.queue.lock() {
            q.quit = true;
        }
        self.shared.wake.notify_all();
    }
}

fn run(shared: &Shared, cx: &mut Cx, notify: &(dyn Fn() + Send + Sync)) {
    loop {
        let job = {
            let Ok(mut q) = shared.queue.lock() else {
                return;
            };
            loop {
                if q.quit {
                    return;
                }
                // Misc first (load / save are small and must not starve), then the lanes in arrival order
                if let Some(job) = q.misc.pop_front() {
                    break job;
                }
                if let Some(lane) = q.order.pop_front()
                    && let Some(job) = q.waiting.remove(&lane)
                {
                    break job;
                }
                q = match shared.wake.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        // a job made stale while it waited never runs
        let stale = shared.queue.lock().is_ok_and(|q| {
            job.lane != Lane::Misc
                && q.newest.get(&job.lane).copied().unwrap_or(0) != job.generation
        });
        if stale {
            continue;
        }
        let (lane, generation) = (job.lane, job.generation);
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (job.work)(cx)));
        match out {
            Ok(Some(done)) => {
                if let Ok(mut r) = shared.results.lock() {
                    r.push((lane, generation, done));
                }
                notify();
            }
            Ok(None) => {}
            Err(_) => qlog!("engine job on {lane:?} panicked"),
        }
    }
}
