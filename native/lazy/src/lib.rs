//! `Lazy<T>`: a static built on first use, like `std::sync::LazyLock`, whose value a few threads can give back.
//!
//! The engine keeps ~300-450 compiled regexes in statics (~19 KB each with their caches, ~5.5-9 MB in all). A
//! `LazyLock` holds them for good, which the Windows app pays for while its bar sits hidden in the tray. Here every
//! thread reads a value built once and kept, exactly as with `LazyLock`, except the (up to OWNERS) threads that
//! called [`own_here`]: the Windows app's engine thread, its UI thread and its Typst thread. Each of those gets
//! copies of its own instead, which it alone can free with [`release`] when it holds none (between jobs, or at a
//! hide); the next use builds them again. No other thread ever sees an owner's copies, so freeing them can't pull a
//! value out from under another thread. A host that never calls `own_here` (the Mac app, the tests) gets
//! `LazyLock`'s behaviour.
//!
//! An owner's copies are listed on the owner thread itself (a box pointer and its drop function), so `release`
//! never touches a `Lazy`: each `Lazy` remembers, per owner slot, the epoch its copy was built in, and a copy from an
//! earlier epoch counts as gone. Epochs only grow, across the owners a slot has had. A `Lazy` may move or drop
//! freely; a dropped one's copies go at their owners' next release.

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};

/// How many threads at once may own copies.
pub const OWNERS: usize = 4;

/// The owner slots taken.
static CLAIMED: [AtomicBool; OWNERS] = [const { AtomicBool::new(false) }; OWNERS];
/// Each slot's last epoch handed out (so a new owner of a slot never meets an older owner's copies as current).
static EPOCHS: [AtomicU64; OWNERS] = [const { AtomicU64::new(0) }; OWNERS];

/// A leaked box's address and the function that drops it.
type Built = (usize, unsafe fn(usize));

thread_local! {
    /// This thread's owner slot + 1, or 0 when it owns none.
    static SLOT: Cell<usize> = const { Cell::new(0) };
    /// The owner's current epoch.
    static EPOCH: Cell<u64> = const { Cell::new(0) };
    /// How deep the owner is in building its copies (`building`).
    static BUILDING: Cell<u32> = const { Cell::new(0) };
    /// The owner's copies built this epoch: each box's pointer and the function that drops it.
    static BUILT: RefCell<Vec<Built>> = const { RefCell::new(Vec::new()) };
}

fn next_epoch(slot: usize) -> u64 {
    EPOCHS[slot].fetch_add(1, Ordering::Relaxed) + 1
}

/// Makes this thread an owner of freeable copies, if a slot is free. True when this thread owns them.
pub fn own_here() -> bool {
    if SLOT.with(Cell::get) != 0 {
        return true;
    }
    for (i, c) in CLAIMED.iter().enumerate() {
        if c.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            EPOCH.with(|e| e.set(next_epoch(i)));
            SLOT.with(|s| s.set(i + 1));
            return true;
        }
    }
    false
}

/// Whether this thread owns freeable copies.
#[inline]
pub fn owns() -> bool {
    SLOT.try_with(Cell::get).unwrap_or(0) != 0
}

/// Whether any thread owns freeable copies (a host that frees them).
pub fn any_owner() -> bool {
    CLAIMED.iter().any(|c| c.load(Ordering::Relaxed))
}

/// Whether this thread is building one of its own freeable copies right now: a host's allocator can put what is
/// made meanwhile in a heap of its own, so freeing the copies leaves no holes among longer-lived blocks.
#[inline]
pub fn building() -> bool {
    BUILDING.try_with(|b| b.get() != 0).unwrap_or(false)
}

/// Runs `f` as if building a freeable copy (`building` is true meanwhile) when this thread is an owner: for a
/// value the owner keeps elsewhere and frees with its copies (a cache it clears at the same time).
pub fn build_scope<R>(f: impl FnOnce() -> R) -> R {
    if !owns() {
        return f();
    }
    BUILDING.with(|b| b.set(b.get() + 1));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    BUILDING.with(|b| b.set(b.get() - 1));
    out.unwrap_or_else(|e| std::panic::resume_unwind(e))
}

/// Frees this owner thread's copies of every `Lazy` value; their next use on this thread builds them again.
/// Returns how many were freed; does nothing (0) on a thread that owns none.
///
/// # Safety
/// No reference obtained from a `Lazy` on this thread may be alive across this call: call it between jobs, at the
/// top of a thread's loop or from a message handler that holds none. Other threads are unaffected: they never see
/// this thread's copies.
pub unsafe fn release() -> usize {
    let slot = SLOT.with(Cell::get);
    if slot == 0 {
        return 0;
    }
    // every Lazy's copy for this slot is stale from here on, before any is freed
    EPOCH.with(|e| e.set(next_epoch(slot - 1)));
    let built = BUILT.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let n = built.len();
    for (p, free) in built {
        // SAFETY: `p` is a box `build_owned` leaked and listed once, `free` its own type's drop; no Lazy hands it
        // out after the epoch moved on, and the caller promises no reference into it is still held
        unsafe { free(p) };
    }
    n
}

/// Frees this thread's copies and gives its owner slot up (a thread that is about to end): from here on it reads
/// the kept values, like any other thread.
///
/// # Safety
/// As for `release`.
pub unsafe fn disown() -> usize {
    // SAFETY: the caller's promise
    let n = unsafe { release() };
    let slot = SLOT.with(|s| s.replace(0));
    if slot != 0 {
        CLAIMED[slot - 1].store(false, Ordering::Release);
    }
    n
}

/// A value built on first use: `static RE: Lazy<Regex> = Lazy::new(|| ...)`.
pub struct Lazy<T> {
    init: fn() -> T,
    /// every thread but the owners: built once, kept
    shared: OnceLock<T>,
    /// each owner slot's copy (a leaked box listed in the owner's BUILT), valid while its `epoch` is the owner's
    /// epoch; only the slot's owner thread reads or writes its pair
    owned: [AtomicPtr<T>; OWNERS],
    epoch: [AtomicU64; OWNERS],
}

impl<T> Lazy<T> {
    pub const fn new(init: fn() -> T) -> Lazy<T> {
        Lazy {
            init,
            shared: OnceLock::new(),
            owned: [const { AtomicPtr::new(std::ptr::null_mut()) }; OWNERS],
            epoch: [const { AtomicU64::new(0) }; OWNERS],
        }
    }

    #[cold]
    #[inline(never)]
    fn build_owned(&self, slot: usize, epoch: u64) -> &T {
        BUILDING.with(|b| b.set(b.get() + 1));
        let built =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Box::new((self.init)())));
        BUILDING.with(|b| b.set(b.get() - 1));
        let p = Box::into_raw(built.unwrap_or_else(|e| std::panic::resume_unwind(e)));
        BUILT.with(|b| b.borrow_mut().push((p as usize, drop_box::<T>)));
        // a build that used this same Lazy would have listed its own box too (never in the engine): both are
        // freed at the next release, and this one is handed out
        self.owned[slot].store(p, Ordering::Relaxed);
        self.epoch[slot].store(epoch, Ordering::Relaxed);
        // SAFETY: just made; freed only by `release`, whose caller guarantees no reference outlives it
        unsafe { &*p }
    }
}

/// Drops one leaked `Box<T>`.
///
/// # Safety
/// `p` came from `Box::<T>::into_raw` and is dropped once, with no reference into it alive.
unsafe fn drop_box<T>(p: usize) {
    // SAFETY: the caller's promise
    drop(unsafe { Box::from_raw(p as *mut T) });
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        let slot = SLOT.with(Cell::get);
        if slot != 0 {
            let (i, epoch) = (slot - 1, EPOCH.with(Cell::get));
            if self.epoch[i].load(Ordering::Relaxed) == epoch {
                // SAFETY: this owner's copy from its current epoch, read on the owner thread: `release` moves the
                // epoch on before freeing it, and its caller guarantees no reference is alive then
                return unsafe { &*self.owned[i].load(Ordering::Relaxed) };
            }
            return self.build_owned(i, epoch);
        }
        self.shared.get_or_init(self.init)
    }
}

impl<T> std::fmt::Debug for Lazy<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lazy").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static BUILDS: AtomicUsize = AtomicUsize::new(0);
    static NAME: Lazy<String> = Lazy::new(|| {
        BUILDS.fetch_add(1, Ordering::Relaxed);
        "pi".repeat(3)
    });

    #[test]
    fn owners_copies_go_and_come_back_and_others_keep_theirs() {
        // another thread first: the shared copy
        std::thread::spawn(|| assert_eq!(&*NAME, "pipipi"))
            .join()
            .unwrap();
        assert_eq!(BUILDS.load(Ordering::Relaxed), 1);
        assert!(!any_owner());
        std::thread::spawn(|| {
            assert!(own_here() && owns());
            assert_eq!(NAME.len(), 6);
            assert_eq!(BUILDS.load(Ordering::Relaxed), 2);
            assert_eq!(NAME.len(), 6);
            assert_eq!(BUILDS.load(Ordering::Relaxed), 2);
            // a Lazy that moves and drops before the release
            let local: Lazy<Vec<u8>> = Lazy::new(|| vec![1, 2, 3]);
            assert_eq!(local.len(), 3);
            let moved = Box::new(local);
            assert_eq!(moved.len(), 3);
            drop(moved);
            // SAFETY: no reference held
            assert_eq!(unsafe { release() }, 2);
            // another owner has its own copy; a plain thread still reads the kept one
            std::thread::spawn(|| {
                assert!(own_here());
                assert_eq!(&*NAME, "pipipi");
                assert_eq!(BUILDS.load(Ordering::Relaxed), 3);
                // SAFETY: no reference held
                assert_eq!(unsafe { disown() }, 1);
                assert!(!owns());
                assert_eq!(&*NAME, "pipipi");
                assert_eq!(BUILDS.load(Ordering::Relaxed), 3);
            })
            .join()
            .unwrap();
            std::thread::spawn(|| assert_eq!(&*NAME, "pipipi"))
                .join()
                .unwrap();
            assert_eq!(&*NAME, "pipipi");
            assert_eq!(BUILDS.load(Ordering::Relaxed), 4);
            // SAFETY: no reference held
            assert_eq!(unsafe { release() }, 1);
            // SAFETY: as above
            assert_eq!(unsafe { release() }, 0);
        })
        .join()
        .unwrap();
        assert!(any_owner() && !owns());
        // SAFETY: not an owner: does nothing
        assert_eq!(unsafe { release() }, 0);
    }
}
