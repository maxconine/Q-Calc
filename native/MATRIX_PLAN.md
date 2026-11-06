# Linear algebra plan

Goal: any undergrad linear algebra problem can be worked in Q Calc. Answers are exact when possible, matrices may hold
variables, multi-part answers show as one labeled typeset row, row operations have a steps view, and matrices can be
typed as text or filled in a grid. Native mac + windows only (no TS port). Every mac-side change is logged in
`native/windows/MAC_CHANGES_TO_PORT.md`.

## Decisions (from the user)
- Features: everything useful (list below).
- Exact first: fractions, radicals, `i`; decimals as the alternate (Tab), and as the fallback when no exact form exists.
- Symbolic entries allowed: `det([k 2; 3 k])` → `k² − 6`.
- Multi-part answers: one typeset row, `L = […]  U = […]`; each part can be stored.
- Steps view: "show steps" lists each row operation.
- Entry: text (`[1 2; 3 4]`, `[[1,2],[3,4]]`, words) **and** a grid editor.

## Phase 1 — exact core (native/core) — done

Landed as `core/src/exact.rs` (moved from `system.rs`) and `core/src/matrix/exact_eval.rs`, tested in
`core/tests/linear_algebra.rs`. Float answers still lead; the exact form rides in `exact` and `MatrixAnswer.stored`
(what a stored matrix keeps), and replaces the answer only where the floats give up or a symbol is unknown. A bare
literal with symbols stays blank, as in the TS, but is stored. Divergence from the TS: `inv([1 1; 1 1.0000000001])`
is answered exactly (golden `eval.matrix` floor 421).
1. Move `Rat`, `Rad`/`RadField`, `Mono`/`Rf`/`RfField`, `trait Field` and `eliminate<F>` out of `system.rs` into a
   shared `exact/` module (`pub(crate)`); `system.rs` imports them back. No behaviour change, golden must stay green.
2. Matrix entries become a cell enum: exact rational → surd → rational function in parameters → f64 fallback.
   `matrix.rs` evaluates in the most exact field that holds every entry; `Fail::Blank` on `k` goes away when `k` is
   unknown (it becomes a parameter) but a *stored* `k` is still substituted.
3. Existing functions (det, inv, rref, rank, trace, powers, products) run exactly through the new cells. Existing
   decimal answers stay identical (golden + `native_fixes.rs`).

## Phase 2 — new operations — done

Landed in `core/src/matrix/`: `exact_eval.rs` (the fields, now with a float field, and the evaluator), `algebra.rs`
(generic algorithms), `eigen.rs`, `functions.rs` (every name), `show.rs` (answer text in labeled parts),
`numeric.rs` (float kernels: polynomial roots, symmetric eigen, SVD, expm, sqrtm, complex null space). Tested in
`core/tests/linear_algebra_more.rs`. New names count only on a line with a matrix (`zeros`, `ones`, `rot` always), and
lines using them skip the float reader. Multi-part answers are joined text for now (`P = […],  D = […]`); Phase 3
exposes the parts. Not done: `sqrtm` beyond Denman–Beavers, exact singular values, Jordan for repeated irrational
eigenvalues, `A[i, j]` indexing (use `entry(A, i, j)`), and `ans`/storing of multi-part answers.
| Area | Functions (plus word forms like "eigenvalues of A") |
|---|---|
| Eigen | `eig`/`eigenvalues`, `eigenvectors`/`eigvec`, `charpoly`, `diagonalize` (P, D), algebraic/geometric multiplicity, `jordan` (small sizes) |
| Systems | `A\b`, `solve(A, b)`, augmented `rref([A|b])` read as a system: unique / parametric (`x₃ = t`) / no solution; `lsq(A, b)` least squares |
| Subspaces | `null`/`kernel`, `col`/`range`, `row`, `leftnull`, `basis`, `dim`, `nullity`, `gramschmidt`/`orth`, `proj(u, v)`, `proj(v, A)` onto a column space, `angle(u, v)`, `unit(v)` |
| Entries | `adj`, `cofactor`, `minor(A, i, j)`, `A[i, j]` / `row(A, i)` / `col(A, j)` indexing, `kron`, `hadamard`/`.*`, `augment`, `stack` |
| Decompositions | `lu` (with P), `qr`, `svd`, `cholesky`, `pinv`, `cond` |
| Checks | `issymmetric`, `isorthogonal`, `isdiagonalizable`, `isinvertible`, `posdef`, `islinind(v1, v2, …)`, `span` membership |
| Other | `expm`, `sqrtm` (diagonalizable), `zeros`, `ones`, `diag`, `rot(θ)`, change of basis, `cross`/`dot` already exist |

Exact eigenvalues: char poly exactly (Faddeev–LeVerrier / Bareiss over the field); pull rational roots, then the
quadratic `closed_form` from `solve.rs` (lifted to `exact/`, extended to `a ± b·i·√r`); anything left is numeric
(QR / complex `C`). Eigenvectors come from the exact null space of `A − λI` when λ is exact.

## Phase 3 — answer shape (core + ffi) — done

Landed as `LineResult.matrix: Option<MatrixParts>` (`#[serde(skip)]`, so the TS JSON and golden are unchanged):
`parts` (label, display, exact, literal, note, typst, exact_typst; a plain answer is one unlabeled part), `steps`
(`core/src/matrix/steps.rs`: `Start`, then each swap, scaling and column clear as `R₂ → R₂ − 3R₁` with the matrix
after it, for `rref`, `inv`, `solve`/`A \ b`, `det` and `rank`, exact where the entries allow; a closing remark for
`det`, `rank`, `inv`), and `stored`. FFI: `LiveAnswer.matrix` and `LineAnswer.matrix: Option<MatrixView>` with ready
preview documents (`document`, `exact_document`, one per step; `typst_math::math_document`), Swift bindings
regenerated. `L, U = lu(A)` stores each part (in order, or by label when fewer names are given), `ans` the first;
history reads such rows, and matrices with unknowns, back through `evaluate::matrix_stores`. Tested in
`core/tests/linear_algebra_parts.rs` and `ffi/tests/matrix_view.rs` (every document compiles with Typst).
Eigenvalue lists stay one part (`λ = 5, 2`), not one per value.

## Phase 4 — mac UI — done

Landed in `mac/Sources/Bar/Matrices.swift` with hooks in `BarModel`, `BarView`, `MathText` and `Keybinds`; the grid's
text rules in `core/src/app/matrix_grid.rs` (exported by `ffi/src/matrix_grid.rs`), the preview document by
`Engine::typst_source_math`. The one-line answer slot stays text (parity); matrix answers are typeset in the
preview row under it, exact where they can be. Steps: a `show steps · N` chip and ⌃R open a card of operations with
typeset matrices. Grid: `matrix 3x3` + ↵ (anywhere in a line), ⌃G on a typed matrix; Tab / ↵ / arrows, `− +`
resize, written live into the line. PARITY.md rows added. Not done: the cheat sheet doesn't list `matrix 3x3`, ⌃G
or ⌃R; a step's matrix is centred in its row rather than set against the operation.

## Phase 5 — windows UI
Same three pieces through the README "Adding a panel" pattern (steps panel, grid editor from `SysState`), within the
budgets in `native/windows/AGENTS.md`. Done by porting from `MAC_CHANGES_TO_PORT.md` (core/ffi already landed).

## Testing
- Unit tests per operation in `matrix.rs`/`exact/` with known textbook answers (exact and decimal).
- Golden (`native/core/golden`) unchanged for existing lines; new cases in `tests/native_fixes.rs`.
- Run the real mac app: typeset row, steps, grid, light + dark.

## Order of work
Phase 1 → 2 (eigen + systems first, then subspaces, entries, decompositions, checks) → 3 → 4 → 5. Each phase is
committed on its own.
