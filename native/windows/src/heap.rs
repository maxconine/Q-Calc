//! The global allocator: the process heap, except for the engine's freeable regexes, which get pages of their own.
//!
//! The engine thread compiles its regexes (qcalc-lazy's owned copies) while it builds longer-lived things too (the
//! units table, the history's context), and frees them when the bar hides (engine.rs `release`). In one heap the
//! freed blocks leave holes in pages that stay committed (~2 MB after a session, PERFORMANCE.md). So while the
//! engine thread is building one of them (`qcalc_core::jsre::building_compiled`), small blocks come from a slab
//! allocator of our own instead: one 64 MB reservation cut into 64 KB slabs, each slab holding blocks of one size.
//! A slab whose blocks are all freed goes back to the pool, and `compact` (after the release) decommits the empty
//! ones, so the regexes' memory goes back to the system whole.
//!
//! A block is told apart by its address: only the reservation holds the slabs' blocks. A block a slab block is
//! reallocated into grows there while it fits a size class (a regex's search caches), so it goes with the regex.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_DECOMMIT, MEM_RESERVE, PAGE_NOACCESS, PAGE_READWRITE, VirtualAlloc, VirtualFree,
};

const RESERVE: usize = 64 << 20;
const SLAB: usize = 64 << 10;
const SLABS: usize = RESERVE / SLAB;
/// The size classes (multiples of 16, so every block is 16-aligned like HeapAlloc's); bigger blocks come from the
/// process heap.
const CLASSES: [usize; 40] = [
    16, 32, 48, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 640, 768, 896, 1024,
    1280, 1536, 1792, 2048, 2560, 3072, 3584, 4096, 5120, 6144, 7168, 8192, 10240, 12288, 14336,
    16384, 20480, 24576, 28672, 32768,
];
const BIG: usize = CLASSES[CLASSES.len() - 1];
const ALIGN: usize = 16;
const PAGE: usize = 4096;
/// A slab's class when it holds no blocks.
const EMPTY: u8 = u8::MAX;
const NONE: u16 = u16::MAX;

/// The reservation's start and end (0 / 0 before it's made; START 1 when it can't be).
static START: AtomicUsize = AtomicUsize::new(0);
static END: AtomicUsize = AtomicUsize::new(0);

struct Slabs {
    base: usize,
    class: [u8; SLABS],
    /// blocks handed out
    used: [u16; SLABS],
    /// the head of the slab's list of freed blocks (0: none); each freed block holds the next one's address
    free: [usize; SLABS],
    /// where the slab's never-used blocks start
    bump: [u32; SLABS],
    /// how much of the slab is committed, from its start (a page at a time, as its blocks are first handed out)
    committed: [u32; SLABS],
    /// the slab each class takes blocks from first
    current: [u16; CLASSES.len()],
}

static SLABS_STATE: Mutex<Slabs> = Mutex::new(Slabs {
    base: 0,
    class: [EMPTY; SLABS],
    used: [0; SLABS],
    free: [0; SLABS],
    bump: [0; SLABS],
    committed: [0; SLABS],
    current: [NONE; CLASSES.len()],
});

fn class_of(size: usize) -> usize {
    CLASSES.partition_point(|&c| c < size.max(1))
}

impl Slabs {
    fn has_room(&self, s: usize, size: usize) -> bool {
        self.free[s] != 0 || self.bump[s] as usize + size <= SLAB
    }

    /// A slab for class `c` with room, taking an empty one when none has; None when the reservation is full.
    fn slab_for(&mut self, c: usize) -> Option<usize> {
        let size = CLASSES[c];
        let cur = self.current[c];
        if cur != NONE && self.class[cur as usize] == c as u8 && self.has_room(cur as usize, size) {
            return Some(cur as usize);
        }
        let s = (0..SLABS)
            .find(|&s| self.class[s] == c as u8 && self.has_room(s, size))
            .or_else(|| {
                // an empty slab, one still committed first
                let s = (0..SLABS)
                    .find(|&s| self.class[s] == EMPTY && self.committed[s] != 0)
                    .or_else(|| (0..SLABS).find(|&s| self.class[s] == EMPTY))?;
                self.class[s] = c as u8;
                self.used[s] = 0;
                self.free[s] = 0;
                self.bump[s] = 0;
                Some(s)
            })?;
        self.current[c] = s as u16;
        Some(s)
    }

    fn alloc(&mut self, size: usize) -> *mut u8 {
        if self.base == 0 {
            // SAFETY: a fresh reservation, no access until a slab is committed
            let p = unsafe { VirtualAlloc(None, RESERVE, MEM_RESERVE, PAGE_NOACCESS) };
            if p.is_null() {
                START.store(1, Ordering::Relaxed);
                return std::ptr::null_mut();
            }
            self.base = p as usize;
            END.store(self.base + RESERVE, Ordering::Relaxed);
            START.store(self.base, Ordering::Release);
        }
        let c = class_of(size);
        let Some(s) = self.slab_for(c) else {
            return std::ptr::null_mut();
        };
        let p = if self.free[s] != 0 {
            let p = self.free[s];
            // SAFETY: a freed block of this slab holds the next one's address
            self.free[s] = unsafe { *(p as *const usize) };
            p
        } else {
            let at = self.bump[s] as usize;
            let end = at + CLASSES[c];
            if end > self.committed[s] as usize {
                let from = self.committed[s] as usize;
                let to = end.next_multiple_of(PAGE);
                // SAFETY: pages of our own reservation
                let p = unsafe {
                    VirtualAlloc(
                        Some((self.base + s * SLAB + from) as *const _),
                        to - from,
                        MEM_COMMIT,
                        PAGE_READWRITE,
                    )
                };
                if p.is_null() {
                    return std::ptr::null_mut();
                }
                self.committed[s] = to as u32;
            }
            self.bump[s] = end as u32;
            self.base + s * SLAB + at
        };
        self.used[s] += 1;
        p as *mut u8
    }

    fn dealloc(&mut self, p: *mut u8) {
        let s = (p as usize - self.base) / SLAB;
        // SAFETY: the block is ours and freed: it holds the list's next address now
        unsafe { *(p as *mut usize) = self.free[s] };
        self.free[s] = p as usize;
        self.used[s] -= 1;
        if self.used[s] == 0 {
            // back to the pool (still committed until `compact`)
            self.class[s] = EMPTY;
            self.free[s] = 0;
            self.bump[s] = 0;
        }
    }

    fn size_of(&self, p: *mut u8) -> usize {
        CLASSES[self.class[(p as usize - self.base) / SLAB] as usize]
    }
}

#[inline]
fn ours(p: *mut u8) -> bool {
    let a = p as usize;
    a >= START.load(Ordering::Relaxed) && a < END.load(Ordering::Relaxed)
}

/// Whether this block comes from the slabs.
#[inline]
fn wanted(l: &Layout) -> bool {
    l.size() <= BIG && l.align() <= ALIGN && qcalc_core::jsre::building_compiled()
}

#[cold]
fn slab_alloc(l: Layout, zeroed: bool) -> *mut u8 {
    let p = match SLABS_STATE.lock() {
        Ok(mut s) if START.load(Ordering::Relaxed) != 1 => s.alloc(l.size()),
        _ => std::ptr::null_mut(),
    };
    if p.is_null() {
        // SAFETY: a layout the caller made
        return unsafe {
            if zeroed {
                System.alloc_zeroed(l)
            } else {
                System.alloc(l)
            }
        };
    }
    if zeroed {
        // SAFETY: the block holds at least l.size() bytes
        unsafe { std::ptr::write_bytes(p, 0, l.size()) };
    }
    p
}

fn slab_free(p: *mut u8) {
    if let Ok(mut s) = SLABS_STATE.lock() {
        s.dealloc(p);
    }
}

/// Decommits the empty slabs (after the engine freed its regexes); the slabs' committed KB after.
pub fn compact() -> usize {
    let Ok(mut s) = SLABS_STATE.lock() else {
        return 0;
    };
    let mut kept = 0;
    for i in 0..SLABS {
        if s.committed[i] != 0 && s.class[i] == EMPTY {
            // SAFETY: an empty slab of our reservation: nothing in it is in use
            if unsafe { VirtualFree((s.base + i * SLAB) as *mut _, SLAB, MEM_DECOMMIT) }.is_ok() {
                s.committed[i] = 0;
            }
        }
        kept += s.committed[i] as usize >> 10;
    }
    kept
}

pub struct Routed;

// SAFETY: every block goes back to where it came from, told apart by address (`ours`)
unsafe impl GlobalAlloc for Routed {
    #[inline]
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if wanted(&l) {
            return slab_alloc(l, false);
        }
        // SAFETY: as the caller promised
        unsafe { System.alloc(l) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if wanted(&l) {
            return slab_alloc(l, true);
        }
        // SAFETY: as the caller promised
        unsafe { System.alloc_zeroed(l) }
    }

    #[inline]
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if ours(p) {
            slab_free(p);
            return;
        }
        // SAFETY: as the caller promised
        unsafe { System.dealloc(p, l) }
    }

    #[inline]
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if !ours(p) {
            // SAFETY: as the caller promised
            return unsafe { System.realloc(p, l, n) };
        }
        let have = SLABS_STATE.lock().map_or(0, |s| s.size_of(p));
        if n <= have {
            return p;
        }
        // a slab block grows in the slabs while it fits a class (a regex's caches go with the regex)
        // SAFETY: the layout of the grown block
        let nl = unsafe { Layout::from_size_align_unchecked(n, l.align()) };
        let q = if n <= BIG {
            slab_alloc(nl, false)
        } else {
            // SAFETY: as above
            unsafe { System.alloc(nl) }
        };
        if !q.is_null() {
            // SAFETY: both hold at least l.size() bytes, and they're different blocks
            unsafe { std::ptr::copy_nonoverlapping(p, q, l.size().min(n)) };
            slab_free(p);
        }
        q
    }
}

/// The slabs' reservation in MB (0 when unused), for the log.
pub fn reserved_mb() -> usize {
    (END.load(Ordering::Relaxed)
        .saturating_sub(START.load(Ordering::Relaxed)))
        >> 20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_fit_and_stay_aligned() {
        assert_eq!(CLASSES[class_of(1)], 16);
        assert_eq!(CLASSES[class_of(16)], 16);
        assert_eq!(CLASSES[class_of(17)], 32);
        assert_eq!(CLASSES[class_of(BIG)], BIG);
        assert!(CLASSES.iter().all(|c| c % ALIGN == 0));
        assert!(CLASSES.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn slabs_hand_out_reuse_and_give_back() {
        let mut s = SLABS_STATE.lock().unwrap();
        let a = s.alloc(24);
        let b = s.alloc(24);
        assert!(ours(a) && ours(b));
        assert_eq!(b as usize - a as usize, 32);
        assert_eq!(s.size_of(a), 32);
        s.dealloc(a);
        // the freed block comes back first
        assert_eq!(s.alloc(30), a);
        let big = s.alloc(BIG);
        assert_eq!(s.size_of(big), BIG);
        s.dealloc(a);
        s.dealloc(b);
        s.dealloc(big);
        assert!(s.class.iter().all(|&c| c == EMPTY));
        drop(s);
        assert_eq!(compact(), 0);
    }
}
