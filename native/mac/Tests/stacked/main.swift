// Headless checks for the bar's stacked fractions and powers (native/mac/scripts/test-stacked.sh): what
// StackedMath.parse stacks, with roots, fractions and powers in each other, and what `/`, `^`, `)` and ⌫ do.
// Foundation only: no app, no window.

import Foundation

var failures = 0

func check(_ what: String, _ got: String, _ want: String) {
    let ok = got == want
    print(ok ? "ok   \(what)" : "FAIL \(what)\n     got  \(got)\n     want \(want)")
    if !ok { failures += 1 }
}

func text(_ s: String, _ r: NSRange) -> String { (s as NSString).substring(with: r) }

/// the stacks of `s`, outer first: `frac(num|den)` and `pow(exp)`, each slot's own text
func stacks(_ s: String) -> String {
    StackedMath.parse(s).stacks.map { st in
        st.isFraction ? "frac(\(text(s, st.num!.content))|\(text(s, st.slot.content)))" : "pow(\(text(s, st.slot.content)))"
    }.joined(separator: " ")
}

/// the line as shown: hidden characters dropped
func shown(_ s: String) -> String {
    let p = StackedMath.parse(s), u = Array(s.utf16)
    let kept = u.indices.filter { !p.hidden.contains($0) }.map { u[$0] }
    return String(utf16CodeUnits: kept, count: kept.count)
}

/// an edit as `text` with `|` at the caret
func edit(_ e: StackedMath.Edit?) -> String {
    guard let e else { return "nil" }
    let u = Array(e.text.utf16)
    return String(utf16CodeUnits: Array(u[..<e.caret]), count: e.caret) + "|" + String(utf16CodeUnits: Array(u[e.caret...]), count: u.count - e.caret)
}

/// typing `keys` one at a time into an empty line, as the bar's keys do: `/`, `^` and `)` through StackedMath
func type(_ keys: String) -> String {
    var t = "", c = 0
    for k in keys {
        let u = Array(t.utf16)
        func put(_ s: String) {
            t = String(utf16CodeUnits: Array(u[..<c]), count: c) + s + String(utf16CodeUnits: Array(u[c...]), count: u.count - c)
            c += (s as NSString).length
        }
        switch k {
        case "/": let e = StackedMath.slashKey(t, start: c, end: c); t = e.text; c = e.caret
        // nothing to raise: the key does nothing
        case "^": if let e = StackedMath.powerKey(t, caret: c) { t = e.text; c = e.caret }
        // the bar moves the caret on past a place it never stops (InputField's snap)
        case ")": if let to = StackedMath.closeKey(t, caret: c) { c = StackedMath.snap(StackedMath.parse(t), to, from: c, length: u.count) } else { put(")") }
        default:
            if let e = StackedMath.breakOut(t, caret: c, typed: String(k)) ?? StackedMath.termAfterStack(t, caret: c, typed: String(k)) {
                t = e.text; c = e.caret
            } else { put(String(k)) }
        }
    }
    return edit(StackedMath.Edit(text: t, caret: c))
}

// ---- fractions

check("a plain fraction", stacks("1/2"), "frac(1|2)")
check("a bracketed denominator", stacks("12/(12^(2))"), "frac(12|12^(2)) pow(2)")
check("bracketed slots hide their brackets", shown("(1+x)/(2)"), "1+x2")
check("a fraction in a denominator", stacks("1/(x/3)"), "frac(1|x/3) frac(x|3)")
check("a fraction in a numerator", stacks("(1/2)/4"), "frac(1/2|4) frac(1|2)")
check("two in a row", stacks("1/2+3/4"), "frac(1|2) frac(3|4)")
check("a term after the denominator takes it (2/3x is 2/(3x))", stacks("2/3x"), "")
check("a power after the denominator takes it", stacks("1/2^3"), "pow(3)")
check("a mixed number and letter isn't a numerator", stacks("2x/3"), "")
check("a function's call is a numerator", stacks("sin(x)/2"), "frac(sin(x)|2)")
check("an empty fraction", stacks("()/()"), "frac(|)")
check("a slash with nothing after it", stacks("1/"), "")
check("a decimal", stacks("0.5/2.25"), "frac(0.5|2.25)")
check("after a sign", stacks("-1/2"), "frac(1|2)")
check("in an equation", stacks("y=x/2"), "frac(x|2)")

// ---- roots

check("a fraction under a root", stacks("√(12/(12^(2)))"), "frac(12|12^(2)) pow(2)")
check("a fraction under sqrt(", stacks("sqrt(12/(12^(2)))"), "frac(12|12^(2)) pow(2)")
check("a root over its number isn't a numerator (√2/3 is (√2)/3)", stacks("√2/3"), "")
check("a bracketed root is", stacks("(√2)/3"), "frac(√2|3)")
check("a root in a denominator", stacks("1/(√(2))"), "frac(1|√(2))")
check("a root in a power", stacks("2^(√(2))"), "pow(√(2))")
check("a power under a root", stacks("√(2^(10))"), "pow(10)")
check("two roots over fractions", stacks("√(x/2)+√((1/2)/4)"), "frac(x|2) frac(1/2|4) frac(1|2)")
check("a cube root over a fraction", stacks("∛(1/8)"), "frac(1|8)")

// ---- powers

check("a number's power", stacks("2^10"), "pow(10)")
check("a bracketed power hides its brackets", shown("2^(10)"), "210")
check("a letter's power", stacks("x^2"), "pow(2)")
check("one letter as the power", stacks("e^x"), "pow(x)")
check("a word isn't a power", stacks("e^xy"), "")
check("a power of a group", stacks("(1+x)^2"), "pow(2)")
check("a power in a power", stacks("2^(3^(2))"), "pow(3^(2)) pow(2)")

// ---- typing

check("12/12^2 typed", type("12/12^2"), "12/(12^(2|))")
check("…and out of both", type("12/12^2))"), "12/(12^(2))|")
check("sqrt(12/12^2) typed (the root's own bracket is the line's to close)", type("sqrt(12/12^2"), "sqrt(12/(12^(2|))")
check("…closed", type("sqrt(12/12^2)))"), "sqrt(12/(12^(2)))|")
check("a root's number goes over the bar whole", type("√2/3"), "(√2)/(3|)")
check("a bracketed root goes over whole", type("√(2)/3"), "(√(2))/(3|)")
check("/ with nothing before it", type("/"), "(|)/()")
check(") from the numerator goes into the denominator", type("/1)2"), "(1)/(2|)")
check("a term after a fraction is multiplied by it", type("1/2)x"), "1/(2)·x|")
check("^ after a fraction brackets it", edit(StackedMath.powerKey("1/(2)", caret: 5)), "(1/(2))^(|)")
check("^ with nothing before it", edit(StackedMath.powerKey("", caret: 0)), "nil")
check("a selection goes over the bar", edit(StackedMath.slashKey("x+1", start: 0, end: 3)), "(x+1)/(|)")
check("^ after a bracketed fraction brackets it", edit(StackedMath.powerKey("(1)/(2)", caret: 7)), "((1)/(2))^(|)")
check("^ typed after a fraction raises the whole fraction", type("1/2)^"), "(1/(2))^(|)")
check("/ after a fraction makes a fraction of it", type("1/2)/"), "(1/(2))/(|)")
check("^ after a root raises the root", edit(StackedMath.powerKey("√(2)", caret: 4)), "√(2)^(|)")
check("a nested fraction typed", type("1/x/3"), "1/(x/(3|))")

// ---- backspace

check("⌫ after a fraction steps in", edit(StackedMath.backspace("1/(2)", caret: 5)), "1/(2|)")
check("⌫ at a denominator's start takes the fraction apart", edit(StackedMath.backspace("1/(2)", caret: 3)), "1|2")
check("⌫ at a numerator's start takes it apart", edit(StackedMath.backspace("(1)/(2)", caret: 1)), "|12")
check("⌫ after a power steps in", edit(StackedMath.backspace("2^(10)", caret: 6)), "2^(10|)")
check("⌫ in the middle is the line's", edit(StackedMath.backspace("12/(34)", caret: 5)), "nil")

// ---- MathQuill's cases (desmosinc/mathquill test/unit: typing, SupSub, backspace, updown, charsThatBreakOutOfSupSub)

/// `s` with `|` at the caret, as (text, caret)
func at(_ s: String) -> (String, Int) {
    let r = (s as NSString).range(of: "|")
    return ((s as NSString).replacingCharacters(in: r, with: ""), r.location)
}
func bs(_ s: String) -> String { let (t, c) = at(s); return edit(StackedMath.backspace(t, caret: c)) }
func del(_ s: String) -> String { let (t, c) = at(s); return edit(StackedMath.forwardDelete(t, caret: c)) }
func vert(_ s: String, up: Bool) -> String {
    let (t, c) = at(s)
    switch StackedMath.vertical(StackedMath.parse(t), caret: c, up: up) {
    case .to(let i)?: return edit(StackedMath.Edit(text: t, caret: i))
    case .nearest(let slot)?: return "nearest(" + text(t, slot.content) + ")"
    case nil: return "nil"
    }
}
func sel(_ s: String, _ r: NSRange) -> String { text(s, StackedMath.expand(StackedMath.parse(s), r, length: (s as NSString).length)) }

// `/` takes everything back to an operator (LiveFraction)
check("1+/ takes only what's after the +", type("1+2/"), "1+2/(|)")
check("2x/ takes 2x", type("2x/3"), "(2x)/(3|)")
check("a space stops it (1 2/3)", type("1 2/3"), "1 2/(3|)")
check("a function's call goes over whole", type("sin(x)/2"), "sin(x)/(2|)")
check("a power goes over whole", type("2^3)/4"), "(2^(3))/(4|)")
check("a fraction and the term after it go over whole (½x/)", type("1/2)x/"), "(1/(2)·x)/(|)")
check("|x| goes over whole", type("|x|/2"), "(|x|)/(2|)")
check("x! goes over whole", type("x!/2"), "(x!)/(2|)")
check("a percentage goes over whole", type("5%/2"), "(5%)/(2|)")
check("/ inside a root takes only what's under it", type("√(1/2"), "√(1/(2|)")

// `^` (SupSub, supSubsRequireOperand)
check("^ at the start does nothing", type("^2"), "2|")
check("^^ raises once", type("x^^2"), "x^(2|)")
check("^ after a power goes back into it", type("x^2)^3"), "x^(23|)")
check("…an unbracketed one too", edit(StackedMath.powerKey("x^2", caret: 3)), "x^(2|)")
check("^ over a selection raises it", edit(StackedMath.powerKey("x2", start: 1, end: 2)), "x^(2|)")
check("^ over a selection with nothing before it does nothing", edit(StackedMath.powerKey("2", start: 0, end: 1)), "nil")
check("x^2n stays in the power", type("x^2n"), "x^(2n|)")

// what leaves a power (charsThatBreakOutOfSupSub '+-=<>')
check("x^2n+y: the + leaves the power", type("x^2n+y"), "x^(2n)+y|")
check("x^+2n: a sign in an empty power stays", type("x^+2n"), "x^(+2n|)")
check("x^-2n stays", type("x^-2n"), "x^(-2n|)")
check("x^=2n stays", type("x^=2n"), "x^(=2n|)")
check("+ in the middle of a power stays", edit(StackedMath.breakOut("x^(ab)", caret: 4, typed: "+")), "nil")
check("= at a power's end leaves it", type("2^3=8"), "2^(3)=8|")
check("a fraction's slot is never left by +", type("1/2+3"), "1/(2+3|)")

// backspace (backspace.test.js)
check("⌫ after a power steps in", bs("x^(nm)|"), "x^(nm|)")
check("⌫ in an empty power takes it", bs("x^(|)"), "x|")
check("⌫ after an empty power takes it whole", bs("x^()|"), "x|")
check("⌫ after an empty fraction takes it whole", bs("1+()/()|"), "1+|")
check("⌫ in an empty denominator leaves the numerator", bs("1+1/(|)"), "1+1|")
check("⌫ in an empty inner denominator", bs("1+1/(1/(|))"), "1+1/(1|)")
check("⌫ at an inner fraction's start", bs("1/(1/(2)+|2/(3))"), "nil")
check("⌫ after a fraction in a denominator steps into it", bs("1/(1/(2)+2/(3)|)"), "1/(1/(2)+2/(3|))")
check("⌫ takes a fraction's dot with the one term after it", bs("1/(2)·x|"), "1/(2)|")

// ⌦ (delete through …)
check("⌦ at a numerator's end takes the fraction apart", del("(1|)/(2)"), "1|2")
check("⌦ at a plain numerator's end too", del("1|/(2)"), "1|2")
check("⌦ in an empty numerator", del("(|)/(1/(2)+2/(3))+1"), "|1/(2)+2/(3)+1")
check("⌦ at a denominator's end", del("1/(2|)"), "12|")
check("⌦ at a power's end", del("x^(2|)"), "x2|")
check("⌦ before an empty power takes it", del("x|^()"), "x|")
check("⌦ before an empty fraction takes it", del("1+|()/()"), "1+|")
check("⌦ before a full power is the line's (it steps in)", del("x|^(2)"), "nil")

// ↑ ↓ (updown.test.js)
check("↑ after a power goes into it, at its end", vert("x^(nm)|", up: true), "x^(nm|)")
check("↓ from a power's end leaves it after", vert("x^(nm|)", up: false), "x^(nm)|")
check("↓ from a power's start leaves it before", vert("x^(|nm)", up: false), "x|^(nm)")
check("↓ from a power's middle leaves it before", vert("x^(n|m)", up: false), "x|^(nm)")
check("↑ before a power goes into its start", vert("x|^(nm)", up: true), "x^(|nm)")
check("↑ after a fraction goes to the numerator's end", vert("(12)/(34)|", up: true), "(12|)/(34)")
check("↓ after a fraction goes to the denominator's end", vert("(12)/(34)|", up: false), "(12)/(34|)")
check("↑ before a fraction goes to the numerator's start", vert("|(12)/(34)", up: true), "(|12)/(34)")
check("↓ before a fraction goes to the denominator's start", vert("|(12)/(34)", up: false), "(12)/(|34)")
check("↓ in a numerator goes across", vert("(1|2)/(34)", up: false), "nearest(34)")
check("↑ in a denominator goes across", vert("(12)/(3|4)", up: true), "nearest(12)")
check("↑ in a numerator is the bar's", vert("(1|2)/(34)", up: true), "nil")
check("↑ in an inner numerator goes to the outer one", vert("1/(x+(a|)/(b))", up: true), "nearest(1)")
check("↑ with no stack near is the bar's", vert("1+|2", up: true), "nil")

// Home / End
check("⌘← in a slot goes to its start", edit(StackedMath.Edit(text: "1/(23)", caret: StackedMath.lineEdge(StackedMath.parse("1/(23)"), caret: 5, forward: false, length: 6))), "1/(|23)")
check("…and again to the line's", edit(StackedMath.Edit(text: "1/(23)", caret: StackedMath.lineEdge(StackedMath.parse("1/(23)"), caret: 3, forward: false, length: 6))), "|1/(23)")
check("⌘→ in a slot goes to its end", edit(StackedMath.Edit(text: "1/(23)+4", caret: StackedMath.lineEdge(StackedMath.parse("1/(23)+4"), caret: 3, forward: true, length: 8))), "1/(23|)+4")

// selections across a stack (select.test.js): one inside a slot stays, one past it takes the stack
check("inside a slot stays", sel("1+(23)/(45)", NSRange(location: 3, length: 1)), "2")
check("from a slot past the stack takes it whole", sel("1+(23)/(45)", NSRange(location: 4, length: 7)), "(23)/(45)")
check("from before the stack into it takes it whole", sel("1+(23)/(45)", NSRange(location: 0, length: 4)), "1+(23)/(45)")
check("a power's base and exponent take the power", sel("x^(2)+1", NSRange(location: 1, length: 2)), "^(2)")

// roots
check("√ over a selection puts it under the root", edit(StackedMath.rootKey("1/(2)", start: 0, end: 5, sign: "√")), "√(1/(2)|)")

// brackets grown around a stack
check("shown brackets around a fraction grow", StackedMath.parse("2(1/(2)+1)").tall.map { "\($0.open)-\($0.close)" }.joined(separator: " "), "1-9")
check("a root's brackets grow too, under its tall sign", StackedMath.parse("√(1/(2))").tall.count.description, "1")
check("a fraction's own hidden brackets don't", StackedMath.parse("(1)/(2)").tall.count.description, "0")
check("square brackets do", StackedMath.parse("[1/(2)]").tall.count.description, "1")

print(failures == 0 ? "all passed" : "\(failures) failed")
exit(failures == 0 ? 0 : 1)
