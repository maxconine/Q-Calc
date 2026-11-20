# Sheet explanations

A click on an equation row in a reference sheet opens its explanation. Each file here is one JSON object that maps a
row's key to its explanation. `identities.rs` loads every file listed in `EXPLANATION_FILES` and attaches each
explanation to its row as `explain`. `core/tests/explanations.rs` checks the files.

## The key

The key is the row's `lead` cells and then its `show`, joined by ` ‖ ` (space, U+2016, space). It must match
exactly, with the same Unicode minus signs, `^{…}` and `_{…}`.

- `τ = Tρ/J`
- `e^{−at} u(t), a > 0 ‖ 1/(a + jω)` (a table row: the time cell, then the frequency cell)
- `30° ‖ π/6 ‖ 1/2 ‖ √3/2 ‖ √3/3` (a unit circle row)

## An entry

See `mechanics.json` (`τ = Tρ/J`) for a complete one.

```json
"τ = Tρ/J": {
  "title": "Torsion formula: shear stress in a twisted shaft",
  "about": "1–3 plain sentences: what it gives and when you reach for it.",
  "assumptions": ["when it holds; what breaks it", "..."],
  "vars": [["τ", "shear stress at radius ρ", "Pa"], ["T", "internal torque", "N·m"]],
  "related": [["J = πc^{4}/2", "solid shaft of radius c"]],
  "example": {
    "problem": "A short, realistic problem with numbers.",
    "steps": ["one step per line, with the numbers put in"],
    "answer": "τ_{max} ≈ 61.1 MPa"
  },
  "try": "1.5 kN m * 25 mm / (pi (25 mm)^4 / 2) to MPa"
}
```

- `title`, `about` and `example` (with `problem` and `answer`) are required. The other fields may be empty or left
  out.
- In any text, `^{…}` raises and `_{…}` lowers, as in `show`. Use real symbols (−, ×, ·, √, π, ≈, ≤).
- In `vars`, the third cell is the SI unit, or `""` for none.
- `related` holds the companion formulas a student needs alongside this one, such as J for a solid and a hollow shaft.
- `try` is optional. It is one line typed into the Q Calc bar that works out the example's answer. Use units where
  they help (`kN m`, `mm`, `to MPa`). Leave it out when the bar can't do it (symbolic results, proofs, identities).
  Every `try` must answer in the bar, and its answer must agree with `example.answer`.

## Checking

```bash
cargo test -q -p qcalc-core --test explanations
EXPLAIN_FILE=mechanics cargo test -q -p qcalc-core --test explanations -- --nocapture show
```

The second command prints each Try line beside the bar's answer and the answer the example states.
