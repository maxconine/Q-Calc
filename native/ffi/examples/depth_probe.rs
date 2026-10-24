//! Which nesting depths the engine answers, on a thread of a given stack, for sizing the Swift test's deep input:
//! `cargo run --release -p qcalc-ffi --example depth_probe -- <depth> <stack KB>`. An overflow aborts the process.

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let depth = args.first().copied().unwrap_or(250);
    let stack_kb = args.get(1).copied().unwrap_or(256 * 1024);
    let text = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
    let out = std::thread::Builder::new()
        .stack_size(stack_kb * 1024)
        .spawn(move || qcalc_core::evaluate_line(&text, &Default::default()).display)
        .expect("thread")
        .join()
        .expect("join");
    println!("depth {depth} on {stack_kb} KB: {out:?}");
}
