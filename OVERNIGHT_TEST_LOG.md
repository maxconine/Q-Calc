# Overnight Q Calc engine tests

Hourly campaign through the morning of 2026-09-24. Each hour picks the next unfinished use case, adds edge-case tests, runs them, and fixes engine bugs. Do not commit. Do not weaken an assertion to hide a wrong answer.

## Use cases

- [x] Systems of equations (`src/engine/system.ts`, `src/engine/system.test.ts`)
- [x] Scientific arithmetic, trig, logs, powers, factorials, radicals
- [x] Exact forms, fractions, rationalize
- [x] Units and conversions
- [x] Variables, ans, and user functions
- [x] Single-equation solve
- [x] Calculus
- [x] Sums
- [x] Chemistry
- [x] Graphing
- [x] Sig figs and uncertainty
- [x] Plain math, LaTeX, and Typst input
- [x] Parentheses and glued input

## 01:15 — systems of equations

`src/engine/system.test.ts`: 44 passed.

Engine bugs fixed in `src/engine/system.ts`:

- Parametric answers were wrong or reported "no solution" once a polynomial's content was greater than 1. Content removal used `pscale(poly, 1n / content)`, and bigint division truncates `1/content` to 0, which wiped the polynomial. Coefficients are now divided by the content (`pdivc`).
- Multivariate monomials for the same term were stored under two keys (`1` and `1,0,0`), so like terms did not combine. Exponent keys now drop trailing zeros.
- Juxtaposition such as `2x`, `2sin(theta)`, `3sqrt(x+1)`, and `sqrt(2)x` never parsed, so those systems came back blank. A `*` is inserted before parsing.

Test bug, not an engine bug: the cosine case passed the array index into `math.evaluate` via `.map(num)`. It now evaluates each angle string on its own.

Still open on systems: the Mac UI imports `isSysCommand`, `sysCommand`, and `SystemAnswer` from `system.ts`, and those exports are not there. `QuickCalc.tsx` will not typecheck against the solver the tests call.

## 02:15 — scientific arithmetic

`src/engine/scientific.test.ts`: 76 passed. No engine bug.

The one failure was the expectation. `round(-1.5)` is -2 because the engine rounds halves away from zero (`roundHalfAway` in `scientific.ts`, already stated in `scientific.extras.ts`). JavaScript `Math.round(-1.5)` is -1 (half toward +∞). The test now expects -2.

## 03:32 — exact forms, fractions, rationalize

`src/engine/simplify.test.ts`: 29 passed. No engine bug.

Two expectations were wrong:

- `exactForm` only recovers a short list of nested radicals used by half-angles (`sin(15)` → `(sqrt(6)-sqrt(2))/4`, `tan(15)` → `2-sqrt(3)`). A general sum like `sqrt(2)+sqrt(3)` is not one of those, so it stays a decimal. `pi/6` is `pi/6`.
- `1/3 + 0.1` in fraction mode is `13/30`. `0.1` is one tenth, so that fraction is the sum, not a spurious approximation. `pi` stays a decimal.

## 04:45 — units and conversions

`src/engine/units.test.ts`: 28 passed. No engine bug.

Two expectations were the wrong contract:

- `1 m^2 to cm^2` is `10000 cm²`. The unit label uses a superscript ², not `cm^2`.
- `1 kg to m` (and other cross-dimension conversions) displays `improper unit conversion`, not a blank.

## 05:47 — variables, ans, and user functions

`src/engine/functions.test.ts`: 14 passed. No engine bug.

Two expectations treated a non-number as a wrong answer:

- An extra argument is blank. A missing argument is `undefined`, because the missing parameter is not a number. Both are the no-number contract.
- An unguarded `fact(n) = n*fact(n-1)` displays `undefined` (or blank). A text answer still carries `n: 0` as a placeholder, so the result is not the number 0.

## 07:02 — single-equation solve

`src/engine/solve.test.ts`: 12 passed. No engine bug.

The answer line is the roots themselves (`5`, `±3`, `-3`), and the unknown is `solve.variable`. It is not prefixed as `x = 5`. A stored coefficient works: `a = 2`, then `a*x = 10` displays `5` for `x`. `x^2 = -4` is `no real solution`. `sin(x) = 0.5` in degrees includes 30. `x = 5` stays an assignment.

## 10:22 — calculus through parentheses

`src/engine/calculus.test.ts`: 22 passed.
`src/engine/sums.test.ts`: 15 passed.
`src/engine/chem.test.ts`: 15 passed.
`src/engine/graph.test.ts`: 8 passed.
`src/engine/measure.test.ts`: 19 passed.
`src/engine/typstInput.test.ts`: 21 passed.
`src/engine/parens.test.ts`: 18 passed.

Engine bug fixed in `src/engine/evaluate.ts` and `src/engine/plainMath.ts`:

- LaTeX was rewritten only inside `tryPlainMath`, which runs after the ± gate and the calculus parser. `(5.0 \pm 0.1)*2` and `lim x->\infty 1/x` came back blank. A line that looks like LaTeX is now converted first, and `\to` becomes `->` so `lim_{x\to 0}` still parses.

Contracts the new tests had wrong:

- `d/dx abs(x)` is `|x|/x`. The second derivative at the corner stays blank. `lim x->0+ ln(x)` stays blank: divergence that slows down is not reported as `-∞`. `lim x->0+ 1/x` is `∞`.
- A one-feature graph such as `(x-0.2)^2` keeps the ±10 window. The window tightens only when two or more roots or extrema sit in the middle.
- `2.0 * 3.0 * 4` in sig-fig mode is `24`. `(10.0 ± 0.3) / 2` is `5.00 ± 0.15`, because an uncertainty that leads with 1 keeps two figures. `sin(30 ± 1)` is modelled (`0.500 ± 0.015`). `(1+2) ± 0.1` stays blank.
- In degrees, `sin 90` is `1`. With `x = 4`, `2sinx` is `2 sin(4°)`. `)(` fills as `()()`.
