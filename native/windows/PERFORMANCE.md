# qcalc-win performance

The native Windows app against the three targets (AGENTS.md, tightened by 50%): keystroke to top answer painted
under 0.8 ms median, idle memory under 12.5 MB (private working set and private bytes), warm launch to ready under
75 ms. Modelled on native/PERFORMANCE.md (the Mac's).

## First key after reopening (win-polish)

The bar used to free the core's compiled regexes, the WARP device and swap chain, and the text layouts at every
hide, so a key typed within ~50 ms of the next showing waited for the warm-up's first step (~35 ms; smoke.ps1 Z2's
p99, since its bench types the instant the bar shows). Now they go only once the bar has stayed hidden 30 s
(app.rs `release_hidden`, from the trim timer a showing cancels); the working set is trimmed 2 s after that, once the
engine thread has freed them. The pipe's `release` does it at once, so smoke.ps1 Z1 still reads the released state
0.8 s after a hide without waiting 30 s.

| | before (a27cf55) | after |
|---|---|---|
| smoke Z2 p99, keystroke to answer painted | 42.2 ms (bar shown and typed into at once) | **1.64 ms** after a quick hide; 37.0 ms after a release (as after 30 s hidden) |
| smoke Z2 median | 0.69 ms | 0.67 ms |
| a showing after a quick hide (perf.ps1 `showMed`) | 9.36-9.39 ms | **6.31-6.32 ms** (the device and swap chain kept) |
| hidden 35-45 s after a standard session (launch settled first): private working set / private bytes | 0.61 / 7.70-7.75 MB | 0.25 / 7.79-7.84 MB |
| hidden 1-10 s after use (before the release) | 6.1-6.3 / 7.8-8.5 MB | 20.5-20.7 / 22.9-23.1 MB (what a quick reopen keeps) |
| smoke Z1 (released, 0.8 s after) / Z3 | 5.06 MB / 44.1 ms | 5.86 MB / 31.7 ms |

A currency-rates fetch landing inside the 30 s (a fresh data folder's first, then once a day) leaves the released
state ~0.5 MB higher in private bytes: blocks the fetch and reload make among what is freed later keep their pages.
HeapCompact on the process heap didn't give them back. What is left of a showing's ~6.3 ms is SetWindowPos and the
two paints (below).

## Second pass (win-perf2)

`windows-native` at e77c180 (before) against `win-perf2` (after), with stretch goals of hidden after use under 8 MB
private working set and 10 MB private bytes, keystroke p90 under 0.8 ms and p99 under 1.5 ms, a showing after a
hide under 5 ms, and warm launch under 30 ms. Release builds, `scripts/perf.ps1` interleaved, the median of the
runs (3 with the golden inputs, 2 without), on the same shared 24-core machine.

| | before | after |
|---|---|---|
| keystroke to answer painted, standard inputs: median / p90 / p99 | 0.51 / 1.16 / 1.54-1.61 ms | **0.47 / 0.62 / 1.28-1.38 ms** |
| same, the 269 golden inputs | 0.54 / 1.23 / 1.69 ms | **0.47 / 0.56 / 1.73 ms** |
| the engine alone (`live_bench`, `Engine::live`): median / p90 / p99 | 195 / 462 / 695 µs | **113 / 204 / 448 µs** |
| idle, never shown: private working set / private bytes | 4.1 / 6.3 MB | 4.2 / 6.4 MB |
| idle, hidden 10 s after a standard session | 13.8 / 15.5 MB | **6.4 / 8.1 MB** |
| same session after the 269 golden inputs too | 19.2 / 21.0 MB | **7.5 / 9.2 MB** |
| the e2e suite's memory check (smoke.ps1 Z1, 0.8 s after a hide) | 11.6 MB | **4.7 MB** |
| a showing after a hide (Op::Show to the uncloak; perf.ps1's new `showMed`) | 10.3-10.5 ms | 9.0-9.4 ms |
| warm launch to the first frame | 43-46 ms | **34 ms** (smoke.ps1 Z3, to ready in the tray: 31 ms) |

Met: memory after use (both counters, both sessions), keystroke p90 and the standard inputs' p99. Not met: the
golden inputs' p99 (1.73 ms: the first-time paths of a few heavy inputs), a showing under 5 ms, launch under 30 ms.

### What changed

1. **The compiled regexes go at a hide** (19.2 -> 10.9 MB after the golden session). A new crate, `native/lazy`
   (`qcalc-lazy`), replaces the core's `LazyLock<Regex>` statics (`jsre::Lazy`, ~480 statics, and the regex tables):
   every thread reads a value built once and kept, as with `LazyLock`, except up to four threads that call
   `jsre::own_compiled_here` (the engine, UI and Typst threads). Each of those builds copies of its own, listed on
   that thread, which it alone frees with `release_compiled` when it holds none: the engine thread as a job of its
   own when the bar hides, the UI thread in `after_hide`, the Typst thread as it ends. A copy remembers the
   owner's epoch it was built in, so `release` never touches a static, and a static may move or drop. No other
   thread ever sees an owner's copies, so freeing them can't pull a regex out from under one. The core keeps
   `unsafe_code = "forbid"`: the one `unsafe` primitive is in the new crate. The Mac and the tests never claim, so
   they get `LazyLock`'s behaviour. The next showing's warm-up compiles the keystroke path's again (~35-50 ms on the
   engine thread, in steps that each run only when no keystroke waits).
2. **The freed regexes' blocks come from slabs of their own** (heap.rs; 10.9 -> 9.4 MB): while an owner builds a
   copy (`jsre::building_compiled`), blocks up to 32 KB come from a 64 MB reservation cut into 64 KB slabs of one
   size class each, committed a page at a time; a regex's search caches are made at compile time and grow there.
   Freed, the slabs empty out whole and are decommitted, instead of leaving holes in the process heap's pages.
3. **The UI and Typst threads' regexes go too** (8.5 -> 6.4 MB standard session): 48 and 28 regexes the bar's own
   rules and the typeset source compile on those threads, kept for good before.
4. **fancy-regex's seek** (`RegexBuilder::seek`) on the owners' regexes (and everywhere for the Mac and tests): a
   pattern that needs the backtracking VM (each JS `\b` becomes look-arounds, so most do) first finds where a match
   can start with a regular over-approximation, instead of running the VM at every position. A sampling profiler
   (below) put 46% of `evaluate_sheet` in `preprocess_ascii`'s rewrite chains and a third of all samples in
   fancy-regex's VM. With seek the engine is ~2.5x quicker (keystroke p90 1.17 -> 0.57 ms). Checked: every one of
   the 1,078 patterns the core's tests compile gave the same captures with and without seek over ~39,500 haystacks
   (the golden inputs and their prefixes, variants, 40,000 random token strings); the golden fixtures pass. Seek's
   copies are ~60% bigger and ~80% slower to compile, which only the owners' (freed) copies pay.
5. **The bar's WARP device is made on a thread of its own** (warp.rs) at launch and as a hidden bar starts to
   show, while the UI thread makes its windows and lays out; the first paint takes it. Launch 46 -> 34 ms (the
   first device in a process is ~11 ms), a showing ~1 ms. WARP's DLL also stays loaded across hides (pinned), which
   was another ~1 ms of a showing for no measurable memory.

Where a showing's ~9 ms goes now (instrumented, not kept): `prepare_show` ~0.9 ms, SetWindowPos ~8 ms of which the
paint inside it is ~4-5 (device ~0-3 ms if the prefetch hasn't finished, swap chain 0.6, D2D device context, draw)
and the rest the window manager's, the second paint 0.6 ms. Under 5 ms needs work in the paint path (one paint, the
D2D context kept) and a look at what SetWindowPos waits for.

### Iteration log (second pass)

Interleaved A/B against the build before, 1-3 runs each.

| change | keystroke median / p90 / p99 | hidden after use (pws / priv) | other |
|---|---|---|---|
| owned regex copies freed at a hide | = | golden session 19.3 / 21.1 -> 10.9 / 12.6 | |
| their blocks in a private heap (HeapCreate, fixed size) | = | 11.1 / 12.8 -> 9.35 / 11.1 | re-warm 36 -> 48 ms (no LFH) |
| slabs of our own instead | = / p99 2.5 -> 2.1 | = | re-warm 48 -> 31 ms; release 0.8 ms |
| ... committed a page at a time | = | = | never shown back to 6.4 MB private (whole slabs: 7.5) |
| warm-up in steps behind keystrokes, more of a keystroke's paths | p99 2.1 -> 1.56 | = | |
| seek on every regex | 0.50 / 1.17 / 1.55 -> 0.47 / 0.57 / 1.31 | 9.7 / 11.4 -> 10.5 / 12.3 | |
| ... on the owners' only | = | 10.4 / 12.1 -> 9.8 / 11.5 | |
| WARP DLL pinned | = | +0.05 | show 10.2 -> 9.3 ms |
| UI and Typst threads own and free theirs | = | standard session 8.5 / 10.2 -> 6.4 / 8.1 | show +0.4 ms |
| device made ahead on a thread | = | = | launch 46 -> 35 ms, show 9.7 -> 8.6 ms |
| regexes freed at once at a hide, not 5 s after | first key after a showing see below | Z1 15.4 -> 4.7 | |

Tried and dropped:
- **A 5 s delay before the engine frees its regexes**, so a quick reopen keeps them: smoke.ps1's Z1 reads 0.8 s
  after a hide, and with seek's bigger copies it read 15.4 MB. They go at once now. The cost: a key typed within
  ~50 ms of a showing after a hide (before the warm-up's steps ran) compiles what it needs, up to ~30 ms (Z2's p99
  shows it, since the bench types the instant the bar shows); perf.ps1 types 0.5 s after a showing and its p99 is
  unaffected. A person's first key comes 150+ ms after the hotkey.
- **Every engine-thread block in the private heap**, not just the regexes': 0.35 / 0.7 MB less, but the keystroke
  median 0.54 -> 0.73 ms (the fixed-size heap has no low-fragmentation front end).
- **HeapCreate'd private heap** for the regexes: worked, but compiling in it was ~30% slower; the slabs replaced it.
- **Freeing the units table at a hide** (0.85 MB): units are compared by `Arc` identity, and values the engine
  keeps across a hide hold the old table's units, so answers could change. Kept.
- **`(?-u:\b)` for JS `\b`** (a native ASCII word boundary, no VM): fancy-regex rejects changing Unicode mode, and
  under the `i` flag the current translation also counts ſ and K (Kelvin) as word characters, so it wouldn't be
  byte-identical. Seek got the speed instead.
- **Pre-made device kept at a hide** (show 6.5 ms): ~2.3 MB, which is what the hide gives back.

### Tools added

- `examples/live_bench.rs` with `LIVE_BENCH_EACH=1` lists the 30 slowest prefixes (engine and `evaluate_sheet`).
- `examples/mem_attrib.rs` runs on an owner thread, takes `GOLDEN=<inputs.json>` and `RELEASE=1` (what stays after
  the hide's release).
- `scripts/perf.ps1` times a showing after a hide (`-Shows`, `showMed`); the pipe's `shows` now records in every
  build (the last 32).
- The sampling profiler (not in the repo): a thread that suspends the evaluating thread every 200 µs, walks its
  stack with `RtlLookupFunctionEntry` / `RtlVirtualUnwind` into a preallocated buffer (allocating while the other
  thread is suspended can deadlock on the heap lock) and symbolises with dbghelp; inclusive and self percentages.

## Headline (first pass)

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

After the second pass (the first pass's notes on memory and the keystroke p90 are done above):

- **A showing after a hide, ~9 ms** (stretch 5): the paint inside SetWindowPos (swap chain, D2D device context, a
  full draw) and the window manager's share of SetWindowPos; then a second paint from UpdateWindow.
- **Warm launch, 34 ms** (stretch 30): ~10 ms pass before `main` (loading a 47 MB image and its imports), then
  the factories, the windows and the first frame.
- **The golden inputs' p99, 1.7 ms**: a few inputs' first-time paths (regexes or tables not in the warm-up).
- **What stays while hidden, ~6.4 MB**: the units table (~0.85 MB, kept: see above), Typst's process-wide leftovers
  (~0.85 MB after its thread ends), the process heap's other blocks and holes, stacks, DLL data.
- **One frame is ~140 µs** on WARP, single-threaded now (+25 µs for 1.2 MB of threads).
- typst's `web_parity` test fails in a Windows checkout with CRLF line endings (macros.typ), before and after.

## What was left after the first pass


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
