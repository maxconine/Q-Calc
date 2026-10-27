# qcalc-win performance

The native Windows app against the three targets (AGENTS.md, tightened by 50%): keystroke to top answer painted
under 0.8 ms median, idle memory under 12.5 MB (private working set and private bytes), warm launch to ready under
75 ms. Modelled on native/PERFORMANCE.md (the Mac's).

## Headline

`windows-native` at 8e73660 (before) against `win-perf` (after). Release builds, 4K at 150%, on a 24-core machine
shared with ~7 agents building at the same time. Three interleaved runs each (before, after, before, after, ...) of
`scripts/perf.ps1`; the figure is the median of the three runs.

| | before | after |
|---|---|---|
| keystroke to answer painted, standard inputs: median / p90 / p99 | 1.09 / 1.45 / 2.31 ms | **0.51 / 1.18 / 1.55 ms** |
| same, the 269 golden inputs (`native/bench/inputs.json`, 4,121 keys) | 1.32 / 1.55 / 2.66 ms | **0.53 / 1.20 / 1.70 ms** |
| idle, never shown since launch (autostart at sign-in): private working set / private bytes | 11.6 / 14.6 MB | **4.2 / 6.3 MB** |
| idle, hidden 10 s after a session (standard typing, then ten lines typed, paused on, committed) | 21.9 / 25.1 MB | **13.7 / 15.5 MB** |
| same session after the 269 golden inputs too | 28.8 / 31.9 MB | 19.2 / 20.9 MB |
| warm launch to the first frame (median of 3, per run) | 45.5 ms | 46.0 ms |
| the e2e suite's own memory check (smoke.ps1 Z1, hidden after its session) | 21.3 MB | 11.6 MB |

The Mac's native app measured 1.3-1.6 ms on the golden inputs (its p90 3.0-3.8, p99 5.2-6.8).

Met: keystroke median (37% under), never-shown memory (both counters, half the target), launch. Not met: memory
after use, at 13.7 / 15.5 MB for a standard session. What is left there is mostly the engine's compiled regexes (see
the end).

## Against the Tauri app

The installed Tauri app (2.0.15, WebView2) and the native build (93a2db2), on the same desktop with one external
harness, since the Tauri app has no bench pipe. The harness launches the exe and polls until a visible window of
the process has painted. It types `1` to `9` four times with SendInput and times each key until the pixels of the
answer side of the line change. It then measures the private working set of the whole process tree 10 s after Esc.
The screen is read through DWM, so a keystroke can never measure under one 60 Hz frame (16.7 ms). What counts is
whether the answer lands on the next frame or a later one. The Tauri app was launched once (it is the user's own
install); the native build five times.

| | Tauri (WebView2) | native |
|---|---|---|
| launch to a painted window (includes PowerShell's process start) | 794 ms | **79 ms** (median of 5; 72-82 warm) |
| keystroke to answer on screen: median / p90 | 16.9 / 33.2 ms (1 / 2 frames) | **16.6 / 16.9 ms** (every key the next frame) |
| keys whose answer never visibly changed within 250 ms | 4 of 36 | **0 of 36** |
| idle, hidden 10 s after use: private working set | 121.5 MB in 7 processes | **12.0 MB** in 1 process |

By its own clock the native app needs 0.6 ms from keystroke to painted answer (smoke.ps1 Z2), so the frame it
lands on is set by DWM, not by the app. About a third of the Tauri app's keystrokes miss the next frame.

## Method

- **Keystrokes**: the bench (`bench.rs`), in-process: from posting WM_CHAR to the EndDraw (now the Present) of the
  frame that shows that text's answer. Before this work, a key that typed over an auto-closed bracket left the text
  as it was and timed out after 10 s (15 timeouts in the golden run); the bench now counts such a key as done at
  its first paint. The figures are over three rounds of the standard inputs (276 keys) after a fresh launch, so the
  first-time paths are in the p99.
- **Memory**: `GetProcessMemoryInfo`: `PrivateWorkingSetSize` (Task Manager's "Memory") and `PrivateUsage` (private
  bytes, commit). Measured 10 s after `--autostart` (never shown), and 1 s and 10 s after hiding, well before the
  30 s trim (`EmptyWorkingSet`, not counted). The session: the standard inputs three times a key at a time, then
  each standard input typed, a 0.9 s pause (the typeset row renders, the squiggle comes), and Enter.
- **Launch**: the log's `ready ... since process start` line, ms since process creation.
- **Data**: a returning user's folder (past the tour, twenty rows of history), copied fresh for every run, with
  QCALC_INSTANCE / QCALC_DATA / QCALC_LOG of the run's own.
- **Profiling**: there's no ETW without elevation here, so: `--features perf` (perf.rs) marks each keystroke's path
  (WM_CHAR, the engine thread's steps, WM_ENGINE, layout, paint) and the bench reports each stage's median and each
  key's timeline; a counting global allocator splits the Rust heap by allocating thread and tracks big blocks; the
  pipe's `vm` walks the address space (VirtualQuery, QueryWorkingSetEx, HeapSummary) and `threads` names every
  thread or its start module. `examples/live_bench.rs` times the engine's calls alone, `examples/present_bench.rs`
  a frame's presentation alone, and `examples/mem_attrib.rs` (built with line tables) groups the engine's live
  heap by the static or function that made it.

## Where a keystroke's time went

Before (median of the stages, µs from the post): WM_CHAR at 9; the engine thread picked the job up at 21; `chains`
(a hop to qcalc-eval and back) at 41; `live` at 360; the result at the UI thread at 425; paint start 455, EndDraw
690; the bench saw it at 850. In order of size:

1. **`live` worked out the squiggle on every blank key** (~110 µs median, more than the answer): `blank_reason`
   re-reads every piece of the line. Neither bar reads `LiveAnswer.blank`; both ask `blank_span` after a 700 ms
   pause. The core's `LiveRequest` gains `no_squiggle` (off by default, so golden recordings are unchanged) and the
   FFI's `live` sets it. 0.83 -> 0.66 ms; golden 1.05 -> 0.66 ms.
2. **Every frame placed the IME's windows** (~90 µs after EndDraw): ImmGetContext / ImmSetCompositionWindow /
   ImmSetCandidateWindow on each paint. Now only when the caret's spot moves, a composition starts, or the bar is
   activated. 0.83 -> 0.76 ms.
3. **Two hops per keystroke to the FFI's evaluation thread** (~15 µs each, plus other calls): the app's engine
   thread already has the 32 MB stack, so its calls now run on it (`run_engine_calls_here`). 0.53 -> 0.50 ms,
   p99 2.11 -> 2.00.
4. **Presentation**: a flip-model swap chain on a WARP device instead of the window render target: a frame alone
   took 113 µs instead of 226-337 in present_bench, but in the app the answer's frame stayed ~140 µs; it kept p90
   1.25 -> 1.18 and 0.8 MB, so it stayed.

After: WM_CHAR at 8, job picked up at 31, `chains` 50, `live` ~300, result applied ~360, paint ~140 µs, seen at
~500. The engine (`evaluate_sheet` ~150 µs of `live`'s ~250) and one frame are what's left. The p90 (~1.15 ms) is
the engine's heavy prefixes (`sin(9`, `1/3 + 1/`, `3x+2=1`: 400-700 µs in the engine alone, live_bench).

## Where the idle memory went

Never shown, before (14.6 MB private bytes): the engine's warm-up ~4.8 MB (its lazy tables and ~150 regexes), Typst
~2.5 MB (renderer, library and fonts, warm-up compile, the thread's stack), the warm paint's WARP device and target
~1.9 MB, the process heap's leftovers.

After a session, before (25.1 MB): the engine's heap on qcalc-eval ~6.3 MB (5.8 MB of it compiled regexes: NFAs,
reverse NFAs, fancy-regex programs, dense DFAs, lazy-DFA caches), Typst ~4.5 MB (and 24 rayon threads it started,
one per core), 29 thread-pool workers WARP's rasteriser grew to, the WARP device and swap chain ~2.5 MB, the
layouts, heap fragmentation, and ~3 MB of DLL image commit.

What changed:

1. **The segment heap** (manifest `heapType`): never shown 14.6 -> 13.3 MB, after use 20.6 -> 19.5.
2. **Typst on demand**: the thread starts with the first typeset and ends when the bar hides, evicting Typst's
   process-wide memos as it goes; its library and fonts (native/typst `world.rs`) now live while a renderer does
   (the Mac keeps one renderer, so nothing changes there); its rayon pool has one thread. With the layouts dropped
   at the hide (`trim_hidden` without the working-set trim): never shown 13.5 -> 11.0 MB, after use 21.9 -> 18.1.
3. **WARP without internal threading** (`D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS`): 29 pool
   threads gone, -1.2 MB after use, for ~25 µs more per frame.
4. **The engine warms with the first showing** when the app starts into the tray, and the warm-up now covers a
   keystroke's own path (live, chains, decorations on `2+`, `2+2`, `5 km in miles`): never shown 11.0 -> 6.3 MB, and
   the first two keys after the first showing 8.2 / 6.9 -> 1.6 / 0.7 ms. The first showing stays ~17-23 ms.
5. **The swap chain and the WARP device go at the hide** (D3D10Warp unloads): after use 18.2 -> 15.7 MB; a later
   showing 6.5 -> 10.5 ms.

## Iteration log

Each change A/B'd interleaved against the build before it, 2-3 runs each. Kept:

| change | keystroke median / p90 | never shown (pws / priv) | hidden after use (pws / priv) |
|---|---|---|---|
| IME placed only when it moves | 0.83 -> 0.76 / 1.67 -> 1.55 | | |
| no eager squiggle (core `no_squiggle`) | 0.83 -> 0.66; golden 1.05 -> 0.66 | | |
| segment heap | = | 11.6 / 14.6 -> 10.5 / 13.3 | 17.4 / 20.6 -> 16.6 / 19.5 |
| flip swap chain for the bar | 0.521 -> 0.514 / 1.25 -> 1.18 | = | 16.7 / 19.6 -> 15.9 / 18.8 |
| Typst on demand, layouts at the hide, one rayon thread, WARP single-threaded | 0.52 -> 0.54 | 10.7 / 13.5 -> 8.9 / 11.0 | 18.4 / 21.9 -> 15.7 / 18.1 |
| engine calls on the engine thread | 0.53 -> 0.50 / 1.18 -> 1.14 | = | -0.1 |
| engine warm-up with the first showing, covering the keystroke path | first keys 8.2 -> 1.6 ms | 8.9 / 11.0 -> 4.2 / 6.3 | = |
| swap chain and WARP device dropped at the hide | = | = | 15.7 / 18.2 -> 14.0 / 15.7 |

Tried and dropped:
- **A DC render target bound to the dirty part of the window** (present_bench): 0.5-1.3 ms a frame, worse than the
  window target's 0.23-0.34.
- **Dirty rects with Present1**: no faster than a full present for a window this size (118 against 113 µs).
- **Spawning the closed-form search after the frame** (its first poll on a posted WM_ENGINE): no change in median
  or p90; the ~150 µs between WM_ENGINE and the model on heavy keys isn't the spawn.
- **mimalloc** as the global allocator: private bytes after use 15.5 -> 51.8 MB (it commits its arenas).
- **No full DFAs in the engine's regexes** (`delegate_dfa_size_limit(0)`): regex memory 5.74 -> 5.37 MB only, the
  lazy DFAs' caches grow instead, and matching is slower. Not kept.
- **HeapCompact / HeapOptimizeResources at the hide**: -0.2 MB.
- **Dropping only the swap chain at the hide** (device kept): -0.2 MB; with the device, -2.5 MB (kept).
- **Not painting the hidden bar at launch** (no device until the first showing): never shown 6.3 -> 4.3 MB, but the
  first showing went 17 -> 37 ms; the warm paint stays.

## What's left

- **Memory after use, 13.7 / 15.5 MB** (target 12.5). The engine's thread holds ~6.8 MB of Rust heap (5.4-5.8 MB of
  it the core's ~300 compiled `fancy_regex` statics, ~19 KB each: forward and reverse NFAs, fancy-regex programs,
  dense and lazy DFAs and their caches; the units table ~0.85 MB). They're `LazyLock` statics, so they never go,
  and a wider session compiles more (19.2 / 20.9 MB after the golden inputs). Then ~2.4 MB of heap fragmentation,
  ~3 MB of DLL image commit, ~0.8 MB of thread stacks. Getting under 12.5 needs fewer or smaller regexes in the
  core (shared prefilters, `RegexSet`s, literal fast paths), which also changes the Mac.
- **Keystroke p90 ~1.15 ms**: the engine's heavy prefixes (400-700 µs in `evaluate_sheet` alone). The core's
  evaluator is next.
- **One frame is ~140 µs** on WARP, single-threaded now (+25 µs for 1.2 MB of threads).
- The e2e suite's G2 (dark appearance applies live) fails on this machine for the old build too: another agent's
  pong window sits where the settings window opens.
- typst's `web_parity` test fails in a Windows checkout with CRLF line endings (macros.typ), before and after.
