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
