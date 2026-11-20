//! QA item 99: isolate on absurd lines was 0.5–1.5 s, now quick with the same answers. `y = 1e17 x^1e9 - 1 for x`
//! walked Fraction.js's Farey search 10^7 steps at a time for `0.9999999608560541`, 24 times (now runs of steps
//! taken at once, `symbolic/fraction.rs`); `y = 2+3i x^i + -inf for x` simplified `1^(3/i)`, `1^(4/i)`, … until the
//! budget ran out (now charged at once once it only counts, `symbolic/simplify.rs` `Counting`).

use qcalc_core::evaluate::evaluate_line;
use std::time::Instant;

fn on_engine_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn absurd_isolate_lines_answer_as_before_and_quickly() {
    on_engine_stack(|| {
        for (line, want, ms) in [
            (
                "y = 1e17 x^1e9 - 1 for x",
                "x = ±0.9999999608560541(y + 1)^(1/1e+9)",
                400,
            ),
            ("y = 2+3i x^i + -inf for x", "", 400),
            ("y = 2 x^3 - 1 for x", "x = cbrt((y + 1)/2)", 400),
        ] {
            let t = Instant::now();
            let got = evaluate_line(line, &Default::default()).display;
            let took = t.elapsed().as_millis();
            assert_eq!(got, want, "{line}");
            // a debug build: 3–4 s before
            assert!(took < ms, "{line}: {took} ms");
        }
        // the decimal's fraction is the same as the one-step-at-a-time walk found
        let f = qcalc_core::symbolic::simplify_text("0.9999999608560541 x").unwrap();
        assert_eq!(
            qcalc_core::symbolic::to_string(&f, &Default::default()),
            "0.9999999608560541 x"
        );
    });
}
