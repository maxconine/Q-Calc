//! Background work's way back to the UI thread: the rates fetch, the update loop and the usage post run on their
//! own threads and hand results over as closures (`post`), run on the UI thread with the app (`run`, from the
//! host window's handler for the registered message `QCalc.Background`). Like the engine's `Done`, but for
//! threads that aren't the engine.
//!
//! Also `start`: the one place the app's background jobs begin, lazily, once the first frame is out (app.rs
//! `warm`), never on the launch path.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
use windows::core::w;

use crate::app::App;

type Job = Box<dyn FnOnce(&mut App) + Send>;

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static HOST: AtomicUsize = AtomicUsize::new(0);
static MSG: AtomicU32 = AtomicU32::new(0);

/// The registered message the host hears for `run` (0 before `start`).
pub fn message() -> u32 {
    MSG.load(Ordering::Relaxed)
}

/// Whether a host message is ours.
pub fn is_ours(msg: u32) -> bool {
    msg != 0 && msg == message()
}

/// From any thread: `f` runs on the UI thread with the app, soon.
pub fn post(f: impl FnOnce(&mut App) + Send + 'static) {
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.push(Box::new(f));
    }
    let host = HOST.load(Ordering::Relaxed);
    if host != 0 {
        // SAFETY: posting to our host window from a worker thread
        let _ =
            unsafe { PostMessageW(Some(HWND(host as *mut _)), message(), WPARAM(0), LPARAM(0)) };
    }
}

/// On the UI thread (the host's handler): the waiting jobs.
pub fn run(app: &mut App) {
    let jobs = match JOBS.lock() {
        Ok(mut j) => std::mem::take(&mut *j),
        Err(_) => return,
    };
    for job in jobs {
        job(app);
    }
}

/// Spawns a small background thread (these only wait on the network and on files).
pub fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name(name.into())
        .stack_size(512 * 1024)
        .spawn(f);
}

/// Once, after the first frame: the rates, the usage counts and the update loop.
pub fn start(app: &mut App) {
    if HOST.load(Ordering::Relaxed) != 0 {
        return;
    }
    // SAFETY: plain registration
    MSG.store(
        unsafe { RegisterWindowMessageW(w!("QCalc.Background")) },
        Ordering::Relaxed,
    );
    HOST.store(app.host.0 as usize, Ordering::Relaxed);
    crate::analytics::start(app);
    crate::rates::start();
    crate::update::start(app);
}

/// A function for `Op::Run`: window work a feature does once the app's borrow has ended. Never equal to another
/// (function addresses aren't unique), which only matters to tests comparing ops.
#[derive(Clone, Copy)]
pub struct Later(pub fn());

impl PartialEq for Later {
    fn eq(&self, _: &Later) -> bool {
        false
    }
}

impl std::fmt::Debug for Later {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Later")
    }
}
