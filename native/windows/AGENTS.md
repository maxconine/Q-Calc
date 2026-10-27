# Working on qcalc-win in parallel

Several agents port the Mac native app's features to `qcalc-win` at the same time, each on its own branch.
These rules keep the branches mergeable.

## Setup
- Make your own worktree from the integration branch. Run this from the repo root
  `C:\Users\maxco\Documents\GitHub\QCALC\Q-Calc`:
  `git worktree add .claude/worktrees/win-<pkg> -b win-<pkg> windows-native`
  Work only there. Never edit `.claude/worktrees/windows-native` or the main checkout.
- Build in your worktree's own `native/target`. Iterate with debug builds (`cargo build -p qcalc-win`) and use
  release only to measure. The machine has 24 cores and 32 GB shared by ~7 agents, so don't run more than one
  cargo build at a time.
- Commit often on your branch, with messages in the repo's style (lowercase prose saying what the user sees). End
  each message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never push, never merge other
  branches, and never rebase `windows-native`. The coordinator merges.

## Ownership
- `native/windows/README.md` is the contract. Read it first: the module map, threading, the paint model, and
  "Adding a panel".
- Own your files. You may add new files in `native/windows/src/`.
- For the shared hubs (`app.rs`, `model.rs`, `window.rs`, `keys.rs`, `render.rs`, `main.rs`, `Cargo.toml`), keep
  edits small and additive, at the hooks the README names: a field, a match arm, a call. Don't reformat, reorder
  or refactor existing code there. Put your logic in your own module and call it from the hub in one or two lines.
- Don't change `native/core` or `native/ffi`. If you need an engine function that isn't exported, use
  `Engine::call_json` (see `ffi/src/engine.rs` and `core/src/app/js.rs` for what it dispatches). If that's truly
  impossible, note what's needed in your report.
- The feature spec is the Mac native app in `native/mac/Sources`. Files in `native/mac/Sources/Shared/` are git
  symlinks stored as text: open the `macos/...` path they contain. `native/mac/PARITY.md` lists the expected
  behaviours. Windows conventions come from the Tauri app (`src-tauri/src`) and the web app (`src/`): on Windows
  read ⌘ as Ctrl and ⌥ as Alt, use Ctrl+Shift+Backspace to clear history, and Ctrl+, opens settings.

## Quality bar
- Targets (tightened by 50%): keystroke → top answer painted < 0.8 ms median, idle memory < 12.5 MB private
  working set with the bar hidden after use, warm launch < 75 ms to ready. The foundation measured 1.03 ms, 14.7 MB
  and 43 ms, so keystroke and memory already need headroom: whatever you add must cost ~nothing on those paths. Never evaluate on
  the UI thread, no timers while idle, cache text layouts, and don't add startup work on the UI thread. Lazily
  create anything that only your feature needs.
- `cargo clippy -p qcalc-win --all-targets` clean, and `cargo test -p qcalc-win` passing, with unit tests for your
  pure logic.
- Run the app and check your feature for real. Use `QCALC_DATA=<temp dir>` and `QCALC_LOG=<temp file>` so you
  don't touch the user's data, and `QCALC_INSTANCE=<your pkg>` so your copy takes its own single-instance mutex
  and bench pipe (`\.\pipe\qcalc-bench-<pkg>`; bench.ps1 follows the variable). Without it, your launch hands off
  to another agent's running copy. Keep scripts in a subfolder of the scratchpad named after your package. Drive it with `--bench-pipe` / `scripts/bench.ps1`, SendInput, or `--bench
  spec.json`. Screenshot with PowerShell (System.Drawing CopyFromScreen) and look at the result in light and dark
  mode. Kill only the qcalc.exe processes you started (by PID). Never kill or touch the installed Q Calc at
  `%LOCALAPPDATA%\Q Calc`.
- Before you finish, run the keystroke bench (release build) and confirm you didn't regress it.
- Your final report (under 400 words): what works and how you checked it, the hub edits you made (file, function,
  one line each, so the coordinator can resolve merge conflicts), measurements, and gaps.
