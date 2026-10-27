//! Who holds the engine's heap (native/windows/PERFORMANCE.md): the bar's warm-up and a typing session through
//! `Engine`, then every allocation still live, grouped by the engine function that made it. Build it with line
//! tables for names worth reading:
//!   CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_PROFILE_RELEASE_STRIP=none \
//!     cargo run --release -p qcalc-win --example mem_attrib --target-dir target-prof -- <data dir>
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use qcalc_ffi::{AnswerForm, Engine, EngineConfig, HistoryShow, HistoryView, LiveRequest};

struct Tracking;
static ON: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static SITES: Mutex<Option<HashMap<usize, (usize, std::backtrace::Backtrace)>>> = Mutex::new(None);
thread_local! {
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

fn note(p: *mut u8, n: usize) {
    LIVE.fetch_add(n, Ordering::Relaxed);
    if !ON.load(Ordering::Relaxed) || INSIDE.with(|i| i.replace(true)) {
        return;
    }
    let bt = std::backtrace::Backtrace::force_capture();
    if let Ok(mut s) = SITES.lock() {
        s.get_or_insert_with(HashMap::new)
            .insert(p as usize, (n, bt));
    }
    INSIDE.with(|i| i.set(false));
}

fn forget(p: *mut u8, n: usize) {
    LIVE.fetch_sub(n, Ordering::Relaxed);
    if INSIDE.with(|i| i.replace(true)) {
        return;
    }
    if let Ok(mut s) = SITES.lock()
        && let Some(m) = s.as_mut()
    {
        m.remove(&(p as usize));
    }
    INSIDE.with(|i| i.set(false));
}

// SAFETY: forwards to System
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        note(p, l.size());
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        forget(p, l.size());
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        forget(p, l.size());
        let q = unsafe { System.realloc(p, l, n) };
        note(q, n);
        q
    }
}

#[global_allocator]
static A: Tracking = Tracking;

fn main() {
    let data = std::env::args().nth(1);
    ON.store(true, Ordering::Relaxed);
    let engine = Engine::new(EngineConfig {
        is_windows: true,
        locale: "en_US".into(),
        data_dir: data,
        time_zone: None,
    });
    // the bar's warm-up (app.rs)
    let _ = engine.reload_rates();
    let _ = engine.evaluate(
        vec!["2+2".into(), "sin(90)".into(), "72 f to c".into()],
        qcalc_ffi::default_eval_options(),
    );
    let warm = LIVE.load(Ordering::Relaxed);
    let rows = engine.load_history();
    let now = 1_793_000_000_000;
    let view = HistoryView {
        show: HistoryShow::Recent,
        answer_form: AnswerForm::Exact,
        sig_figs: 10,
        now_ms: now,
    };
    let context = engine.context_from_history(rows, view);
    let golden: Vec<String> = std::env::var("GOLDEN")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let standard = [
        "2+2",
        "sin(90)",
        "72 f to c",
        "1/3 + 1/6",
        "sqrt(2)*pi",
        "5 km in miles",
        "3x+2=11",
        "(1+2)*(3+4)/5^2",
        "20% of 80",
        "e^(i pi)+1",
    ];
    for input in standard
        .iter()
        .map(|s| s.to_string())
        .chain(golden)
    {
        let chars: Vec<char> = input.chars().collect();
        for k in 1..=chars.len() {
            let text: String = chars[..k].iter().collect();
            let chained = engine.chains(text.clone(), context.clone());
            let _ = engine.live(LiveRequest {
                text,
                options: qcalc_ffi::default_eval_options(),
                context: context.clone(),
                chained,
                now_ms: now,
                phrases: true,
            });
        }
    }
    ON.store(false, Ordering::Relaxed);
    let typed = LIVE.load(Ordering::Relaxed);
    INSIDE.with(|i| i.set(true));
    let sites = SITES.lock().unwrap().take().unwrap_or_default();
    let mut by: HashMap<String, (usize, usize)> = HashMap::new();
    let mut regex_bytes = 0usize;
    for (_, (n, bt)) in sites {
        let text = format!("{bt}");
        let frames: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .filter_map(|l| l.split_once(": ").map(|(_, f)| f))
            .collect();
        if frames
            .iter()
            .any(|f| f.starts_with("regex") || f.starts_with("fancy_regex"))
        {
            regex_bytes += n;
            // which part of a regex: the deepest regex frame's module
            let part = frames
                .iter()
                .find(|f| {
                    (f.starts_with("regex")
                        || f.starts_with("aho_corasick")
                        || f.starts_with("fancy_regex"))
                        && !f.contains("::pool::")
                })
                .map_or("?".to_string(), |f| {
                    f.split("::").take(3).collect::<Vec<_>>().join("::")
                });
            let e = by.entry(format!("  [regex part] {part}")).or_default();
            e.0 += n;
            e.1 += 1;
        }
        let by_static = std::env::var_os("BY_STATIC").is_some();
        let site = frames
            .iter()
            .find(|f| {
                let helper = [
                    "::js",
                    "::re",
                    "::js_re",
                    "::caps",
                    "::compile",
                    "::replace",
                    "::regex",
                ]
                .iter()
                .any(|h| f.ends_with(h));
                f.starts_with("qcalc_core::")
                    && !f.starts_with("qcalc_core::jsre")
                    && (if by_static {
                        !helper
                    } else {
                        !f.contains("closure")
                    })
            })
            .or_else(|| frames.iter().find(|f| f.starts_with("qcalc_")))
            .map_or("?", |v| v);
        let e = by.entry(site.to_string()).or_default();
        e.0 += n;
        e.1 += 1;
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1.0));
    println!(
        "live after warm-up {:.2} MB, after typing {:.2} MB; under regex/fancy_regex {:.2} MB",
        warm as f64 / 1e6,
        typed as f64 / 1e6,
        regex_bytes as f64 / 1e6
    );
    for (site, (n, k)) in v.iter().take(40) {
        println!("{:>9} {:>6}  {site}", n, k);
    }
}
