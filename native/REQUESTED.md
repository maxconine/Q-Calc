# Requested features: native check

Every feature request and behaviour fix from the user's own messages in the Q Calc Claude Code chats (all sessions
under `~/.claude/projects/*Q-Calc*`, 2026-09-30 to 2026-10-28), checked against the native app. One-off questions,
release/push/merge requests and the study-notes sessions are left out. Also left out here, since other agents own
them: game window sizing (game full screen, the Notion-style window), hover names for functions and units, and
the Typst preview (when it shows, `+ 2`, matrices, long and unfinished lines, `&`). Windows-only fixes (the
Windows smoke test, opening from search, `alt+q` on Windows) are out of scope for the Mac app.

Status: ✅ tested (a test covers it) · 🔧 fixed here · ❌ missing · 🖥 UI-only (checked in the Swift code and
`mac/PARITY.md`, no core test). Test names without a path are in `core/tests/requested_features.rs`.

## Engine and app logic

| Request | Date | Status | Test |
|---|---|---|---|
| `+ 2` (an operator first) works on the previous answer | 2026-09-30 | ✅ | `an_operator_first_chains_from_the_last_answer` |
| Stress/strain and m/s sheets don't mix units | 2026-09-30 | ✅ | `stress_and_strain_sheets_keep_their_units` |
| Answer unit follows the system typed (3mm+8mm in mm, 3in+4in in in) | 2026-09-30 | ✅ | `answers_stay_in_the_system_typed` |
| Free up `E` as a variable | 2026-09-30 | ✅ | `capital_e_is_free_for_a_variable` |
| Matrix add, subtract, scalar, multiply, transpose, determinant, inverse, trace | 2026-09-30 | ✅ | `matrix_operations` |
| `isolate x`, incl. the `R_("eq")` resistor line | 2026-09-30 | ✅ | `isolate_a_variable` |
| `solve x` isolates when there's no numeric solution | 2026-09-30 | ✅ | `solve_falls_back_to_isolating` |
| Partial derivatives | 2026-09-30 | ✅ | `partial_derivatives` |
| `sys3`, solve with equations still empty, with units | 2026-09-30 | ✅ | `systems_of_equations` |
| User-chosen keybinds | 2026-09-30 | ✅ | `keybinds_are_the_users` |
| Usage analytics (downloads, daily use, features) | 2026-09-30 | ✅ | `app::analytics` unit tests (`counts_per_day_and_keeps_a_month`, `sorts_a_calculation_into_one_bucket`, `never_carries_anything_typed`) |
| Copied answers unitless, a setting on by default | 2026-10-02 | ✅ | `copy_unitless_and_six_sig_figs_by_default` |
| Tab completes the auto parens, as → does | 2026-10-02 | ✅ | `tab_fills_the_grey_parens` (the key handling is Swift, `BarTextView`) |
| Units shown green | 2026-10-06 | ✅ | `units_are_found_to_colour` (the colour itself is Swift) |
| `solve(0.5 = e^(-t/0.384 "milliseconds"), t)`: units, parens, named variable | 2026-10-06 | ✅ | `solve_with_units` |
| `hello` says hi back | 2026-10-06 | ✅ | `hello_says_hi` |
| `solve(x^2 - cos(x) = 0, x = 0.2)` takes a starting guess | 2026-10-07 | ✅ | `solve_with_a_starting_guess` |
| Time derivatives of x sinθ, L cos²θ, θ²θ̇, √(x²+h²), e^{kθ}θ̇ and similar | 2026-10-08 | ✅ | `time_derivatives` |
| deg / rad in inverse trig answers, every way of writing the inverse | 2026-10-09, 10-10 | ✅ | `inverse_trig_names_its_angle_unit` |
| `(1.1009 "in" "to" "mm") / 10`, `ans in to mm`, operations keep the answer's units | 2026-10-09 | ✅ | `conversions_on_the_last_answer` |
| `549.16 Nm / (0.293 deg)` is N m/deg (N m/rad in rad mode); kg m²/s² simplified | 2026-10-09 | ✅ | `torque_per_angle_keeps_its_units` |
| `game` / `games` lists the games | 2026-10-09 | ✅ | `games_list` |
| phi, hex and binary, complex results; space-thousands a setting (a space multiplies by default) | 2026-10-11 | ✅ | `phi_bases_spaces_and_complex` |
| Metric prefixes / conversions sheet on `conversion` or `metric prefixes` | 2026-10-12 | ✅ | `reference_sheets_open_by_name` |
| Missing volt: no answer, the unit offered in grey, ↵ again fills it (`10 mA + 5 / 1 kohm …`) | 2026-10-14 | ✅ | `a_missing_volt_is_offered` |
| Tab completes in the middle of an expression (no fraction mode) | 2026-10-14 | ✅ | `tab_completes_inside_an_expression`; forms moved to ⌃E (Swift) |
| `solve tan(2θ) = 4/3` gives every angle, each pickable | 2026-10-16 | ✅ | `every_angle_of_a_trig_solve` (clicking one is Swift) |
| `ans mm to in` gives ans the unit, then converts | 2026-10-23 | ✅ | `conversions_on_the_last_answer` |
| `graph y = x^2 {2 < y < 5}`, several bounds | 2026-10-23 | ✅ | `graph_bounds_and_two_variable_functions` |
| `graph f(x,y) = x^2 + y^2`, level curves | 2026-10-23 | ✅ | `graph_bounds_and_two_variable_functions` |
| `message max hello` to nearby users, name set in settings | 2026-10-23 | ✅ | `message_commands`; `app::messaging` unit tests |
| Tutorial turns off after a few opens | 2026-10-23 | ✅ | `the_tutorial_retires_after_a_few_opens` |
| `chem R` with units to pick (atm, bar, torr, mmHg, cal) | 2026-10-23 | ✅ | `gas_constant_in_its_units` |
| Integration in the identities; `trig` opens them; `idt` + trig / integration / partial fractions / summation | 2026-10-23 | ✅ | `reference_sheets_open_by_name` |
| Several picked elements make one formula | 2026-10-23 | ✅ | `periodic_picks_make_one_formula` |
| Constants table on `constants` / `idt constants` | 2026-10-23 | ✅ | `reference_sheets_open_by_name` |
| Remove SoulverCore; the phrases it answered keep working | 2026-10-23 | ✅ | `soulver_style_phrases_still_answer`; phrase goldens |
| `f(t) = t*t` derivative bug | 2026-10-26 | ✅ | `derivative_of_a_defined_function` |
| Paste equations into the solver (`20(7.5)-B_y(5)+40(2)` and `B_y+D_y = 60`) | 2026-10-26 | ✅ | `pasted_equations_solve` |
| Auto parens: grey parens go with the expression, Tab / → fill them | 2026-10-27 | ✅ | `tab_fills_the_grey_parens` (drawing is Swift) |
| Default significant figures 6 | 2026-10-27 | ✅ | `copy_unitless_and_six_sig_figs_by_default` |
| `open messages with max2`, threads with past contacts | 2026-10-27 | ✅ | `message_commands`; `app::messaging::reads_the_conversation_commands` |
| Custom hotkey (e.g. `alt+q`) is kept | 2026-10-18 | ✅ | `keybinds_are_the_users` |
| Default keybind ⌘⌥Space | 2026-10-27 | ❌ | Still `ctrl+alt+space` (`app/keybinds.rs` `default_keybind`, `macos/Keybinds.swift`). On hold since that session: macOS uses ⌘⌥Space for Finder search, so Q Calc would find it taken. Needs the user's call (take it over, or only where Finder's is off), then the default changed in both places plus the taken-hotkey note |

## Games

| Request | Date | Status | Test |
|---|---|---|---|
| Pong online anywhere with a shared code; single player | 2026-10-08 | ✅ | `games::pong*` unit tests (solo digest, online simulation) |
| Connect 4; no ghost piece after G or with the mouse off the board | 2026-10-08, 10-09 | ✅ / 🖥 | `games::connect4*` tests; the G toggle save is `qcalc-c4-ghost`, the mouse-off rule is Swift |
| Pop the lock (crack), ultimate, equal, 24 | 2026-10-09 | ✅ | `games::crack`, `ultimate*`, `equal`, `numbers` unit tests |
| 24 game: autofill parens | 2026-10-09 | 🖥 | faint parens drawn by the Swift game (PARITY §10) |
| Ember & frost: harder/longer levels, all levels open, light switch, trampoline / double jump, gravity switch | 2026-10-11 | ✅ | `games_golden::every_shipped_level_validates_and_compiles`, `every_walkthrough_matches_the_ts_step_for_step` |
| Trails multiplayer | 2026-10-11 | ✅ | `games_golden::trails_bots_make_the_same_moves`, `games::trails` tests |
| Esc pauses | 2026-10-12 | ✅ | `games::shell` tests (`esc_action`, `warns_in_an_online_game_and_leaves_on_a_second_esc_soon_after`) |
| Leave Q Calc mid online game and come back to it | 2026-10-14 | ✅ | `games::peer_online::reconnects_to_the_same_seat`, `hiding_steps_away_until_shown` |
| Games native on Mac | 2026-10-27 | ✅ | PARITY §9–10 |

## UI only

| Request | Date | Status | Where |
|---|---|---|---|
| Hover over buttons says what they do | 2026-09-30 | 🖥 | edge tools' tooltips (PARITY §5) |
| Settings icon top left | 2026-09-30 | 🖥 | `BarView.swift` gear edge tool |
| Intel and Apple silicon downloads | 2026-09-30 | 🖥 | universal build (arm64 + x86_64) |
| Old macOS (Ventura 13.4 and earlier) | 2026-09-30, 10-27 | 🖥 | floor is macOS 12 (PARITY "macOS 12"); not run on 12 |
| "copied" when the answer is clicked, not cut off | 2026-10-02, 10-16 | 🖥 | PARITY §3 "✓ copied" |
| Long calculations scroll sideways / up and down in the field | 2026-10-06, 10-09 | 🖥 | `InputField.swift` (horizontally resizable field); vertical scroll not separately verified |
| Widen the bar by its edges, drag by top/bottom, keep its place until n minutes idle (a setting), narrower minimum | 2026-10-06, 10-16 | 🖥 | PARITY §1 (🟡 ported, not driven end to end) |
| ⌘W closes the bar, or the sheet / table in front | 2026-10-08, 10-12, 10-23 | 🖥 | PARITY §6–7 |
| ↑ picks expression or answer, ← / → between them | 2026-10-16 | 🖥 | PARITY §2, §4 |
| Click one angle (or all) to copy or use it; no array brought in | 2026-10-16 | 🖥 | PARITY §3 "Solve display" |
| Tab fills ans (green) like → | 2026-10-16 | 🖥 | faint `ans` while chaining (PARITY §2) |
| Highlight then `(` wraps the selection | 2026-10-23 | 🖥 | `Symbols.wrap` (PARITY §2) |
| Tutorial: "clear" overlapping "skip" | 2026-10-23 | 🖥 | tour line wraps on a narrow bar (PARITY §5) |
| Sheets don't cover the bar | 2026-10-23 | 🖥 | shared `IdentityWindow.swift` / `PeriodicWindow.swift` placement |
| No "Write with Siri" on a new window | 2026-10-23 | 🖥 | `writingToolsBehavior = .none` (PARITY §2) |
| Periodic table: a clear button, ↵ or a click inserts and starts over | 2026-10-23 | 🖥 | PARITY §6 |
| Name field says "your name" | 2026-10-23 | 🖥 | `macos/Messages.swift` |
| Messages under the bar with a quick reply; notifications only the alert when hidden | 2026-10-23, 10-26 | 🖥 | PARITY §5 message cards |
| Two-way file transfer; ⌘V puts a clipboard chip on the reply line | 2026-10-27 | 🖥 | PARITY §5 thread view |
| Drag a file onto `message max2` to send it | 2026-10-27 | 🖥 | PARITY §5 (drops); `app::messaging::a_huge_drop_is_refused_whole`, `names_what_was_dropped` |
| Files and clips through the relay, secure and private, and users told so; spam-proof size limit | 2026-10-27 | 🖥 | relay limits (20 uploads/hour, 5 GiB/day per sender) and end-to-end sealing; `app::messaging::sizes_and_encrypted_lengths` |
| Faster open, less memory, old laptops | 2026-10-27 | 🖥 | `native/PERFORMANCE.md` |
| Caret the right height when the bar opens | 2026-10-27 | 🖥 | commit 962613b (field is the clip's height) |
| Tab on `(838*2123` leaves the caret past the `)`; grey parens as wide as typed ones, nothing shifts | 2026-10-27 | 🖥 | commits 9a603d2, d051328 |
| Tab autofills during tutorial steps | 2026-10-27 | 🖥 | same Tab handling as the line; not driven |
| Bar hides on click-away unless messaging | 2026-10-28 | 🖥 | `holdsOpen` (PARITY §5) |

## Counts

- ✅ tested: 51 (43 engine/app, 8 games): 34 new tests in `core/tests/requested_features.rs`, the rest existing tests
- 🔧 fixed: 0 (nothing small and clearly specified was broken)
- ❌ missing: 1 (default ⌘⌥Space, waiting on a decision)
- 🖥 UI-only: 27 (26 bar/app, 1 game)
