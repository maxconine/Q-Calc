//! Fuzzes the preview converter (`qcalc_core::app::typst_math`) with short random lines from the calculator's
//! character set and compiles every preview it produces: none may be a compile error, and no typed symbol may
//! vanish. Slow (thousands of compiles), so it's ignored by default:
//! `cargo test --release -p qcalc-typst --test preview_fuzz -- --ignored --nocapture`; `FUZZ_CASES=200000`,
//! `FUZZ_SEED=7` and `FUZZ_LEN=20` (tokens per line) go further.

use qcalc_core::app::typst_math::{TYPST_PREAMBLE, preview_math, to_typst_math, typst_document};
use qcalc_typst::Renderer;

/// What a line is made of: single characters the keyboard and paste give, and the words the calculator reads.
#[rustfmt::skip]
const TOKENS: &[&str] = &[
    "0", "1", "2", "3", "7", "9", "12", ".5", "3.14", "1e5", "2e-3", "0b1010", "0xff", "0o17", "x", "y", "n",
    "e", "i", "A", "t", "ab", "km", "x1", "dx", "dt", " ", " ", " ", "+", "-", "−", "*", "/", "^", "=", "<", ">",
    "!", "%", ".", ",", ";", ":", "?", "~", "&", "|", "'", "\"", "$", "#", "@", "`", "\\", "_", "(", "(", ")",
    ")", "[", "]", "{", "}", "√", "∛", "∜", "π", "θ", "∞", "±", "∓", "×", "÷", "·", "°", "²", "³", "⁻¹", "ᵀ",
    "Σ", "∑", "Π", "∫", "≤", "≥", "≠", "≈", "→", "->", "=>", "..", "...", "…", "′", "″", "⌊", "⌋", "⌈", "⌉", "‖",
    "⟨", "⟩", "€", "£", "¥", "¢", "‰", "§", "¶", "—", "–", "‹", "›", "«", "»", "“", "”", "‘", "’", "α", "Ω",
    "∂", "∇", "sin", "cos", "tan", "asin", "atan", "ln", "log", "log2", "sqrt", "cbrt", "abs", "floor",
    "ceil", "round", "max", "min", "ncr", "npr", "det", "inv", "transpose", "sum", "prod", "int", "integral",
    "lim", "solve", "d/dx", "for", "at", "of", "to", "from", "and", "or", "mod", "ans", "pi", "theta", "inf",
    "binom", "frac", "mat", "vec", "root", "norm", "amat", "ip", "imod", "ibar", "multibinom", "dot",
    "cross", "eye", "nthroot", "\\frac", "\\sqrt", "\\pi", "\\alpha", "\\left", "\\cdot",
];

/// The symbols that must reach the preview in some form: alone, each one previews as something.
#[rustfmt::skip]
const SYMBOLS: &[&str] = &[
    "+", "-", "*", "/", "^", "=", "<", ">", "!", "%", ",", ";", ":", "?", "~", "&", "|", "'", "\"",
    "$", "#", "@", "`", "\\", "√", "∛", "∜", "π", "θ", "∞", "±", "∓", "×", "÷", "·", "°", "Σ", "∫", "≤",
    "≥", "≠", "≈", "→", "…", "′", "″", "⌊", "⌋", "⌈", "⌉", "‖", "⟨", "⟩", "€", "£", "¥", "¢", "‰",
    "§", "¶", "—", "–", "‹", "›", "«", "»", "“", "”", "‘", "’", "(", ")", "[", "]", "{", "}",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn line(rng: &mut Rng, longest: usize) -> String {
    let n = 1 + rng.below(longest);
    (0..n).map(|_| TOKENS[rng.below(TOKENS.len())]).collect()
}

fn digits(s: &str) -> Vec<char> {
    let mut d: Vec<char> = s.chars().filter(char::is_ascii_digit).collect();
    d.sort_unstable();
    d.dedup();
    d
}

#[test]
#[ignore = "slow: compiles thousands of previews"]
fn random_lines_preview_or_compile() {
    let r = Renderer::with_macros(TYPST_PREAMBLE);
    let mut failures: Vec<String> = Vec::new();
    for sym in SYMBOLS {
        for expr in [sym.to_string(), format!("2{sym}3"), format!("x {sym} y")] {
            if to_typst_math(&expr).is_empty() || preview_math(&expr).is_empty() {
                failures.push(format!("{expr:?} previews as nothing"));
            }
            let math = preview_math(&expr);
            // a symbol between two numbers keeps both
            if expr.starts_with('2') && !(math.contains('2') && math.contains('3')) {
                failures.push(format!("{expr:?} → {math:?} drops a number"));
            }
        }
    }
    let env = |k: &str| std::env::var(k).ok().and_then(|n| n.parse::<u64>().ok());
    let cases = env("FUZZ_CASES").unwrap_or(4000);
    let longest = env("FUZZ_LEN").unwrap_or(7) as usize;
    let mut rng = Rng(env("FUZZ_SEED").unwrap_or(0x9E37_79B9_7F4A_7C15) | 1);
    let mut compiled = 0;
    for _ in 0..cases {
        let expr = line(&mut rng, longest);
        for (answer, solved) in [("", ""), ("12", ""), ("2", "x")] {
            let Some(doc) = typst_document(&expr, answer, solved) else {
                continue;
            };
            compiled += 1;
            if let Err(e) = r.compile(&doc) {
                failures.push(format!("{expr:?} → {:?}: {e:?}", preview_math(&expr)));
                break;
            }
        }
        // every digit typed shows (log10's 10 is the log's base, and LaTeX is rewritten first)
        let math = preview_math(&expr);
        if !math.is_empty() && !expr.contains("log") && !expr.contains('\\') {
            let missing: Vec<char> = digits(&expr)
                .into_iter()
                .filter(|d| !math.contains(*d))
                .collect();
            if !missing.is_empty() {
                failures.push(format!("{expr:?} → {math:?} drops {missing:?}"));
            }
        }
    }
    failures.sort();
    failures.dedup();
    for f in failures.iter().take(80) {
        println!("{f}");
    }
    println!(
        "{cases} lines, {compiled} documents compiled, {} failures",
        failures.len()
    );
    assert!(failures.is_empty());
}
