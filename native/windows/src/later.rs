//! One-shot timers that run a closure on the UI thread, for views that need "in a moment" (a tooltip after 450 ms,
//! a card's expiry, a pause hint, the example rotation, "✓ copied" going back). Each is keyed, so arming a key
//! again replaces its timer and `cancel` drops it; nothing repeats, so nothing runs while idle.
//!
//! The closure runs through `app::poke`, outside any borrow of the app, so it may lay out, repaint and queue ops
//! as a message handler does. Built on `SetTimer` with a TIMERPROC (no window needed), which the message loop's
//! `DispatchMessage` calls on this thread.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::app::App;

enum Job {
    /// run on the app (inside `app::poke`)
    App(Box<dyn FnOnce(&mut App)>),
    /// run as is, with nothing borrowed: for Win32 calls that send messages back (creating and moving windows)
    Free(Box<dyn FnOnce()>),
}

thread_local! {
    /// timer id → (key, job)
    static TIMERS: RefCell<HashMap<usize, (&'static str, Job)>> = RefCell::new(HashMap::new());
}

/// Runs `f` on the app in `ms`, replacing any timer armed under `key`.
pub fn after(key: &'static str, ms: u32, f: impl FnOnce(&mut App) + 'static) {
    arm(key, ms, Job::App(Box::new(f)));
}

/// Runs `f` in `ms` (at the next turn of the loop for 0) outside any borrow of the app.
pub fn free(key: &'static str, ms: u32, f: impl FnOnce() + 'static) {
    arm(key, ms, Job::Free(Box::new(f)));
}

fn arm(key: &'static str, ms: u32, job: Job) {
    cancel(key);
    // SAFETY: a thread timer with our own static callback; it is killed when it fires or is cancelled
    let id = unsafe { SetTimer(None, 0, ms.max(1), Some(fire)) };
    if id == 0 {
        return;
    }
    TIMERS.with(|t| t.borrow_mut().insert(id, (key, job)));
}

/// Drops the timer armed under `key`, if any.
pub fn cancel(key: &'static str) {
    let ids: Vec<usize> = TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        let ids: Vec<usize> = t
            .iter()
            .filter(|(_, (k, _))| *k == key)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            t.remove(id);
        }
        ids
    });
    for id in ids {
        // SAFETY: a thread timer we made
        let _ = unsafe { KillTimer(None, id) };
    }
}

/// Whether a timer is armed under `key`.
pub fn armed(key: &'static str) -> bool {
    TIMERS.with(|t| t.borrow().values().any(|(k, _)| *k == key))
}

unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
    // SAFETY: a thread timer we made
    let _ = unsafe { KillTimer(None, id) };
    let job = TIMERS.with(|t| t.borrow_mut().remove(&id));
    match job {
        Some((_, Job::App(f))) => crate::app::poke(f),
        Some((_, Job::Free(f))) => f(),
        None => {}
    }
}
