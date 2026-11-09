# Linear algebra: Mac changes to port and test on Windows

To be folded into `native/windows/MAC_CHANGES_TO_PORT.md` when branch `linear-algebra` is merged. The plan is
`native/MATRIX_PLAN.md`.

## Checks to run first

The `eval.matrix` golden floor is now 420, on purpose: `inv([1 1; 1 1.0000000001])` gives the exact inverse of the
matrix as typed instead of "nearly singular", and `[1 2; 3 4] * y` gives `[y 2*y; 3*y 4*y]` (a lone letter beside a
written matrix is an unknown) where the TS is blank. If Windows passes a different count, tell the Mac side rather
than raising it. `eval.arithmetic` may show one failure, a 2001-term `1+1+…` the TS blanks by running out of stack:
it fails the same on the Phase 3 commit run today, so it isn't from this branch.

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

### Written the textbook way (core only, nothing to port)

| Type | Expect |
| --- | --- |
| `2x + y = 5, x - y = 1` (or `;`, or `solve …`) | `x = 2, y = 1`; `solve(…, …)` stays the current app's call |
| `[1 2 3]·[4 5 6]`, `[1 2 3] . [4 5 6]`, `u · v` | `32` (a column times a row is still the outer product) |
| `[1 2 3] × [4 5 6]` | `[-3 6 -3]`; `2 × A` still scales |
| `A .* B`, `A ./ B` | entry by entry |
| `\|[1 2; 3 4]\|`, `\|[3 4]\|` | `-2`, `5` |
| `x = [1; 2; 3]`, then `x^T x` | `14` |
| `projection of [1 2] onto [3 4]`, `angle between u and v`, `the dot product of u and v`, `magnitude of v`, `unit vector v` | the call's answer |
| `basis for the null space of A`, `dim null A`, `least squares A b` | the call's answer |
| `is A invertible?`, `is A positive definite`, `are u, v and w linearly independent` | `yes (det = -2)`, `no (rank 1 of 2)`… |
| `find eigenvalues of A`, `what is the rank of A?`, `compute det A` | the bare question's answer |
| `cofactor matrix of A`, `minor M12 of A`, `the (1, 2) cofactor of A` | the grid, `-6`, `6` |
| `rotation matrix 30°`, `rotation matrix about the z axis by 90°`, `rot(30°)` | 2×2 / 3×3 rotation; `°` inside a matrix entry works |
| `is [1 2 2] in the span of [1 0 0], [0 1 1]` | `yes (weights 1, 2)` |
| `coordinates of [3 5] in basis [1 1], [1 -1]` | `[4; -1]` |
| `orthogonal complement of [1 2 3]` | `span{[-2; 1; 0], [-3; 0; 1]}` |
| `singular values of [3 0; 0 4]` | `σ = 4, 3` |
| `change of basis from B (to C)` | `P = …, P⁻¹ = …` or `P = C⁻¹B` |
| `outer product of [1 2] and [3 4]` | `[3 4; 6 8]` |
| `A^(1/2)`, `A^0.5`, `matrix square root of A` | the matrix square root (was "power must be a whole number") |

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

### After Phase 4: textbook forms (core)

| Type | Expect |
| --- | --- |
| `A = [2 1; 1 3]`, `b = [3; 5]`, then `Ax = b` (or `A x = b`, `A*x = b`, `solve Ax = b`, `Ax = b for x`) | `x = [0.8; 1.4]`, exact `x = [4/5; 7/5]` |
| …then `solve(A, b)` | `[0.8; 1.4]` (it was blank: the systems solver took it) |
| `[2 1; 1 3] x = [3; 5]` | `x = [0.8; 1.4]` |
| `characteristic equation of [1 2; 3 4]` | `λ^2 - 5*λ - 2 = 0,  λ = 5.37228, -0.372281` |
| `det(A - λI)`, `A - 2I` | `λ^2 - 5*λ + 5`, `[0 1; 1 1]` |
| `A = [1 2 3; 4 5 6]`, then `A[2, 1]`, `A[1, :]`, `A[:, 3]` | `4`, `[1 2 3]`, `[3; 6]` |
| `unit([1 1])` exact form | `[sqrt(2)/2 sqrt(2)/2]` (was `1/2*sqrt(2)`) |
| `det(A)` with no A stored | still blank |

`MatrixView.message` is set when the answer says why there is none (`determinant needs a square matrix`): the Mac
shows it as a 13 pt muted message, not typeset in the preview. The cheat sheet gains `matrix 3x3` and `Ax = b`
(core `NATIVE_CHEATS`), and a keys row for ⌃G ⌃R. A matrix answer's slot text gets real minus signs and drawn roots
(`Pretty.matrix`: `−` leading each entry, `sqrt(` as `√(`), and when both halves of the dual start `λ = ` the
approximation drops it. A step's matrix is set against the left of its row.

## Phase 5: already ported, check it on Windows

The Mac side wrote the Windows port of §2 (`src/matrices.rs`, `src/matrix_text.rs`). It type-checks and is
clippy-clean for `x86_64-pc-windows-msvc` and `matrix_text`'s tests pass, but nobody has run it: build it, drive the
table in §2 and the rows below, screenshot light and dark, and run the keystroke bench (nothing new runs on a plain
line: `follow` is one `grid_hint` scan, the steps only on an answer that has them).

Hub edits, one line or a few each: `model.rs` (`matrix` field and its default; `matrices::follow` in
`evaluate_live`; `matrices::key_down` before the systems' in `key_down`; `char16`, `insert_text`), `window.rs`
(`BarLayout::matrix`, its height after the typst row, its paint, the line's caret hidden while a cell has the keys),
`app.rs` (`click`, `WM_MOUSEWHEEL`, the bench state's `matrix`), `keys.rs` (the cheat sheet's `Ctrl+G Ctrl+R` row),
`main.rs`. Also `answer.rs` (`slot_hint`, matrix messages muted, `pretty`, the dual's label once, `refresh_steps`
when an answer lands) and `typst_row.rs` (`Request::matrix`, `draw` shared with the steps' pictures, `trim`).

| Do | Expect |
| --- | --- |
| type `matrix 3x3` | slot: `↵ opens a 3×3 grid`, muted; ↵ opens it, the line reads `[0 0 0; 0 0 0; 0 0 0]` |
| type `matrix`, then `det(matrix`, then `matrix 9x2` | slot: `what size? rows × columns: matrix 2x2, matrix 3x3…`, the same, then `1 to 8 rows and columns · matrix 8x8 is the biggest` (core text, no Windows code) |
| in the grid, ↑ from the top row, then type on the line | keys back on the line with the grid open; the edit closes it |
| `[1 2; 3 4]`, Ctrl+G, select a cell's text, Ctrl+C | the cell's text is copied, not the answer |
| `det([1 2 3])` | `determinant needs a square matrix`, 13 pt muted; the preview shows no answer after `=` |
| `characteristic equation of [1 2; 3 4]` | dual `λ² − 5λ − 2 = 0,  λ = (5 ± √33)/2 ≈ 5.37228, −0.372281` |
| `inv` of a 6×6 with steps open | the card stops at 320 and the wheel scrolls it |

The Mac fixed its dual to match (`BarModel.dualApprox`): the shared label comes off the plain text before styling, since `λ^2` styled as `λ²` is a character shorter.

Known gaps: Ctrl+G and Ctrl+R are fixed chords (the core's keybinds have no matrix actions; a user binding on the
same chord wins); a click in a cell puts the caret at the end (double-click selects it) rather than under the
pointer; the grid doesn't scroll sideways (8 columns need about 590 px); no UI Automation for the cells yet.

## 4. Mac grid changes after Phase 5 (to port)

| Mac does | Windows should |
| --- | --- |
| `mat 2x2` / `matrix 3x3` lit as a known word with hover name `matrix`, no squiggle left from `mat` alone (`BarModel.matrixWord`) | the same in `matrices::follow`'s hint path |
| ↵ on `mat 2x2`: the first cell gets the keys once the card is in the window (focus set on the next run-loop turns) | check the caret lands in row 1, column 1 |
| ⌘⌫ in a cell empties it; in an empty cell the grid closes and the line clears (as `sys`) | Ctrl+Backspace the same |
| hiding with the grid open saves it with the draft (`grid: start, end, r, c`); shown again within the draft time, the grid reopens on that cell | the same with the draft |
| a bare `mat` asks for its size too (core), unless history stores a `mat` (`storesMat`) | the same |
| moving to a cell (arrows, Tab, ↓ back) puts the caret at the end of its value, never drawn selected first (`OverlayPanel.makeFirstResponder` sets it in the same turn, while the grid or a system is open) | the same |
| ↓ on the line while the grid is open goes to its first cell | the same |
| a line committed with a grid-written matrix is remembered (`GridMemory`, newest 200); inserted again from history, the grid opens on its matrix | the same |

## 2. Mac UI to port (Phase 4)

Mac code: `mac/Sources/Bar/Matrices.swift` (new), with hooks in `BarModel.swift` (`evaluateLive`, `setLive`,
`refreshTypeset`, `enter`, `escape`, `perform`), `BarView.swift` (`below`), `MathText.swift` (`MatrixAsk`,
`MathTypesetter.render`) and `Keybinds.swift`. The text rules are in the core, so Windows calls the same functions:
`matrix_grid_command`, `matrix_grid_hint`, `matrix_grid_at`, `matrix_grid_literal`, `matrix_grid_max`
(`ffi/src/matrix_grid.rs`), and `Engine::typst_source_math` for the preview. All native only.

1. **Typeset matrix answers.** When `LiveAnswer.matrix` has `typeset` set (a matrix in it, or several parts), the
   preview row typesets `Engine::typst_source_math(expr, view.exact_typst ?? view.typst, view.labeled)` instead of
   `typst_source`, skipping the `typstPreviewUseful` check; not while a cycled answer form or a prefix shows. The
   answer slot is unchanged.
2. **Steps.** When `matrix.steps` isn't empty, a muted chip under the preview reads `show steps · N` (`hide steps`
   when open); it, or ⌃R (keybind `matrixSteps`), opens a card (the `sys` card's look) of rows: the operation (13 pt
   muted, 168 pt column) and the step's `matrix_document` typeset beside it, at most 72 pt high; a remark (empty
   `matrix_document`) is one line of ink text. The card scrolls past 320 pt. Esc shuts it; an emptied line starts
   with it shut. With the preview setting off, rows show `display` as text.
3. **Grid.** While the line holds `matrix 3x3` the answer slot shows `matrix_grid_hint` (`↵ opens a 3×3 grid`); ↵
   opens a rows × columns grid of 64 × 34 pt cells in the `sys` card, between drawn square brackets, `0` as each
   empty cell's placeholder. Each keystroke writes `matrix_grid_literal(cells)` into the line in the grid's place
   (UTF-16 offsets from the spot), caret after it, so the answer stays live (`det(matrix 2x2)`). Any other edit of
   the line closes the grid. Keys: Tab / ⇧Tab along the rows, ↵ down the column (from the last row the grid closes
   to the line), arrows between cells (← → only from the ends of the cell's text; ↑ from the top row to the line),
   esc closes. `− +` buttons for rows and columns (1 to 8). ⌃G (keybind `matrixGrid`) opens `matrix_grid_at` (the
   matrix under the caret, else the last), else a 2×2 at the caret; again, it closes.

| Do | Expect |
| --- | --- |
| type `diagonalize([4 1; 2 3])` | preview: the line ⇒ `P = [1 −1; 1 2]  D = [5 0; 0 2]` typeset |
| type `eig([1 2; 3 4])` | preview: `λ = (5 ± √33)/2` |
| type `rref([1 2 \| 5; 3 4 \| 6])`, click `show steps · 4` | Start, R₂ → R₂ − 3R₁, R₂ → −(1/2)R₂, R₁ → R₁ − 2R₂, augmented bars drawn |
| type `det(matrix 2x2)`, ↵, type 1 Tab 2 Tab 3 Tab 4 | line reads `det([1 2; 3 4])`, answer `-2` |
| line `det([1 2; 3 4])`, ⌃G | grid opens on 1 2 / 3 4; `+` columns makes it 2×3, line `det([1 2 0; 3 4 0])` |
