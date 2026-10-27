//! A seeded random-input harness for the engine's public entry points (built as qcalc-core's `fuzz` example).
//!   fuzz <corpus.json> <seed> <cases> [<current-file>]
//!   fuzz <corpus.json> replay <case seed> <json input or @file>   (case seed 0: the line alone; 1: live)
//! The corpus is a JSON array of strings (inputs to mutate; the golden fixtures' string arguments make a good one).
//! Each case runs on a 32 MB thread like the app's engine thread. A panic is caught and printed as `PANIC`; a case
//! slower than 500 ms prints `SLOW`; a watchdog prints `HANG` and exits 3 after 5 s, or `HUGE` and exits 4 when
//! the resident size passes 3 GB. The input being run is written to <current-file> first, so a stack overflow
//! (which aborts the process) leaves its input behind. With FUZZ_SHORT set every input stays under 300
//! characters, the lengths people type. Every line printed is `KIND <ms> <entry> <json input>`.
use qcalc_core::app::live::{LiveRequest, live};
use qcalc_core::app::messaging;
use qcalc_core::{EvaluateOptions, evaluate_sheet, graph};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// FUZZ_SHORT set: inputs stay under 300 characters, the lengths people type.
fn short() -> bool {
    static SHORT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SHORT.get_or_init(|| std::env::var_os("FUZZ_SHORT").is_some())
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
    fn chance(&mut self, p: u32) -> bool {
        self.below(100) < p as usize
    }
}

const TOKENS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "7",
    "10",
    "0.5",
    "1e308",
    "1e-308",
    "9e15",
    "123456789012345678901234567890",
    ".",
    "e",
    "pi",
    "π",
    "i",
    "x",
    "y",
    "z",
    "t",
    "n",
    "a",
    "b",
    "ans",
    "inf",
    "infinity",
    "NaN",
    "+",
    "-",
    "*",
    "/",
    "^",
    "!",
    "%",
    "=",
    "==",
    "<",
    ">",
    "≤",
    "≥",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    ";",
    ":",
    "|",
    "'",
    "\"",
    " ",
    "  ",
    "°",
    "deg",
    "rad",
    "sin",
    "cos",
    "tan",
    "asin",
    "atan2",
    "sinh",
    "log",
    "ln",
    "log2",
    "exp",
    "sqrt",
    "cbrt",
    "√",
    "∛",
    "abs",
    "floor",
    "round",
    "gcd",
    "lcm",
    "nCr",
    "nPr",
    "choose",
    "mod",
    "zeta",
    "gamma",
    "factorial",
    "sum",
    "mean",
    "median",
    "std",
    "max",
    "min",
    "det",
    "inv",
    "transpose",
    "d/dx",
    "derivative of",
    "integral of",
    "∫",
    "from",
    "to",
    "dx",
    "Σ",
    "sum of",
    "for",
    "solve",
    "isolate",
    "graph",
    "sys",
    "->",
    "→",
    "in",
    "to",
    "as",
    "m",
    "km",
    "ft",
    "in",
    "mph",
    "kg",
    "lb",
    "N",
    "J",
    "W",
    "kWh",
    "°C",
    "°F",
    "K",
    "L",
    "mL",
    "s",
    "min",
    "h",
    "day",
    "year",
    "USD",
    "$",
    "€",
    "EUR",
    "GBP",
    "%",
    "of",
    "off",
    "plus",
    "minus",
    "times",
    "half",
    "dozen",
    "H2O",
    "NaCl",
    "C6H12O6",
    "molar mass",
    "+ O2",
    "Fe2(SO4)3",
    "R",
    "mol",
    "atm",
    "message",
    "messages",
    "share",
    "bob",
    "@",
    "#",
    "now",
    "today",
    "tomorrow",
    "next friday",
    "in 3 hours",
    "pm",
    "am",
    "UTC",
    "×",
    "÷",
    "−",
    "·",
    "²",
    "³",
    "⁻¹",
    "₁",
    "½",
    "∞",
    "≈",
    "±",
    "√(",
    "f(x)",
    "f(x)=",
    "g(t)",
    "e^",
    "10^",
    "2^-",
    "!!",
    "%%",
    "lim",
    "as x→0",
    "x→∞",
    "def",
    "let",
    "set",
    "\\frac{1}{2}",
    "\\sqrt{",
    "\\pi",
    "\u{200b}",
    "\u{202e}",
    "\u{0301}",
    "😀",
    "ﬁ",
    "Ⅻ",
    "\t",
    "\n",
];

const WRAPS: &[(&str, &str)] = &[
    ("(", ")"),
    ("sqrt(", ")"),
    ("sin(", ")"),
    ("abs(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("|", "|"),
    ("2^(", ")"),
    ("(", ")!"),
    ("-(", ")"),
    ("log(", ")"),
    ("d/dx ", ""),
    ("integral of ", " from 0 to 1"),
    ("sum of ", " for n=1 to 10"),
    ("solve ", "=0"),
    ("e^(", ")"),
    ("f(", ")"),
];

fn random_tokens(r: &mut Rng, n: usize) -> String {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(r.pick(TOKENS));
        if r.chance(20) {
            s.push(' ');
        }
    }
    s
}

fn random_char(r: &mut Rng) -> char {
    match r.below(4) {
        0 => (0x20 + r.below(0x5f) as u32) as u8 as char,
        1 => char::from_u32(r.below(0x3000) as u32).unwrap_or('?'),
        2 => char::from_u32(0x1d400 + r.below(0x400) as u32).unwrap_or('?'),
        _ => *r.pick(&[
            '(', ')', '^', '!', '-', '/', '*', '.', 'e', '0', '9', ' ', '=', ',', '[', ']',
        ]),
    }
}

fn mutate(r: &mut Rng, base: &str, corpus: &[String]) -> String {
    let mut chars: Vec<char> = base.chars().collect();
    for _ in 0..1 + r.below(4) {
        let len = chars.len();
        match r.below(11) {
            0 if len > 0 => {
                let a = r.below(len);
                let b = (a + 1 + r.below(8)).min(len);
                chars.drain(a..b);
            }
            1 => {
                let at = r.below(len + 1);
                let t: Vec<char> = r.pick(TOKENS).chars().collect();
                chars.splice(at..at, t);
            }
            2 => {
                let at = r.below(len + 1);
                let c = random_char(r);
                chars.insert(at, c);
            }
            3 if len > 0 => {
                // repeat a slice many times: long and nested input
                let a = r.below(len);
                let b = (a + 1 + r.below(6)).min(len);
                let times = *r.pick(if short() {
                    &[2usize, 3, 5, 10, 20]
                } else {
                    &[2usize, 5, 20, 100, 500, 2000]
                });
                let piece: Vec<char> = chars[a..b].to_vec();
                let rep: Vec<char> = piece
                    .iter()
                    .copied()
                    .cycle()
                    .take(piece.len() * times)
                    .collect();
                chars.splice(a..b, rep);
            }
            4 => {
                let (open, close) = *r.pick(WRAPS);
                let times = *r.pick(if short() {
                    &[1usize, 2, 3, 5, 8]
                } else {
                    &[1usize, 3, 10, 50, 300, 1500]
                });
                let mut s = open.repeat(times);
                s.extend(chars.iter());
                s.push_str(&close.repeat(times));
                chars = s.chars().collect();
            }
            5 => {
                let other: Vec<char> = r.pick(corpus).chars().collect();
                let at = r.below(len + 1);
                let from = r.below(other.len() + 1);
                chars.truncate(at);
                chars.extend_from_slice(&other[from..]);
            }
            6 if len > 0 => {
                let at = r.below(len);
                if chars[at].is_ascii_digit() {
                    let big = *r.pick(&[
                        "999999999999999999999",
                        "1e400",
                        "0.000000000000000000001",
                        "170",
                        "171",
                        "1000000",
                    ]);
                    chars.splice(at..at + 1, big.chars());
                }
            }
            7 => {
                let at = r.below(len + 1);
                let t: Vec<char> = {
                    let n = 1 + r.below(5);
                    random_tokens(r, n)
                }
                .chars()
                .collect();
                chars.splice(at..at, t);
            }
            8 if len > 0 => {
                let at = r.below(len);
                chars[at] = random_char(r);
            }
            9 => {
                let tail: Vec<char> = r
                    .pick(&[
                        "!",
                        "!!",
                        "^99",
                        "^^",
                        "%",
                        " in ft",
                        " to hex",
                        " as fraction",
                        "=",
                        "?",
                        " =x",
                    ])
                    .chars()
                    .collect();
                chars.extend(tail);
            }
            _ => {
                let pre = *r.pick(&[
                    "graph ",
                    "solve ",
                    "d/dx ",
                    "∫ ",
                    "message bob ",
                    "sys 2",
                    "isolate x ",
                    "lim ",
                    "Σ ",
                ]);
                let mut s: Vec<char> = pre.chars().collect();
                s.extend(chars);
                chars = s;
            }
        }
    }
    chars
        .into_iter()
        .take(if short() { 300 } else { 20_000 })
        .collect()
}

fn pathological(r: &mut Rng) -> String {
    let n = *r.pick(if short() {
        &[3usize, 10, 30, 100, 300]
    } else {
        &[10usize, 100, 1000, 5000, 20000]
    });
    match r.below(20) {
        0 => format!("{}1{}", "(".repeat(n), ")".repeat(n)),
        1 => "(".repeat(n),
        2 => format!("1{}", "+1".repeat(n)),
        3 => "-".repeat(n) + "1",
        4 => "2^".repeat(n.min(3000)) + "2",
        5 => format!(
            "{}1{}",
            "sqrt(".repeat(n.min(3000)),
            ")".repeat(n.min(3000))
        ),
        6 => "9".repeat(n),
        7 => "1".to_string() + &"!".repeat(n),
        8 => format!("[{}]", vec!["1"; n.min(5000)].join(",")),
        9 => format!("[{}]", vec!["[1,2]"; n.min(3000)].join(";")),
        10 => "x".repeat(n),
        11 => format!("{}x", "x*".repeat(n)),
        12 => "1/".repeat(n) + "1",
        13 => "sin".repeat(n.min(5000)) + "(1)",
        14 => format!("d/dx {}", "x^".repeat(n.min(500)) + "x"),
        15 => "é".repeat(n),
        16 => format!("{}1", "√".repeat(n)),
        17 => format!("solve {}=1", "x^".repeat(n.min(200)) + "x"),
        18 => format!("{}1{}", "[".repeat(n.min(5000)), "]".repeat(n.min(5000))),
        _ => format!("{}x{}", "f(".repeat(n.min(3000)), ")".repeat(n.min(3000))),
    }
}

fn options(r: &mut Rng) -> EvaluateOptions {
    let mut o = EvaluateOptions::default();
    if r.chance(30) {
        o.angle_mode = Some(if r.chance(50) {
            qcalc_core::types::AngleMode::Deg
        } else {
            qcalc_core::types::AngleMode::Rad
        });
    }
    if r.chance(20) {
        o.fraction_mode = Some(true);
    }
    if r.chance(10) {
        o.sig_figs = Some(*r.pick(&[1.0, 3.0, 6.0, 12.0, 16.0, 0.0, -1.0, 100.0]));
    }
    if r.chance(10) {
        o.ans = Some(*r.pick(&[0.0, -1.0, 1e300, f64::NAN, f64::INFINITY, 3.5]));
    }
    o
}

/// FUZZ_TRACE set: each step of a case goes to stderr before it runs, to find which one crashed.
fn trace(step: &str) {
    if std::env::var_os("FUZZ_TRACE").is_some() {
        eprintln!("trace {step}");
    }
}

/// x ranges the graph panel can reach by panning and zooming, and some it shouldn't.
const DOMAINS: &[(f64, f64)] = &[
    (-10.0, 10.0),
    (-1e-12, 1e-12),
    (1e15, 1e15 + 1.0),
    (-1e300, 1e300),
    (5.0, 5.0),
    (10.0, -10.0),
    (0.0, 5e-324),
    (-1e-300, 1e-300),
    (1e300, f64::MAX),
    (f64::NAN, 1.0),
    (f64::NEG_INFINITY, f64::INFINITY),
    (-720.0, 720.0),
];

fn graph_options(r: &mut Rng) -> graph::GraphOptions {
    let mut go = graph::GraphOptions::default();
    if r.chance(60) {
        go.domain = Some(*r.pick(DOMAINS));
    }
    if r.chance(30) {
        go.sample_count = Some(*r.pick(&[0.0, 1.0, 2.0, 3.0, 50.0, 500.0, 2000.0, -5.0, f64::NAN]));
    }
    if r.chance(30) {
        go.aspect = Some(*r.pick(&[0.0, 0.5, 1.0, 1e-9, 1e9, -1.0, f64::NAN]));
    }
    if r.chance(20) {
        go.angle_mode = Some(qcalc_core::types::AngleMode::Deg);
    }
    go
}

/// The entry points a case may hit; returns the name of the one it ran.
fn run_case(r: &mut Rng, input: &str, corpus: &[String]) -> &'static str {
    let o = options(r);
    // FUZZ_ENTRY=line|sheet|live|graph|sample|messaging runs only that entry point
    static ENTRY: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let only = *ENTRY.get_or_init(|| {
        let e = std::env::var("FUZZ_ENTRY").ok()?;
        ["line", "sheet", "live", "graph", "sample", "messaging"]
            .iter()
            .position(|n| *n == e)
            .map(|i| [0, 5, 6, 9, 10, 11][i])
    });
    let pick = r.below(12);
    match only.unwrap_or(pick) {
        0..=4 => {
            std::hint::black_box(evaluate_sheet(&[input.to_string()], &o));
            "line"
        }
        5 => {
            let mut lines = vec![];
            for _ in 0..1 + r.below(4) {
                lines.push(if r.chance(50) {
                    {
                        let b = r.pick(corpus).clone();
                        mutate(r, &b, corpus)
                    }
                } else {
                    r.pick(corpus).clone()
                });
            }
            lines.insert(r.below(lines.len() + 1), input.to_string());
            eprintln!("sheet {}", serde_json::to_string(&lines).unwrap());
            std::hint::black_box(evaluate_sheet(&lines, &o));
            "sheet"
        }
        6..=8 => {
            let req = LiveRequest {
                text: input.to_string(),
                options: o,
                phrases: true,
                now_ms: 1.7e12,
                tab: r.below(3),
                ..Default::default()
            };
            std::hint::black_box(live(&req));
            "live"
        }
        9 => {
            let text = if input.starts_with("graph") {
                input.to_string()
            } else {
                format!("graph {input}")
            };
            let go = graph_options(r);
            trace(&format!("build_graph {text:?} {go:?}"));
            std::hint::black_box(graph::build_graph(&text, &go));
            if r.chance(50) {
                let (lo, hi) = graph::graph_home(&text, &go);
                let target = *r.pick(&[7.0, 1.0, 0.0, -3.0, 1e9, f64::NAN]);
                let unit = *r.pick(&[
                    None,
                    Some(qcalc_core::types::AngleMode::Deg),
                    Some(qcalc_core::types::AngleMode::Rad),
                ]);
                trace(&format!("graph_ticks {lo} {hi} {unit:?} {target}"));
                std::hint::black_box(graph::graph_ticks(lo, hi, unit, target));
                let (a, b) = *r.pick(DOMAINS);
                trace(&format!("ticks/readouts {a} {b}"));
                std::hint::black_box(graph::graph_ticks(a, b, unit, target));
                std::hint::black_box(graph::readout_y(&text, &go, a));
                std::hint::black_box(graph::readout_z(&text, &go, a, b));
            }
            "graph"
        }
        10 => {
            std::hint::black_box(graph::sample_graph(input, &graph_options(r)));
            "sample"
        }
        _ => {
            std::hint::black_box(messaging::parse_message(input));
            std::hint::black_box(messaging::parse_messages_command(input));
            std::hint::black_box(messaging::parse_share_command(input));
            std::hint::black_box(messaging::parse_inbox_frame(input));
            std::hint::black_box(messaging::clean_filename(input));
            std::hint::black_box(messaging::clean_message_text(&json!(input)));
            std::hint::black_box(messaging::b64url_decode(input));
            std::hint::black_box(messaging::open_file_facts(input, r.next() % 1_000_000));
            let share = json!({"id": input, "from": input, "epk": input, "sealed": input, "bytes": r.next() % 10_000});
            std::hint::black_box(messaging::clean_sealed_offer(&share));
            std::hint::black_box(messaging::clean_sealed_clip(&share));
            if let Ok(v) = serde_json::from_str::<Value>(input) {
                std::hint::black_box(messaging::clean_sealed_offer(&v));
                std::hint::black_box(messaging::sanitize_messaging(&v));
            }
            "messaging"
        }
    }
}

fn rss_kb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output();
    out.ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let corpus: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    if args[2] == "replay" {
        // fuzz <corpus> replay <case seed> <json input>: rerun one case with the default panic hook
        // the input as JSON, or @file holding it
        let raw = match args[4].strip_prefix('@') {
            Some(path) => std::fs::read_to_string(path).unwrap(),
            None => args[4].clone(),
        };
        let input: String = serde_json::from_str(&raw).unwrap();
        let case_seed: u64 = args[3].parse().unwrap();
        let t = Instant::now();
        let entry = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                if case_seed == 0 {
                    // seed 0: the line alone, default options, and its answer
                    let r = evaluate_sheet(&[input], &EvaluateOptions::default())
                        .pop()
                        .unwrap();
                    println!("{:?} {:?}", r.kind, r.display);
                    return "line";
                }
                if case_seed == 1 {
                    // seed 1: the bar's live answer for the line, as the app asks for it
                    let req = LiveRequest {
                        text: input,
                        phrases: true,
                        now_ms: 1.7e12,
                        ..Default::default()
                    };
                    let a = live(&req);
                    println!(
                        "{:?}",
                        format!("{a:?}").chars().take(160).collect::<String>()
                    );
                    return "live";
                }
                run_case(&mut Rng(case_seed | 1), &input, &corpus)
            })
            .unwrap()
            .join();
        println!("{entry:?} {} ms", t.elapsed().as_millis());
        return;
    }
    let seed: u64 = args[2].parse().unwrap();
    let cases: usize = args[3].parse().unwrap();
    let current = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| "fuzz-current.txt".into());

    std::panic::set_hook(Box::new(|_| {}));
    let started = Arc::new(AtomicU64::new(0));
    let busy = Arc::new(AtomicBool::new(false));
    let now_input = Arc::new(Mutex::new(String::new()));
    let epoch = Instant::now();
    {
        let (started, busy, now_input) = (started.clone(), busy.clone(), now_input.clone());
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let el = epoch.elapsed().as_millis() as u64 - started.load(Ordering::SeqCst);
                if busy.load(Ordering::SeqCst) && el > 5000 {
                    println!("HANG {el} ? {}", now_input.lock().unwrap());
                    std::process::exit(3);
                }
                let rss = rss_kb();
                if rss > 3_000_000 {
                    println!("HUGE {rss} ? {}", now_input.lock().unwrap());
                    std::process::exit(4);
                }
            }
        });
    }
    let worker = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            for _ in 0..cases {
                let input = match r.below(10) {
                    0 => pathological(&mut r),
                    1 => {
                        let n = 1 + r.below(12);
                        random_tokens(&mut r, n)
                    }
                    _ => {
                        let base = r.pick(&corpus).clone();
                        mutate(&mut r, &base, &corpus)
                    }
                };
                let shown = serde_json::to_string(&input).unwrap();
                let case_seed = r.next();
                // the case seed first, then the input: what `replay` takes
                std::fs::write(&current, format!("{case_seed} {shown}")).ok();
                *now_input.lock().unwrap() = format!("{case_seed} {shown}");
                started.store(epoch.elapsed().as_millis() as u64, Ordering::SeqCst);
                busy.store(true, Ordering::SeqCst);
                let t = Instant::now();
                let mut cr = Rng(case_seed | 1);
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_case(&mut cr, &input, &corpus)
                }));
                busy.store(false, Ordering::SeqCst);
                let took = t.elapsed().as_millis();
                match res {
                    Err(e) => {
                        let msg = e
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_default();
                        println!("PANIC {took} {case_seed} {shown} :: {msg}");
                    }
                    Ok(entry) if took > 500 => println!("SLOW {took} {entry}:{case_seed} {shown}"),
                    _ => {}
                }
            }
        })
        .unwrap();
    worker.join().unwrap();
}
