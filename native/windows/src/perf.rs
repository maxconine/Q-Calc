//! Performance diagnostics (native/windows/PERFORMANCE.md), all behind the `perf` cargo feature; without it every
//! function here is an empty inline, so a shipping build carries none of it.
//!
//! - `mark(Stage)`: a keystroke's timeline (WM_CHAR, the engine thread's steps, WM_ENGINE, layout, EndDraw), read
//!   back by the bench at the paint that shows the answer (`take`), so a run reports where each keystroke's time
//!   goes.
//! - a counting global allocator (`Counting`): the Rust heap's live bytes and allocations.
//! - `cmd`: the bench pipe's `"vm"` (the address space as VMMap counts it: committed private memory by region,
//!   resident pages, the process heaps' summaries, the Rust heap) and `"trace"`.

/// The points of a keystroke's path, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    /// the UI thread took WM_CHAR
    Char,
    /// the live job was queued (end of the field's own handling)
    Submitted,
    /// the first paint, of the typed text, ended
    TextPainted,
    /// the engine thread picked the job up
    JobStart,
    /// the history context was ready
    Context,
    /// `chains`
    Chains,
    /// `live`
    Live,
    /// the closed form's job and the decorations
    Decorated,
    /// the result was posted
    Posted,
    /// WM_ENGINE reached the UI thread
    Engine,
    /// the result was applied to the model
    Applied,
    /// `Bar::layout` after it
    Laidout,
    /// the answer's paint began
    PaintStart,
    /// EndDraw
    EndDraw,
    /// the bench posted WM_CHAR (its t0)
    Post,
    /// the bench saw the answer painted (its t1)
    Seen,
}

pub const STAGES: usize = 16;

#[cfg(feature = "perf")]
mod on {
    use super::{STAGES, Stage};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Instant;

    static BASE: OnceLock<Instant> = OnceLock::new();
    static MARKS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];

    fn now_ns() -> u64 {
        BASE.get_or_init(Instant::now).elapsed().as_nanos() as u64
    }

    pub fn mark(s: Stage) {
        MARKS[s as usize].store(now_ns(), Ordering::Relaxed);
    }

    fn is_set(s: Stage) -> bool {
        MARKS[s as usize].load(Ordering::Relaxed) != 0
    }

    /// Before a paint: the answer's paint starts once the result was applied.
    pub fn paint_start() {
        if is_set(Stage::Applied) {
            mark(Stage::PaintStart);
        }
    }

    /// After a paint: the text's first frame, or the answer's.
    pub fn painted() {
        if is_set(Stage::Applied) {
            mark(Stage::EndDraw);
        } else if is_set(Stage::Char) && !is_set(Stage::TextPainted) {
            mark(Stage::TextPainted);
        }
    }

    /// A step's start: clears the marks, and marks `Post`.
    pub fn reset() {
        for m in &MARKS {
            m.store(0, Ordering::Relaxed);
        }
        mark(Stage::Post);
    }

    /// The marks in µs since `Post` (None where a stage didn't happen).
    pub fn take() -> [Option<f64>; STAGES] {
        mark(Stage::Seen);
        let base = MARKS[Stage::Post as usize].load(Ordering::Relaxed);
        let mut out = [None; STAGES];
        for (i, m) in MARKS.iter().enumerate() {
            let v = m.load(Ordering::Relaxed);
            if v != 0 && base != 0 && v >= base {
                out[i] = Some((v - base) as f64 / 1000.0);
            }
        }
        out
    }

    pub struct Counting;
    pub static LIVE: AtomicUsize = AtomicUsize::new(0);
    pub static PEAK: AtomicUsize = AtomicUsize::new(0);
    pub static COUNT: AtomicUsize = AtomicUsize::new(0);
    pub static TOTAL: AtomicUsize = AtomicUsize::new(0);

    /// Net bytes by the thread that allocated or freed them (a string made on one thread and dropped on another
    /// moves between them), by thread name.
    pub static BY_THREAD: [std::sync::atomic::AtomicIsize; 32] =
        [const { std::sync::atomic::AtomicIsize::new(0) }; 32];
    pub static THREAD_NAMES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    thread_local! {
        static SLOT: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
    }

    fn slot() -> usize {
        let s = SLOT.with(|s| s.get());
        if s != usize::MAX {
            return s;
        }
        if INSIDE.with(|i| i.replace(true)) {
            return 31;
        }
        let name = std::thread::current().name().unwrap_or("?").to_string();
        let i = THREAD_NAMES.lock().map_or(31, |mut v| {
            if let Some(i) = v.iter().position(|n| *n == name) {
                i
            } else if v.len() < 31 {
                v.push(name);
                v.len() - 1
            } else {
                31
            }
        });
        INSIDE.with(|x| x.set(false));
        SLOT.with(|s| s.set(i));
        i
    }

    fn by_thread(n: isize) {
        BY_THREAD[slot()].fetch_add(n, Ordering::Relaxed);
    }

    fn add(n: usize) {
        by_thread(n as isize);
        let live = LIVE.fetch_add(n, Ordering::Relaxed) + n;
        PEAK.fetch_max(live, Ordering::Relaxed);
        COUNT.fetch_add(1, Ordering::Relaxed);
        TOTAL.fetch_add(n, Ordering::Relaxed);
    }

    /// Live allocations of at least `BIG` bytes, with where they were made (QCALC_PERF_BIG=1).
    const BIG: usize = 64 * 1024;
    pub static BIGS: std::sync::Mutex<Vec<(usize, usize, String)>> = std::sync::Mutex::new(Vec::new());
    static TRACK: OnceLock<bool> = OnceLock::new();
    thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn track_big(p: *mut u8, n: usize) {
        if n < BIG || p.is_null() || !*TRACK.get_or_init(|| std::env::var_os("QCALC_PERF_BIG").is_some()) {
            return;
        }
        if INSIDE.with(|i| i.replace(true)) {
            return;
        }
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        let frames: Vec<&str> = bt
            .lines()
            .filter(|l| l.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()))
            .map(|l| l.split_once(": ").map_or(l, |(_, f)| f))
            .filter(|f| {
                !f.starts_with("std::")
                    && !f.starts_with("alloc::")
                    && !f.starts_with("core::")
                    && !f.contains("perf::on")
                    && !f.starts_with("hashbrown")
                    && !f.contains("__rust")
            })
            .take(8)
            .collect();
        if let Ok(mut b) = BIGS.lock() {
            b.push((p as usize, n, frames.join(" < ")));
        }
        INSIDE.with(|i| i.set(false));
    }

    fn untrack_big(p: *mut u8, n: usize) {
        if n < BIG || INSIDE.with(|i| i.get()) {
            return;
        }
        INSIDE.with(|i| i.set(true));
        if let Ok(mut b) = BIGS.lock() {
            b.retain(|e| e.0 != p as usize);
        }
        INSIDE.with(|i| i.set(false));
    }

    // SAFETY: forwards to System, only counting
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            add(l.size());
            // SAFETY: as the caller promised
            let p = unsafe { System.alloc(l) };
            track_big(p, l.size());
            p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            add(l.size());
            // SAFETY: as the caller promised
            let p = unsafe { System.alloc_zeroed(l) };
            track_big(p, l.size());
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
            by_thread(-(l.size() as isize));
            untrack_big(p, l.size());
            // SAFETY: as the caller promised
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
            by_thread(-(l.size() as isize));
            untrack_big(p, l.size());
            add(n);
            // SAFETY: as the caller promised
            let q = unsafe { System.realloc(p, l, n) };
            track_big(q, n);
            q
        }
    }
}

#[cfg(feature = "perf")]
pub use on::{Counting, mark, paint_start, painted, reset, take};

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn mark(_: Stage) {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn paint_start() {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn painted() {}

#[cfg(not(feature = "perf"))]
#[inline(always)]
pub fn reset() {}

#[cfg(not(feature = "perf"))]
pub fn take() -> [Option<f64>; STAGES] {
    [None; STAGES]
}

/// Times a showing (Op::Show, from prepare_show to the uncloak) into `show_ms` (perf builds only).
pub struct ShowTimer(pub std::time::Instant);

static SHOWS: std::sync::Mutex<Vec<f64>> = std::sync::Mutex::new(Vec::new());

impl Drop for ShowTimer {
    fn drop(&mut self) {
        if cfg!(feature = "perf")
            && let Ok(mut s) = SHOWS.lock()
        {
            s.push(self.0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

/// The bench pipe's diagnostics commands.
#[cfg(windows)]
pub fn cmd(cmd: &str) -> Option<serde_json::Value> {
    match cmd {
        #[cfg(feature = "perf")]
        "vm" => Some(vm()),
        #[cfg(feature = "perf")]
        "threads" => Some(threads()),
        "shows" => Some(serde_json::json!(SHOWS.lock().map(|s| s.clone()).unwrap_or_default())),
        _ => None,
    }
}

/// The process's threads by name (SetThreadDescription, which Rust's names use), unnamed ones counted.
#[cfg(all(windows, feature = "perf"))]
fn threads() -> serde_json::Value {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows::Win32::System::Threading::{
        GetThreadDescription, OpenThread, THREAD_QUERY_INFORMATION,
        THREAD_QUERY_LIMITED_INFORMATION,
    };
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationThread(
            h: windows::Win32::Foundation::HANDLE,
            class: u32,
            info: *mut std::ffi::c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
    }
    /// The module a thread started in (ThreadQuerySetWin32StartAddress).
    fn start_module(h: windows::Win32::Foundation::HANDLE) -> Option<String> {
        use windows::Win32::System::LibraryLoader::{
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            GetModuleFileNameW, GetModuleHandleExW,
        };
        let mut addr = 0usize;
        // SAFETY: out pointers are live locals; the module handle isn't kept
        unsafe {
            if NtQueryInformationThread(h, 9, (&mut addr as *mut usize).cast(), 8, std::ptr::null_mut()) != 0 {
                return None;
            }
            let mut m = windows::Win32::Foundation::HMODULE::default();
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                windows::core::PCWSTR(addr as *const u16),
                &mut m,
            )
            .ok()?;
            let mut buf = [0u16; 260];
            let n = GetModuleFileNameW(Some(m), &mut buf) as usize;
            let path = String::from_utf16_lossy(&buf[..n]);
            Some(path.rsplit('\\').next().unwrap_or("").to_string())
        }
    }
    let pid = std::process::id();
    let mut names: Vec<String> = Vec::new();
    // SAFETY: a snapshot walked and closed here; each thread handle is closed after its query
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return serde_json::json!({});
        };
        let mut e = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut ok = Thread32First(snap, &mut e).is_ok();
        while ok {
            if e.th32OwnerProcessID == pid {
                let name = OpenThread(
                    THREAD_QUERY_LIMITED_INFORMATION | THREAD_QUERY_INFORMATION,
                    false,
                    e.th32ThreadID,
                )
                .ok()
                .and_then(|h| {
                    let d = GetThreadDescription(h)
                        .ok()
                        .and_then(|p| p.to_string().ok())
                        .filter(|d| !d.is_empty())
                        .or_else(|| start_module(h).map(|m| format!("({m})")));
                    let _ = CloseHandle(h);
                    d
                })
                .unwrap_or_else(|| "(unnamed)".into());
                names.push(name);
            }
            ok = Thread32Next(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    names.sort();
    serde_json::json!({ "count": names.len(), "names": names })
}

/// The address space, VMMap's way: committed memory by type, private regions by allocation base with what's
/// resident, the heaps, the Rust heap.
#[cfg(all(windows, feature = "perf"))]
fn vm() -> serde_json::Value {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use windows::Win32::System::Memory::{
        GetProcessHeaps, HEAP_SUMMARY, HeapSummary, MEM_COMMIT, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE,
        MEMORY_BASIC_INFORMATION, PAGE_GUARD, VirtualQuery,
    };
    use windows::Win32::System::ProcessStatus::{
        K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut heaps = vec![windows::Win32::Foundation::HANDLE::default(); 64];
    // SAFETY: a live buffer of the length passed
    let n = unsafe { GetProcessHeaps(&mut heaps) } as usize;
    heaps.truncate(n.min(64));
    let heap_info: Vec<serde_json::Value> = heaps
        .iter()
        .map(|h| {
            let mut s = HEAP_SUMMARY {
                cb: std::mem::size_of::<HEAP_SUMMARY>() as u32,
                ..Default::default()
            };
            // SAFETY: a heap handle from GetProcessHeaps; s is live
            let _ = unsafe { HeapSummary(*h, 0, &mut s) };
            serde_json::json!({ "base": format!("{:x}", h.0 as usize), "allocated": s.cbAllocated,
                "committed": s.cbCommitted, "reserved": s.cbReserved })
        })
        .collect();
    let heap_bases: Vec<usize> = heaps.iter().map(|h| h.0 as usize).collect();

    // regions
    let mut by_base: HashMap<usize, (usize, usize, bool, usize)> = HashMap::new(); // committed, reserved, guard, resident
    let mut totals: HashMap<&str, usize> = HashMap::new();
    let mut addr = 0usize;
    let page = 4096usize;
    loop {
        let mut m = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: plain query into a live struct
        let got = unsafe {
            VirtualQuery(
                Some(addr as *const _),
                &mut m,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if got == 0 {
            break;
        }
        let base = m.AllocationBase as usize;
        let size = m.RegionSize;
        let kind = if m.Type == MEM_PRIVATE {
            "private"
        } else if m.Type == MEM_MAPPED {
            "mapped"
        } else if m.Type == MEM_IMAGE {
            "image"
        } else {
            "free"
        };
        if m.State == MEM_COMMIT {
            *totals.entry(kind).or_default() += size;
        }
        if m.Type == MEM_PRIVATE {
            let e = by_base.entry(base).or_default();
            e.1 += size;
            if m.State == MEM_COMMIT {
                e.0 += size;
                if m.Protect.0 & PAGE_GUARD.0 != 0 {
                    e.2 = true;
                }
                // resident pages
                let pages = size / page;
                let mut info: Vec<PSAPI_WORKING_SET_EX_INFORMATION> = (0..pages)
                    .map(|i| PSAPI_WORKING_SET_EX_INFORMATION {
                        VirtualAddress: (m.BaseAddress as usize + i * page) as *mut _,
                        ..Default::default()
                    })
                    .collect();
                // SAFETY: a live buffer of the size passed
                let ok = unsafe {
                    K32QueryWorkingSetEx(
                        GetCurrentProcess(),
                        info.as_mut_ptr().cast(),
                        (info.len() * std::mem::size_of::<PSAPI_WORKING_SET_EX_INFORMATION>())
                            as u32,
                    )
                };
                if ok.as_bool() {
                    // SAFETY: reading the union's flags
                    e.3 += info
                        .iter()
                        .filter(|i| unsafe { i.VirtualAttributes.Flags } & 1 != 0)
                        .count()
                        * page;
                }
            }
        }
        addr = m.BaseAddress as usize + size;
        if addr == 0 {
            break;
        }
    }
    let mut regions: Vec<(usize, (usize, usize, bool, usize))> =
        by_base.into_iter().filter(|(_, v)| v.0 > 0).collect();
    regions.sort_by_key(|r| std::cmp::Reverse(r.1.3));
    let label = |base: usize, v: &(usize, usize, bool, usize)| {
        if heap_bases.contains(&base) {
            "heap"
        } else if v.2 {
            "stack"
        } else {
            "private"
        }
    };
    let mut by_label: HashMap<&str, (usize, usize)> = HashMap::new();
    for (b, v) in &regions {
        let e = by_label.entry(label(*b, v)).or_default();
        e.0 += v.0;
        e.1 += v.3;
    }
    let top: Vec<serde_json::Value> = regions
        .iter()
        .take(40)
        .map(|(b, v)| {
            serde_json::json!({ "base": format!("{b:x}"), "kind": label(*b, v), "committed": v.0,
                "reserved": v.1, "resident": v.3 })
        })
        .collect();
    let (ws, private) = crate::bench::memory();
    serde_json::json!({
        "workingSet": ws, "private": private, "totals": totals,
        "byLabel": by_label.iter().map(|(k, v)| (k.to_string(), serde_json::json!({"committed": v.0, "resident": v.1}))).collect::<serde_json::Map<_, _>>(),
        "heaps": heap_info, "regions": top,
        "rust": { "live": on::LIVE.load(Ordering::Relaxed), "peak": on::PEAK.load(Ordering::Relaxed),
            "allocs": on::COUNT.load(Ordering::Relaxed), "totalBytes": on::TOTAL.load(Ordering::Relaxed) },
        "byThread": on::THREAD_NAMES.lock().map(|names| {
            names.iter().enumerate().map(|(i, n)| (n.clone(), serde_json::json!(on::BY_THREAD[i].load(Ordering::Relaxed))))
                .collect::<serde_json::Map<_, _>>()
        }).unwrap_or_default(),
        "big": on::BIGS.lock().map(|b| {
            let mut v: Vec<_> = b.iter().map(|e| (e.1, e.2.clone())).collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.0));
            v.into_iter().map(|(n, w)| serde_json::json!({"size": n, "at": w})).collect::<Vec<_>>()
        }).unwrap_or_default(),
    })
}
