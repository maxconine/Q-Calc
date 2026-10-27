//! `Lazy<T>`: a static built on first use, like `std::sync::LazyLock`, whose value one thread can give back.
//!
//! The engine keeps ~300-450 compiled regexes in statics (~19 KB each with their caches, ~5.5-9 MB in all). A
//! `LazyLock` holds them for good, which the Windows app pays for while its bar sits hidden in the tray. Here every
//! thread but one reads a value built once and kept, exactly as with `LazyLock`. The one thread that called
//! [`own_here`] (the Windows app's engine thread, where nearly all evaluation runs) gets copies of its own instead,
//! which it alone can free with [`release`] between jobs; the next use builds them again. No other thread ever sees
//! those copies, so freeing them can't pull a value out from under another thread. A host that never calls
//! `own_here` (the Mac app, the tests) gets `LazyLock`'s behaviour.
//!
//! The owner's copies are listed on the owner thread itself (a box pointer and its drop function), so `release`
//! never touches a `Lazy`: each `Lazy` remembers the release epoch its copy was built in, and a copy from an
//! earlier epoch counts as gone. A `Lazy` may move or drop freely; a dropped one's copy goes at the next release.

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};

/// Whether the owning thread has been claimed (one thread, ever, may own the freeable copies).
static CLAIMED: AtomicBool = AtomicBool::new(false);

/// A leaked box's address and the function that drops it.
type Built = (usize, unsafe fn(usize));

thread_local! {
    /// 0 on every thread but the owner; on the owner, the current release epoch (from 1).
    static EPOCH: Cell<u64> = const { Cell::new(0) };
    /// How deep the owner is in building its copies (`building`).
    static BUILDING: Cell<u32> = const { Cell::new(0) };
    /// The owner's copies built this epoch: each box's pointer and the function that drops it.
    static BUILT: RefCell<Vec<Built>> = const { RefCell::new(Vec::new()) };
}

/// Makes this thread the owner of the freeable copies, if no thread is yet. True when this thread owns them.
pub fn own_here() -> bool {
    if EPOCH.with(Cell::get) != 0 {
        return true;
    }
    if CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        EPOCH.with(|e| e.set(1));
        return true;
    }
    false
}

/// Whether this thread is building one of its own freeable copies right now: a host's allocator can put what is
/// made meanwhile in a heap of its own, so freeing the copies leaves no holes among longer-lived blocks.
#[inline]
pub fn building() -> bool {
    BUILDING.try_with(|b| b.get() != 0).unwrap_or(false)
}

/// Runs `f` as if building a freeable copy (`building` is true meanwhile) when this thread is the owner: for a
/// value the owner keeps elsewhere and frees with its copies (a cache it clears at the same time).
pub fn build_scope<R>(f: impl FnOnce() -> R) -> R {
    if EPOCH.with(Cell::get) == 0 {
        return f();
    }
    BUILDING.with(|b| b.set(b.get() + 1));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    BUILDING.with(|b| b.set(b.get() - 1));
    out.unwrap_or_else(|e| std::panic::resume_unwind(e))
}

/// Whether some other thread owns the freeable copies (so this one reads the kept ones).
pub fn owned_elsewhere() -> bool {
    CLAIMED.load(Ordering::Relaxed) && EPOCH.with(Cell::get) == 0
}

/// Frees the owner thread's copies of every `Lazy` value; their next use on this thread builds them again. Returns
/// how many were freed; does nothing (0) on any other thread.
///
/// # Safety
/// No reference obtained from a `Lazy` on this thread may be alive across this call: call it between jobs, at the
/// top of a thread's loop, never from code that may hold one. Other threads are unaffected: they never see the
/// owner's copies.
pub unsafe fn release() -> usize {
    let epoch = EPOCH.with(Cell::get);
    if epoch == 0 {
        return 0;
    }
    // every Lazy's copy is stale from here on, before any is freed
    EPOCH.with(|e| e.set(epoch + 1));
    let built = BUILT.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let n = built.len();
    for (p, free) in built {
        // SAFETY: `p` is a box `build_owned` leaked and listed once, `free` its own type's drop; no Lazy hands it
        // out after the epoch moved on, and the caller promises no reference into it is still held
        unsafe { free(p) };
    }
    n
}

/// A value built on first use: `static RE: Lazy<Regex> = Lazy::new(|| ...)`.
pub struct Lazy<T> {
    init: fn() -> T,
    /// every thread but the owner: built once, kept
    shared: OnceLock<T>,
    /// the owner thread's copy (a leaked box listed in its BUILT), valid while `epoch` is the owner's epoch; only
    /// the owner thread reads or writes these two
    owned: AtomicPtr<T>,
    epoch: AtomicU64,
}

impl<T> Lazy<T> {
    pub const fn new(init: fn() -> T) -> Lazy<T> {
        Lazy {
            init,
            shared: OnceLock::new(),
            owned: AtomicPtr::new(std::ptr::null_mut()),
            epoch: AtomicU64::new(0),
        }
    }

    #[cold]
    #[inline(never)]
    fn build_owned(&self, epoch: u64) -> &T {
        BUILDING.with(|b| b.set(b.get() + 1));
        let built =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Box::new((self.init)())));
        BUILDING.with(|b| b.set(b.get() - 1));
        let p = Box::into_raw(built.unwrap_or_else(|e| std::panic::resume_unwind(e)));
        BUILT.with(|b| b.borrow_mut().push((p as usize, drop_box::<T>)));
        // a build that used this same Lazy would have listed its own box too (never in the engine): both are
        // freed at the next release, and this one is handed out
        self.owned.store(p, Ordering::Relaxed);
        self.epoch.store(epoch, Ordering::Relaxed);
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
        let epoch = EPOCH.with(Cell::get);
        if epoch != 0 {
            if self.epoch.load(Ordering::Relaxed) == epoch {
                // SAFETY: the owner's copy from this epoch, read on the owner thread: `release` moves the epoch
                // on before freeing it, and its caller guarantees no reference is alive then
                return unsafe { &*self.owned.load(Ordering::Relaxed) };
            }
            return self.build_owned(epoch);
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
    fn the_owners_copies_go_and_come_back_and_others_keep_theirs() {
        // another thread first: the shared copy
        std::thread::spawn(|| assert_eq!(&*NAME, "pipipi"))
            .join()
            .unwrap();
        assert_eq!(BUILDS.load(Ordering::Relaxed), 1);
        std::thread::spawn(|| {
            assert!(own_here());
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
            // another thread still reads its copy
            std::thread::spawn(|| assert_eq!(&*NAME, "pipipi"))
                .join()
                .unwrap();
            assert_eq!(&*NAME, "pipipi");
            assert_eq!(BUILDS.load(Ordering::Relaxed), 3);
            // SAFETY: no reference held
            assert_eq!(unsafe { release() }, 1);
            // SAFETY: as above
            assert_eq!(unsafe { release() }, 0);
        })
        .join()
        .unwrap();
        // one owner, ever
        assert!(!own_here());
        // SAFETY: not the owner: does nothing
        assert_eq!(unsafe { release() }, 0);
    }
}
