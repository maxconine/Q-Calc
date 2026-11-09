# Runbook: bring Windows up to the Mac

Steps for the Windows agent. `MAC_CHANGES_TO_PORT.md` is the list of changes, and this file says how to work through it. `AGENTS.md` has the house rules, and they still apply.

## Prompt to paste into the Windows agent

> Read `native/windows/AGENTS.md`, then follow `native/windows/PORT_RUNBOOK.md` step by step. Work through `native/windows/MAC_CHANGES_TO_PORT.md` in its order, and `native/windows/PORT_linear_algebra.md` with it. Don't skip the checks. Report back in the runbook's report format.

## Step 0: get the Mac work

1. `git fetch`, then check that `native` (or the branch you were told) has the Mac commits. These files must exist: `native/core/src/feet_inches.rs`, `native/core/src/phrases/native_money.rs`, `native/core/src/calculus/ode.rs`, `native/core/src/app/native_sheets.json`.
2. If any is missing, stop and report "the Mac work isn't pushed yet". Don't port from memory or from the Mac docs alone.
3. Merge that branch into `windows-native` only if the coordinator told you to. Otherwise make your worktree from the branch you were given (`AGENTS.md` → Setup).

## Step 1: baseline checks (about 10 minutes)

Run these one at a time, never two cargo builds at once:

```
cargo test -p qcalc-core
cargo test -p qcalc-ffi --features typst --test native_sheets_typst
cargo test -p qcalc-win
cargo clippy -p qcalc-win --all-targets
```

- **Golden floors:** the core's golden floors were set on the Mac. If Windows passes fewer in an area, write down the area and both numbers. Don't lower a floor.
- **Windows' own tests:** `qcalc-win` tests may fail where the Mac changed behaviour. `MAC_CHANGES_TO_PORT.md` names the known one, in `commands.rs`. Fix tests only where the new behaviour is the intended one, and list each test you changed.

## Step 2: section 1, "Comes from the core: just test it"

1. Build and run the app with `QCALC_DATA`, `QCALC_LOG` and `QCALC_INSTANCE` set, as in `AGENTS.md`.
2. Type every line in the table and compare the answer with the Expect column. Screenshot a few of them in light and dark.
3. For each line, mark it ✅ or ❌, and for ❌ note what showed.
4. A ❌ here is usually Windows routing (which engine call the line goes through), not maths. Compare with how the Mac calls the engine (`mac/Sources/Bar/BarModel.swift`) before you change anything.
5. Delete the ✅ lines from `MAC_CHANGES_TO_PORT.md`.

## Step 3: section 2, "Edited on Windows but never run: test it"

Same as step 2: try each item, fix what's wrong, and delete it once it works.

## Step 4: section 3, "Not on Windows yet: implement it"

Do one subsection per branch, in this order, with the simplest and most used first:

1. Bar: units on answers, the angle indicator, tip delay, root sign, sheet autofill.
2. Feet and inches, ⌥↑ / ⌥↓ (Alt+↑ / Alt+↓ on Windows), and the currency rate on hover.
3. Systems panel.
4. Formula sheets, including sheet search, then the unit circle.
5. Stacked fractions and powers (off by default).
6. The working panel, Ctrl+W.
7. Graph panel: implicit curves and key points first, then several equations and graph settings, then the 3D view.

If several agents run in parallel, give each one a subsection from this list. The subsections touch different files.

For each subsection:

1. Read the Mac files it names and the lines for it in `native/mac/PARITY.md`.
2. Put the logic in your own module, with one or two lines in a hub (`AGENTS.md` → Ownership).
3. Unit-test the pure logic, then run it for real, in light and dark.
4. Run the keystroke bench in release and compare it with the targets in `AGENTS.md`.
5. Delete the subsection from `MAC_CHANGES_TO_PORT.md` in the same commit.

## Step 5: report format

Keep the report under 400 words:

- **Branch and commits:** one line each.
- **Section 1 and 2 results:** the ✅ count, and each ❌ with what showed.
- **Implemented:** each subsection, how you checked it, and any gaps.
- **Hub edits:** file, function and one line each.
- **Measurements:** keystroke median, idle memory and warm launch, before and after.
- **Needs the Mac side:** core or FFI changes you couldn't make here, and golden floors that differ.
