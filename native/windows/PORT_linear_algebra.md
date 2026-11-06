# Linear algebra: Mac changes to port and test on Windows

To be folded into `native/windows/MAC_CHANGES_TO_PORT.md` when branch `linear-algebra` is merged. The plan is
`native/MATRIX_PLAN.md`.

## Checks to run first

The `eval.matrix` golden floor is now 421, because `inv([1 1; 1 1.0000000001])` now gives the exact inverse of the
matrix as typed instead of "nearly singular". If Windows passes a different count, tell the Mac side rather than
raising it.

## 1. Comes from the core: just test it

Exact and symbolic matrices (`core/src/matrix/exact_eval.rs`, on the shared exact arithmetic in `core/src/exact.rs`).
No Windows code should be needed. Long answers such as `[k^2 2*k; 0 k^2]` must fit the answer slot.

| Type | Expect |
| --- | --- |
| `det([k 2; 3 k])` | `k^2 - 6` |
| `det([a b; c d])` | `a*d - b*c` |
| `trace([a b; c d])` | `a + d` |
| `[k 1; 0 k]^2` | `[k^2 2*k; 0 k^2]` |
| `inv([k 0; 0 2])` | `[1/k 0; 0 1/2]` |
| `A = [k 2; 3 k]`, then `det(A)` | blank, then `k^2 - 6` |
| `k = 4`, then `det([k 2; 3 k])` | `4`, then `10` |
| `det([sqrt(3) 1; 0 2])` | `3.46410161514`, exact form `2*sqrt(3)` |
| `norm([1 2 3])` | `3.74165738677`, exact form `sqrt(14)` |
| `inv([7 0; 0 1001])` | decimals, exact form `[1/7 0; 0 1/1001]` |
| `inv` of the 5×5 Hilbert matrix | whole numbers starting `[25 -300 1050 -1400 630; …` |
| `det(A)` with no A stored | blank, as before |

### Phase 2: the new functions (`core/src/matrix/{functions,algebra,eigen,numeric,show}.rs`)

All core; no Windows code. Answers with several parts are one line for now (`P = […],  D = […]`), so check they
fit or scroll. New names count only on a line with a matrix in it (`zeros`, `ones`, `rot` always).

| Type | Expect |
| --- | --- |
| `eig([1 2; 3 4])` | `λ = 5.37228, -0.372281`, exact form `λ = (5 ± sqrt(33))/2` |
| `eig([1 -2; 2 1])` | `λ = 1 ± 2i` |
| `eigenvectors([4 1; 2 3])` | `λ = 5: [1; 1],  λ = 2: [-1; 2]` |
| `charpoly([1 2; 3 4])` | `λ^2 - 5*λ - 2` |
| `diagonalize([4 1; 2 3])` | `P = [1 -1; 1 2],  D = [5 0; 0 2]` |
| `jordan([2 1; 0 2])` | `[2 1; 0 2]` |
| `[2 1; 1 3] \ [3; 5]` | `[0.8; 1.4]` |
| `solve([1 2 3; 2 4 6], [6; 12])` | `x = [6; 0; 0] + t*[-2; 1; 0] + s*[-3; 0; 1] (any t, s)` |
| `rref([1 2 \| 5; 3 4 \| 6])` | `[1 0 -4; 0 1 4.5]` |
| `null([1 2 3; 2 4 6])` | `span{[-2; 1; 0], [-3; 0; 1]}` |
| `lu([0 1; 1 0])` | `P = …,  L = …,  U = …` |
| `svd([3 0; 0 4])` | `U = [0 1; 1 0],  Σ = [4 0; 0 3],  V = [0 1; 1 0]` |
| `isposdef([2 -1; -1 2])` | `yes` |
| `angle([1 0], [0 1])` in degrees | `90` |
| `row = 3`, then `row + 1` | `4` (an ordinary name, untouched) |

### Phase 3: parts, steps, storing parts (core + ffi)

No Windows UI yet (that is Phase 5). The FFI records gained `LiveAnswer.matrix` and `LineAnswer.matrix:
Option<MatrixView>` (`parts`, `steps`, `document`, `exact_document`; see `QCalcCore/API.md`). A test helper that
builds a `LiveAnswer` field by field needs `matrix: None` (done in `windows/src/answer.rs`). The answer text is
unchanged, so these only need checking:

| Type | Expect |
| --- | --- |
| `A = [4 3; 6 3]`, then `L, U = lu(A)`, then `L * U` | `L = [1 0; 1.5 1],  U = [4 3; 0 -1.5]`, then `[4 3; 6 3]` |
| `P, D = diagonalize([4 1; 2 3])`, then `ans` | `ans` is `[1 -1; 1 2]` (the first part) |
| `L, U = lu([0 1; 1 0])`, then `U` | `[1 0; 0 1]` (names matched by label) |
| `Q, R = eig([1 2; 3 4])` | `this answer has 1 part (λ); give 1 name` |
| `M = 2 * [k 1; 1 k]` committed, then `det(M)` on the next line | `4*k^2 - 4` (history reads M back) |
