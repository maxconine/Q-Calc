//! Missing functions (QA item 36, from items 10 and 11): `size`, `numel` and statistics over a matrix's entries, the
//! exact fractions of an `lu` beside its decimals, and `cis(θ)`, `rect(…)`, one-argument `polar(z)`. Native only.
use qcalc_core::evaluate::evaluate_sheet;
use qcalc_core::types::{EvaluateOptions, LineResult};

fn last(lines: &[&str]) -> LineResult {
    let all: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    evaluate_sheet(&all, &EvaluateOptions::default())
        .pop()
        .expect("a line")
}

fn show(t: &str) -> String {
    last(&[t]).display
}

#[test]
fn size_and_count() {
    assert_eq!(show("size([1 2 3; 4 5 6])"), "[2 3]");
    assert_eq!(show("size([1 2 3])"), "[1 3]");
    assert_eq!(show("numel([1 2; 3 4])"), "4");
    assert_eq!(last(&["A = [1 2 3; 4 5 6]", "size(A)"]).display, "[2 3]");
}

#[test]
fn statistics_of_every_entry() {
    for (t, want) in [
        ("sum([1 2 3])", "6"),
        ("sum([1 2; 3 4])", "10"),
        ("mean([1 2 3 4])", "2.5"),
        ("mean([1 2; 3 4])", "2.5"),
        ("median([3 1 2])", "2"),
        ("median([1 2 3 4])", "2.5"),
        ("min([3 1 2])", "1"),
        ("max([3 1 2])", "3"),
        ("var([1 2 3 4])", "1.66666666667"),
        ("std([1 2 3 4])", "1.29099444874"),
        ("2*sum([1 2 3])", "12"),
        ("std([5; 6; 7])", "1"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // the sample variance, as with plain numbers
    assert_eq!(show("var([1 2 3 4])"), show("var(1,2,3,4)"));
    assert_eq!(
        last(&["std([1 2 3 4])"]).exact.as_deref(),
        Some("sqrt(15)/3")
    );
    assert_eq!(last(&["v = [1 2 3 4]", "mean(v)"]).display, "2.5");
    // plain numbers keep their meaning
    assert_eq!(show("max(3, 5)"), "5");
    assert_eq!(show("sum(k^2, k, 1, 10)"), "385");
}

#[test]
fn lu_writes_its_fractions() {
    let r = last(&["lu([4 3; 6 3])"]);
    assert_eq!(r.display, "L = [1 0; 1.5 1],  U = [4 3; 0 -1.5]");
    assert_eq!(
        r.exact.as_deref(),
        Some("L = [1 0; 3/2 1],  U = [4 3; 0 -3/2]")
    );
    assert_eq!(last(&["lu([1 2; 3 4])"]).exact, None);
}

#[test]
fn cis_rect_and_one_argument_polar() {
    for (t, want) in [
        ("cis(30°)", "0.866025403784 + 0.5i"),
        ("cis(pi/3 rad)", "0.5 + 0.866025403784i"),
        ("2 cis(60°)", "1 + 1.73205080757i"),
        ("cis(30)^3", "i"),
        ("rect(2, 60°)", "1 + 1.73205080757i"),
        ("rect(10∠30)", "8.66025403784 + j5"),
        ("rect(3+4i)", "3 + 4i"),
        ("polar(3+4i)", "5∠53.1301023542°"),
        ("polar((1+i)^2)", "2∠90°"),
        ("polar(5)", "5∠0°"),
        ("polar(cis(45))", "1∠45°"),
        ("polar(3+4i) in rect", "3 + 4i"),
        // two arguments are still r and θ
        ("polar(3, 4)", "2.99269215078 + j0.209269421232"),
    ] {
        assert_eq!(show(t), want, "{t}");
    }
    // your own cis stays yours
    assert_eq!(last(&["cis(x) = 2x", "cis(3)"]).display, "6");
}
