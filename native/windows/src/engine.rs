//! The engine thread: the one place the UI reaches the maths from, so the UI thread never evaluates.
//!
//! One long-lived thread with a 32 MB stack owns the `qcalc_ffi::Engine`, whose sync calls run right on it
//! (`run_engine_calls_here`; the Mac's engine hops each call to its own 32 MB evaluation thread, ARCHITECTURE.md
//! §5, which this thread already is). Work comes in *lanes* (`Lane`), as in the Mac's
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
//! The typeset preview's `TypstRenderer` lives on its own thread (`typst()`), so a Typst compile never delays a
//! keystroke's answer. The thread starts with the first typeset and ends when the bar hides (`stop_typst`): the
//! renderer's fonts and caches and its deep stack are ~2.5 MB the idle app needn't hold (PERFORMANCE.md).
//! typst_row.rs owns what is drawn from it.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use qcalc_ffi::{Engine, EngineConfig, EvalContext, HistoryRow, HistoryView, TypstRenderer};

use crate::app::App;
use crate::qlog;

pub const STACK_BYTES: usize = 32 * 1024 * 1024;

/// How long after `release` the engine thread frees its compiled regexes: at once (once the jobs waiting are done).
/// The wait is app.rs's: `release` is called only once the bar has stayed hidden 30 s (TRIM_AFTER_MS, a timer a
/// showing cancels), so a quick hide and show keeps them and its first key compiles nothing (PERFORMANCE.md).
const RELEASE_AFTER: std::time::Duration = std::time::Duration::ZERO;

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
    /// the graph's readout under the pointer (graph.rs)
    GraphReadout,
    /// the `sys` rows' live solution (systems.rs)
    System,
    /// the typeset row's source (typst_row.rs)
    Typeset,
    /// the input's own asks (line.rs): the squiggle after a pause, the SI prefix step, the examples' answers
    Input,
    /// a selected part of the line answered on its own (selection.rs)
    Selection,
    /// history load / save, warm-up and anything that must not be dropped: never replaced, never stale
    Misc,
}

/// The engine thread's side: the engine and its caches.
pub struct Cx {
    pub engine: Arc<Engine>,
    context: Option<ContextKey>,
    /// the warm-up read the rates file (once: a later warm-up only compiles again)
    rates_loaded: bool,
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

/// A warm-up step.
type Step = Box<dyn FnOnce(&mut Cx) + Send>;

struct Job {
    lane: Lane,
    generation: u64,
    work: Work,
}

#[derive(Default)]
struct Queue {
    waiting: HashMap<Lane, Job>,
    misc: VecDeque<Job>,
    /// the warm-up's steps, run only when nothing else waits
    idle: VecDeque<Step>,
    /// `warm_up` ran since the last release
    warmed: bool,
    /// when the regexes go (`release`), unless the bar shows first
    release_at: Option<Instant>,
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
    /// the Typst thread's queue, while it runs
    typst: Mutex<Option<Sender<TypstJob>>>,
    /// tells the UI thread a result is ready (app.rs posts WM_ENGINE)
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl EngineThread {
    /// Starts the engine thread. `notify` is called on the engine's threads whenever a result is ready (app.rs
    /// posts WM_ENGINE). The Typst thread starts with the first typeset (`typst`).
    ///
    /// Called on the UI thread, which from here on also keeps its own compiled regexes (the few the bar's own
    /// rules use), freed at a hide (`release`).
    pub fn start(config: EngineConfig, notify: impl Fn() + Send + Sync + 'static) -> EngineThread {
        qcalc_core::jsre::own_compiled_here();
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
                // this thread has the evaluation thread's 32 MB stack: the engine's calls run here, with no hop
                // to qcalc-eval and back per call (~15 us each) and one set of regex caches (PERFORMANCE.md)
                qcalc_ffi::run_engine_calls_here();
                // and its compiled regexes are its own, so `release` can free them while the bar is hidden
                qcalc_core::jsre::own_compiled_here();
                let engine = Engine::new(config);
                qlog!(
                    "engine created in {:.1} ms",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                let mut cx = Cx {
                    engine,
                    context: None,
                    rates_loaded: false,
                };
                run(&s, &mut cx, &*n);
            })
            .expect("the engine thread");
        EngineThread {
            shared,
            typst: Mutex::new(None),
            notify,
        }
    }

    /// The Typst thread, started (and its renderer made and warmed) by the first job that needs it.
    fn typst_sender(&self) -> Option<Sender<TypstJob>> {
        let mut slot = self.typst.lock().ok()?;
        if let Some(tx) = slot.as_ref() {
            return Some(tx.clone());
        }
        let (tx, rx) = channel::<TypstJob>();
        let (s, notify) = (self.shared.clone(), self.notify.clone());
        std::thread::Builder::new()
            .name("qcalc-typst".into())
            .stack_size(STACK_BYTES)
            .spawn(move || {
                let t = Instant::now();
                // Typst parallelises with rayon, whose global pool would start a thread per core (24 here, each
                // holding its stack for good) for documents this small: one does (PERFORMANCE.md)
                let _ = rayon::ThreadPoolBuilder::new()
                    .num_threads(1)
                    .thread_name(|_| "qcalc-typst-pool".into())
                    .build_global();
                // its regexes (the typeset source's) are its own, and go with it
                qcalc_core::jsre::own_compiled_here();
                let typst = TypstRenderer::new();
                let ms = typst.warm_up();
                qlog!(
                    "typst warmed in {:.1} ms (warm_up says {ms:.1} ms)",
                    t.elapsed().as_secs_f64() * 1000.0
                );
                // until stop_typst lets go of the sender: then the renderer, its caches and the stack go
                while let Ok(job) = rx.recv() {
                    if let Some(d) = job(&typst) {
                        if let Ok(mut r) = s.results.lock() {
                            r.push((Lane::Misc, 0, d));
                        }
                        notify();
                    }
                }
                // Typst's memos are process-wide: the renderer's drop alone would leave them
                typst.clear_caches();
                qcalc_core::jsre::clear_cache();
                // SAFETY: after the thread's last job: no reference from a regex static is alive on it
                unsafe { qcalc_core::jsre::disown_compiled() };
            })
            .ok()?;
        *slot = Some(tx.clone());
        Some(tx)
    }

    /// Ends the Typst thread once its queued jobs are done (the bar hid): the renderer's fonts and caches and the
    /// thread's stack (deep compiles commit ~1 MB of it) are given back; the next typeset starts it again.
    pub fn stop_typst(&self) {
        if let Ok(mut slot) = self.typst.lock() {
            *slot = None;
        }
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

    /// The engine's first evaluation is the slow one (its lazy tables and ~240 regexes, ~40 ms and ~4.8 MB): run
    /// off the UI thread when the bar shows (or at launch when it shows then), so an app started into the tray at
    /// sign-in holds none of it until it is used, and again after `release` freed the regexes. It runs in steps of
    /// a few ms, each only when no other job waits, so a key typed straight after the showing goes first.
    pub fn warm_up(&self) {
        let Ok(mut q) = self.shared.queue.lock() else {
            return;
        };
        // a showing keeps what a hide was about to free
        q.release_at = None;
        if std::mem::replace(&mut q.warmed, true) {
            return;
        }
        let t = Instant::now();
        q.idle.push_back(Box::new(|cx: &mut Cx| {
            if !std::mem::replace(&mut cx.rates_loaded, true) {
                let _ = cx.engine.reload_rates();
            }
            let _ = cx.engine.evaluate(
                vec!["2+2".into(), "sin(90)".into(), "72 f to c".into()],
                qcalc_ffi::default_eval_options(),
            );
        }));
        // and a keystroke's own path (the live answer's checks, phrases, the field's decorations), whose first
        // run cost the first keys after a showing ~2-7 ms each
        const TEXTS: [&str; 7] = ["2", "2+", "2+2", "si", "sin(9", "3x+2=1", "5 km in miles"];
        for (i, text) in TEXTS.into_iter().enumerate() {
            q.idle.push_back(Box::new(move |cx: &mut Cx| {
                let context = EvalContext::default();
                let chained = cx.engine.chains(text.into(), context.clone());
                let answer = cx.engine.live(qcalc_ffi::LiveRequest {
                    text: text.into(),
                    options: qcalc_ffi::default_eval_options(),
                    context: context.clone(),
                    chained,
                    now_ms: crate::model::now_ms(),
                    phrases: true,
                });
                let _ = crate::line::decorate(text, &context, &answer);
                if i + 1 == TEXTS.len() {
                    qlog!(
                        "engine warmed {:.1} ms after it was asked",
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }));
        }
        drop(q);
        self.shared.wake.notify_one();
    }

    /// The bar hid: after RELEASE_AFTER with no showing, the engine's compiled regexes (~6-9 MB with their caches,
    /// PERFORMANCE.md) are freed on the engine thread between jobs, and the next showing's `warm_up` compiles the
    /// keystroke path's again. A showing before the engine thread gets to it keeps them.
    ///
    /// On the UI thread, once the bar has stayed hidden 30 s (app.rs `release_hidden`), where no regex of the
    /// core's is in use: the UI thread's own compiled regexes go at once.
    pub fn release(&self) {
        qcalc_core::jsre::clear_cache();
        // SAFETY: the UI thread while hidden, outside any call into the core: it holds no reference from a regex static
        unsafe { qcalc_core::jsre::release_compiled() };
        if let Ok(mut q) = self.shared.queue.lock() {
            q.release_at = Some(Instant::now() + RELEASE_AFTER);
        }
        self.shared.wake.notify_one();
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
        if let Some(tx) = self.typst_sender() {
            let _ = tx.send(Box::new(work));
        }
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
                // then a warm-up step, then a due release
                if let Some(step) = q.idle.pop_front() {
                    break Job {
                        lane: Lane::Misc,
                        generation: 0,
                        work: Box::new(move |cx| {
                            step(cx);
                            None
                        }),
                    };
                }
                if let Some(at) = q.release_at {
                    let now = Instant::now();
                    if now >= at {
                        q.release_at = None;
                        q.warmed = false;
                        break Job {
                            lane: Lane::Misc,
                            generation: 0,
                            work: Box::new(|_| {
                                release_here();
                                None
                            }),
                        };
                    }
                    q = match shared.wake.wait_timeout(q, at - now) {
                        Ok((q, _)) => q,
                        Err(_) => return,
                    };
                    continue;
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
                if lane == Lane::Live {
                    crate::perf::mark(crate::perf::Stage::Posted);
                }
                notify();
            }
            Ok(None) => {}
            Err(_) => qlog!("engine job on {lane:?} panicked"),
        }
    }
}

/// Frees the engine's compiled regexes and their pages (`EngineThread::release`). On the engine thread, as a job of
/// its own.
fn release_here() {
    let t = Instant::now();
    qcalc_core::jsre::clear_cache();
    // SAFETY: a job of its own, run from the top of the engine thread's loop: no reference from a regex static is
    // alive on this thread, and other threads keep their own copies (qcalc-lazy)
    let n = unsafe { qcalc_core::jsre::release_compiled() };
    let kept = crate::heap::compact();
    qlog!(
        "engine released {n} compiled statics in {:.1} ms ({kept} KB of their pages kept)",
        t.elapsed().as_secs_f64() * 1000.0
    );
}
