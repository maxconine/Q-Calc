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

## Phase 1 — exact core (native/core)
1. Move `Rat`, `Rad`/`RadField`, `Mono`/`Rf`/`RfField`, `trait Field` and `eliminate<F>` out of `system.rs` into a
   shared `exact/` module (`pub(crate)`); `system.rs` imports them back. No behaviour change, golden must stay green.
2. Matrix entries become a cell enum: exact rational → surd → rational function in parameters → f64 fallback.
   `matrix.rs` evaluates in the most exact field that holds every entry; `Fail::Blank` on `k` goes away when `k` is
   unknown (it becomes a parameter) but a *stored* `k` is still substituted.
3. Existing functions (det, inv, rref, rank, trace, powers, products) run exactly through the new cells. Existing
   decimal answers stay identical (golden + `native_fixes.rs`).

## Phase 2 — new operations
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

## Phase 3 — answer shape (core + ffi)
- `LineResult.matrix: Option<MatrixParts>`: `parts: Vec<{label, literal, typst, display}>`, `values: Vec<…>` for
  eigenvalue lists (with multiplicity), and `steps: Vec<Step>` (`op` text like `R₂ → R₂ − 3R₁`, matrix after).
- `LiveAnswer` gets `matrix_parts`/`steps` with `#[uniffi(default)]`; Swift bindings regenerated.
- `typst_math.rs` writes multi-part rows as `$ L = mat(…) quad U = mat(…) $` and steps with `amat`.
- Storing: `L, U = lu(A)` and `P, D = diagonalize(A)` assign each part; `ans` keeps the first part.

## Phase 4 — mac UI
- Answer slot: matrices and multi-part answers show typeset (reuse `MathTypesetter`), Tab cycles exact/decimal.
- Steps panel under the composer (`BarView.below`, like `SystemPanel`), opened by a "steps" chip on the answer.
- Grid editor: `matrix 3x3` (and a ⌘-shortcut) opens a rows×cols field grid like `SystemPanel`; Tab/arrow move,
  `+`/`−` buttons resize, writes `[a b; c d]` back into the line. Editing an existing `[…]` reopens it in the grid.
- PARITY.md rows marked "Native only".

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
