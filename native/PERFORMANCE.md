# Performance baseline: web-view app vs native app

One baseline, 2026-10-26, on an Apple M4 Pro (14 cores, 48 GB), macOS 26.5.1. Both apps were built from the same
commit, arm64 only: the web-view app is `zsh macos/build.sh` (WKWebView with the TS engine) and the native one is
`zsh native/mac/build.sh --arch=arm64` (SwiftUI and AppKit over the Rust core). Each app ran by itself. The machine
was not otherwise quiet, though: the load average was about 5 from other work, and the user was using the
desktop. Treat a gap of less than 20% as noise.

To rerun it (the scripts are in `native/bench/`):

```sh
zsh native/bench/engine.sh            # engines alone: Rust, TS on JavaScriptCore (jsc), TS on node
python3 native/bench/app_bench.py     # both apps: 3 cold and 5 warm launches, then 3 full sessions each
```

Each run writes its raw JSON to `native/bench/results/`, which git ignores. `summarize.py` prints the tables
below from that JSON. Spreads are given as median [min–max] across runs.

## Headline

| | web-view app (WKWebView + TS) | native app (SwiftUI + Rust) |
|---|---|---|
| launch to ready, cold | 835 ms [772–993] | 606 ms [543–684] |
| launch to ready, warm | 536 ms [496–767] | 154 ms [111–513] |
| memory footprint, idle (bar never shown) | 91 MB [86–91] | 26 MB [24–26] |
| memory footprint, bar shown | 202 MB [199–202] | 28 MB [25–28] |
| memory footprint, after a typing session | 330 MB [328–344] | 47 MB [47–48] |
| memory footprint, after a graph | 396 MB [395–404] | 66 MB [65–66] |
| RSS, after a graph | 612 MB [606–616] | 138 MB [138–139] |
| processes | 4 (app, WebContent, GPU, Networking) | 1 |
| idle CPU, bar hidden | 0.00% | 0.00% [0.00–0.03] |
| CPU while typing (one key every 40 ms) | 32% of a core [32.2–32.6] | 19% of a core [18.5–19.3] |
| keystroke to live answer: median / p90 / p99 | 12 / 27 / 39 ms | 4.6 / 9.0 / 16.5 ms |
| app on disk | 34.9 MB | 40.8 MB |

What the memory rows add up (after the graph, footprint/RSS): for the web-view app, QCalc 23/84 MB, WebContent
236/417 MB, GPU 137/50 MB and Networking 8/56 MB. For the native app, QCalc alone, 66/139 MB.

## What each row measures

- **Launch to ready** is the time from spawning the executable to the app being able to answer.
  - Web-view app: the page's input is mounted and the page's own engine warm-up (`evaluateSheet` in a
    `setTimeout(0)`) has run, as reported by the page.
  - Native app: the later of the end of `applicationDidFinishLaunching` and the log line "engine warm …".
  - **Cold** is the first launch of a fresh copy of the bundle at a new path. Without sudo the file cache can't be
    purged, so this is cold for LaunchServices, the code signature and dyld, but the files are still in the page
    cache.
  - **Warm** is a relaunch of the same copy.
  - The native app's bar is ready 116 ms after spawn (warm); its engine warm-up finishes at 154 ms.
- **Memory** is `phys_footprint` from `footprint -p` (what Activity Monitor calls Memory) plus RSS, summed over
  the app and every `com.apple.WebKit.*` process that started after the app did.
  - RSS double-counts shared pages, and the GPU process's footprint includes IOSurfaces, which is why it is larger
    than its RSS.
  - The samples are taken in this order: 10 s after ready; 3 s after showing the bar; after the typing session;
    after `graph sin(1/x) + sin(40x)`.
- **Idle CPU** is CPU time (`ps` `time`, which has 10 ms resolution) over 30 s with the bar hidden. Energy was not
  measured: there was no sudo, so no `powermetrics`.
- **CPU while typing**: 20 golden inputs (every 13th in `inputs.json`) typed at one key per 40 ms, each followed by
  ↵. This is CPU seconds over wall seconds for all the app's processes.
- **Keystroke latency**: 269 golden inputs (`inputs.json`, 4,058 keystrokes per run, 3 runs pooled), typed a
  prefix at a time.
  - Each keystroke is timed from the field's text changing to the live answer being committed to the UI state.
    The web-view app measures until React has rendered, with one more event-loop turn; the native app until
    `BarModel.setLive` has run for that text, with one more main-queue turn. Painting to the screen is excluded
    in both.
  - WebKit rounds `performance.now()` to 1 ms, so the web-view figures are whole milliseconds.

## Heavy cases

These were measured in the apps, set as one whole line, 5 times each per run, over 3 runs.

| line | web-view app | native app |
|---|---|---|
| `integral of x^2 sin(x) from 0 to pi` (numeric, in degrees) | 29 ms [9–41] | 8.2 ms [5.9–12.7] |
| `integral of e^(-x^2) from -inf to inf` | 25 ms [10–50] | 6.5 ms [5.7–11.7] |
| `solve({x^2 + y^2 = 4, x^2 - y = 2}, x = 1, y = 1)` | 28 ms [18–41] | 6.9 ms [6.4–12.1] |
| `3 ft + 7 in + 25 cm + 1.2 m + 3 yd to mm` | 31 ms [17–37] | 7.1 ms [6.2–12.7] |

## The engines alone

These were measured on the same inputs, single-threaded, in `native/bench/engine.sh`.
- Rust is the release build (thin LTO) of `qcalc-core`.
- The TS engine was bundled by esbuild and run under JavaScriptCore's `jsc`, the same engine the web view runs, and
  under node 26 (V8).
- Keystroke rows are `evaluateSheet([prefix])` for every prefix: one warm-up pass, then 3 timed passes.
- Heavy rows are one cold call and then the median of 9 warm calls.
- All three engines gave the same answer for every heavy case.

| | Rust | TS on jsc | TS on node |
|---|---|---|---|
| first call (`1+1`, lazy tables) | 17.6 ms | 9.1 ms | 12.4 ms |
| keystroke: median / p90 / p99 / max | 0.11 / 0.40 / 0.96 / 13.0 ms | 0.12 / 0.34 / 1.02 / 65.4 ms | 0.08 / 0.19 / 0.71 / 40.3 ms |
| all 4,058 keystrokes, first time each string is seen | 733 ms | 895 ms | 631 ms |
| all 4,058 keystrokes, warm | 725 ms | 773 ms | 497 ms |
| closed-form integral: PSLQ search for ∫₀¹ ln x ln(1−x) dx → `2 - π²/6`, cold / warm | 125 / 111 ms | 182 / 108 ms | 197 / 116 ms |
| numeric integral, e^(−x²) over ℝ | 0.5 / 0.29 ms | 0.8 / 0.28 ms | 0.6 / 0.20 ms |
| nonlinear system | 0.2 / 0.10 ms | 1.1 / 0.12 ms | 1.0 / 0.09 ms |
| unit chain | 0.3 / 0.20 ms | 0.7 / 0.60 ms | 0.4 / 0.29 ms |
| dense graph, `buildGraph`, 3,609 points | 15.0 / 14.8 ms | 33.1 / 31.6 ms | 22.8 / 19.8 ms |
| long sheet, 60 lines with variables and units (`heavy.json`) | 40.1 / 38.9 ms | 39.0 / 36.8 ms | 24.5 / 20.5 ms |

## Worth knowing

- **Most of the native app's advantage is the web view, not the engine.**
  - On the same inputs, Rust and TS-on-JavaScriptCore are about level: per-keystroke medians of 0.11 and 0.12 ms.
    V8 beats both on typing and on the long sheet.
  - Rust has much better tails (p99 0.96 ms against 1.02 ms, max 13 ms against 65 ms) and is twice as fast on the
    graph.
  - The memory gap of 3.5 to 7 times is WebContent plus the GPU process. The CPU gap while typing is React rendering
    and compositing.
- **The engine is a small part of the native app's keystroke cost.** The engine call is about 0.1 ms, yet the
  native app takes a steady ~4.6 ms per keystroke, and about 5 to 7 ms even for the heavy lines. I did not profile
  where the rest goes. The candidates are what a live answer does around the engine call (`engine.live` with the
  history context, the three `callJson` round trips for decorations, completions, typeset refresh) and the two
  thread hops. This is the first place to look if latency matters.
- **The Rust core's first call is slower** (17.6 ms against 9–12 ms for TS). The app hides this by warming up off
  the main thread at launch.
- **Memory climbs with use in both apps**: after typing and graphing, the web-view app is at about 4 times its idle
  footprint and the native app at about 2.5 times. These are single sessions, so this baseline can't tell growth
  that levels off from a leak.
- **On disk, the native app is the bigger one** (40.8 against 34.9 MB). Its 37 MB binary carries the Typst
  compiler and fonts compiled in. The web-view app's size is mostly the 27 MB typst.ts WASM.
- **Neither app uses CPU at idle** within the 10 ms resolution of `ps`.

## How the bench runs the apps without touching an installed Q Calc

`app_bench.py` never launches the builds themselves:
- It copies each one under the temp folder and gives the copy its own bundle id: `com.maxconine.qcalc.bench` for
  the web-view app and `com.maxconine.qcalc.native-bench` for the native one.
- Each copy gets its own default shortcut (⌃⌥⇧⌘J and ⌃⌥⇧⌘K). The native copy also gets its own support folder.
- Automatic update checks and usage sharing are off, and first-run onboarding is marked done.
- It quits every process it started by pid, then removes the copies and their defaults.

Each app is driven like this:
- **Native app**: the dev build's command channel. The channel is now named after the bundle id, and
  `bench:<spec.json>` is in `Sources/App/DevBench.swift`.
- **Web-view app**: `page_bench.js`, injected into the copy's `quick.html` only. It long-polls the script on
  127.0.0.1 for the same specs. The bar hides on any click in another app and WebKit throttles a hidden page's
  timers, so the typing pace comes from the script, not from `setTimeout`.
- **Showing the bar** in the web-view app is a reopen (`open` on the copy).

Two shared files are still written:
- The web-view copy appends to `~/Library/Logs/Q Calc/overlay.log`, the installed app's log. That path is
  hard-coded in `macos/Overlay.swift`.
- Both apps may refresh the ECB rates cache in `~/Library/Application Support/Q Calc/` (`ECBRates.swift`).

Nothing else of the installed app is read or changed.

## Iteration log

2026-10-26, same M4 Pro, with two other agents building games on the machine (load average 6 to 50). Every number
here comes from bench copies run back to back in the same session, each variant twice and interleaved. The
keystroke rows are DevBench's in-process timing.
- **Per change:** 120 of the golden inputs (about 1,865 keystrokes), typed after the 20-line session.
- **Before and after (the last table):** the full 269 inputs (4,056 keystrokes) and 5 warm launches per run.

Where the time went: `sample` of the bench copy with the unstripped binary, symbolicated against the SwiftPM
dSYM. At baseline the main thread was about 95% busy during the bench and the engine thread about 16%. About 60%
of the main thread was the SwiftUI graph update: every subview observed the whole `BarModel`, so each
`@Published` set redrew every history row and the edge tools. Another 8% was the main thread's `callJson` calls,
which waited on the single evaluation thread behind the keystroke's own evaluation.

| change | keystroke median | p90 | p99 | heavy lines |
|---|---|---|---|---|
| history rows don't observe the model (`HistoryRowView` takes values and is `.equatable()`) | 4.0 → 2.4 ms | 6.3 → 4.4 | 8.3 → 6.7 | 5.7 → 3.2 ms |
| `call_json` gets its own logic thread; boundsIn/afterTyping skipped with no ∫ Σ Π; onboarding, tour and identity target kept per input | 2.2 → 2.1 ms | 4.1 → 4.0 | 6.4 → 6.2 | ≈ |
| runtime-built regexes (variable and function names: preprocess, `with_quantities`, solve) compiled once per pattern | 2.3 → 2.1 ms | 4.3 → 4.0 | 6.7 → 6.2 | ≈ |
| edge tools don't observe the model (values in, `.equatable()`) | 2.0 → 1.4 ms | 4.2 → 3.3 | 7.5 → 5.8 | 3.3 → 2.4 ms |
| `breakRun` not sent when there's no run to break | 1.44 → 1.37 ms | 3.3 → 3.0 | 5.5 → 5.2 | 2.1 → 1.9 ms |

Before and after (the baseline build against the last commit; two runs each):

| | before | after |
|---|---|---|
| keystroke to live answer: median / p90 / p99 | 3.8 / 6.0 / 8.2 ms | 1.3–1.6 / 3.0–3.8 / 5.2–6.8 ms |
| heavy lines, median | 5.2–6.2 ms | 1.9–2.5 ms |
| warm launch to ready (median of 5) | 121 ms | 122 ms |

What's left:
- **The SwiftUI update is still the largest cost, at about 55% of the main thread.** Composer, `LiveAnswerView`
  and `InputField` still observe the whole model and redraw on any `@Published` set. A keystroke also makes two
  graph updates: one when the text changes and one when the answer lands. Splitting the model into smaller
  observable pieces is the next step, and a larger change.
- **`applySize`** (about 6%): `setFrame` relayouts the hosting view whenever the bar's height changes, as it does
  when the typeset row comes and goes.
- **Memory isn't explained by the engine.** Footprint after the typing session flips between about 48 and 136 MB
  from run to run, on both builds. The difference is "Owned physical footprint (unmapped) (graphics)": 94–114 MB in
  45–50 regions, which are window backing surfaces. The engine's heap (MALLOC_SMALL) is 30–46 MB. It is likely
  tied to resizing the panel on every height change. Not fixed here.
- **Launch didn't change.** Warm, with probes:
  - dyld to `main`: 8 ms.
  - `main` to `applicationWillFinishLaunching`: 44 ms (AppKit).
  - Status item, hotkey and log to the engine: 23 ms.
  - Engine, model and overlay preload: 7 ms.
  - Engine warm-up: 30 ms more, which is the Rust first call.
- **Not touched:** the Rust first call and the binary size.

### Round 2

Same method. The machine was busier (load 7 to 12), so latency figures are noisier than in round 1.

**The graphics footprint was the typeset row, not the panel's resizes.** With the typeset preview turned off, a
session stays at 37–41 MB. The row drew its Typst paths in a SwiftUI `Canvas`, which renders into GPU surfaces
that SwiftUI keeps, one per size. The row now draws the same paths in an AppKit view through Core Graphics
(`MathPreview.PathsNSView`). A snapshot of the row in dark mode looks the same.

| | before | after |
|---|---|---|
| footprint after the 20-line typing session (several sessions) | 48–143 MB | 41 MB |
| footprint after the graph | 70–155 MB | 64 MB |
| footprint after the keystroke bench | 168–187 MB | 46–65 MB |
| keystroke median (same load, back to back) | 1.79 ms | 1.81 ms |

Tried and dropped, with no measured win:
- **Making `LiveAnswerView` and `InputField` `.equatable()` over snapshots of what they read** (a light split of
  `BarModel`): 1.29–1.41 against 1.30–1.33 ms.
  - Their data changes on every keystroke, so they have to redraw anyway.
  - What's left is the two graph updates per keystroke: one when the text changes, one when the answer lands.
    Merging them means holding back the text's updates until the answer comes, which changes when things appear on
    screen. I didn't do it.
- **Starting the engine warm-up before the status item:** ready went from 147 to 155 ms (median of 5).
  - The ~35 ms between the bar being mounted and "engine warm" is the main thread's first render after
    `applicationDidFinishLaunching`, not the engine. The engine's reply is delivered on the main queue.
- **The status item:** finer probes put the 23–26 ms launch step all in
  `NSStatusBar.system.statusItem(withLength:)`, a window-server call. Deferring it would only move it later. I left
  it.

### After the native games merge (2026-10-26, arm64, native only)

Keystroke median 1.64 ms (p99 8.1), warm launch 180 ms (bar mounted 143 ms), footprint idle 28 MB / after typing 42 MB / after graph 65 MB, one process, 0% idle CPU, 43.7 MB on disk. The web build is unchanged from the baseline above (91 MB idle, 396 MB after a graph, 12 ms keystrokes).

### Rounds 3 and 4 (overnight 2026-10-27, arm64, native only)

Same M4 Pro, load 6 to 20 from other agents' builds, the display asleep. Each change was measured against the
build before it: both saved and run back to back, twice, interleaved (`app_bench.py --only native --runs 1`, with
`QCALC_BENCH_NATIVE_APP` pointing at the saved copy). Pictures from the `snap` dev command were compared pixel
by pixel, in light and dark.

New in the bench:
- **Peak footprint while graphing**, sampled every 0.2 s from the graph line being typed to the 3 s sample.
- **Typed into the field**: the same 269 inputs typed a character at a time through the text view (`field` in
  DevBench). This is `insertText`, so auto-pairs, symbol swaps, undo and the field's updates all run. It is
  timed from before the insert to the answer landing. Setting `model.text` (the older keystroke row) skips all
  of that.

| change | measured | kept? |
|---|---|---|
| graph plot drawn into a CPU bitmap handed to the layer, not a Canvas (round 3, carried over) | peak while graphing 195-197 → 62-65 MB; after the graph 68/157 → 62-64 MB | kept |
| engine regexes without the fully compiled DFA (round 3, carried over) | first try: keystroke 1.71/1.76 → 1.77/1.90 ms. Second try, footprint from `vmmap` over two sessions each: idle 24.1 → 23.8, after typing 41.2 → 40.0, after the graph 58.1 → 56.6 MB; keystroke 1.29/1.28 → 1.34/1.21 ms | kept on the second try |
| undo in Swift (undo.ts line for line) instead of three `callJson` round trips a keystroke | typed into the field 1.92/2.07/2.10 → 1.59-1.80 ms | kept |
| dev builds read `QCALC_PI_DAY` once (the composer read the whole environment on every redraw) | keystroke 1.66/1.79 → 1.57/1.31 ms; field 1.71/1.72 → 1.64/1.39 ms (dev builds only) | kept |
| `malloc_zone_pressure_relief` 2 s after typing or after a graph lands | no change in any footprint row | reverted |
| `lto = "fat"` | binary within 2 KB of thin | reverted |
| plot bitmap in the screen's pixel format and colour space (BGRA, the window's space) | after the graph 64/64 → 57/57 MB, peak 64 → 57-59 MB; BGRA in sRGB alone saved nothing | kept |
| pong's court as Core Animation layers, not a Canvas in a TimelineView | playing pong 167-173 → 32 MB | kept |
| `opt-level = "z"` for the Typst dependencies the preview never runs (images, PDF, WASM, bibliographies) | binary 42.2 → 45.1 MB (bigger) | reverted |
| `strip` instead of `strip -x` | executable 42.2 → 40.5 MB, on disk 44.4 → 42.9 MB | kept |
| a CPU-bitmap stand-in for SwiftUI's Canvas (ImageRenderer) on crack's dial | crack's 125 MB spikes unchanged | reverted |
| `.drawingGroup()` on connect 4's animating Canvas | peak 167 → 168 MB | reverted |
| plot drawn again only when its state, the pointer or the scale changes | graph shown, 20 unrelated model changes: 20 → 0 redraws of ~10.6 ms each | kept |
| menu bar icon bundled at 18 and 36 px (it was drawn from the 1254 px PNG, kept decoded) | idle 24/24 → 22/22, after typing 41/40 → 39/38, after the graph 57/57 → 55/55 MB; the icon is the same to the pixel | kept |

Before and after: the build this night started from (`native` at 405b057) against the last commit. Two
invocations each with `--runs 2`, interleaved, so each cell is the median over four sessions (launches: 6 cold
and 10 warm).

| | before | after |
|---|---|---|
| launch to ready, cold / warm (ms) | 558-576 / 132-153 | 548-569 / 133-134 |
| footprint, idle (bar never shown) | 25-26 MB | 24 MB [22-25] |
| footprint, bar shown | 28 MB | 26 MB |
| footprint, after the typing session | 40 MB | 38-40 MB |
| footprint, after the graph | 68 MB | 54 MB |
| footprint, peak while graphing | 198 MB | 54 MB |
| RSS, after the graph | 140 MB | 122-125 MB |
| keystroke (set text): median / p90 / p99 | 1.30-1.36 / 2.83-2.97 / 4.96-5.48 ms | 1.19-1.26 / 2.51-2.83 / 4.89-5.27 ms |
| keystroke typed into the field: median | 1.92-2.10 ms (measured on the build just before the undo change) | 1.29-1.36 ms |
| heavy lines | 1.6-2.0 ms | 1.5-2.0 ms |
| CPU while typing at 40 ms a key | 8.2-8.9% | 7.7-8.9% |
| idle CPU | 0% | 0% |
| playing pong vs the computer | 167-173 MB | 32 MB |
| app on disk | 44.4 MB | 42.8 MB |

The before build has no `field` mode, so its "typed into the field" row in the raw JSON is the set-text path again.
Its value above comes from the round's own A/B runs instead.

Where the time goes now:
- **A keystroke typed into the field** (a `sample` of the bench copy, with the unstripped binary): the main thread
  is about half busy.
  - SwiftUI's graph update is about half of that, as in round 2.
  - The text view's own insert and `textDidChange` are about a fifth.
  - Core Animation's commit is about a tenth.
  - The engine thread is busy about 18% of the time, nearly all of it waiting for `Engine.live` on the
    evaluation thread.
- **First launch after an install or update** (the "cold" row) is about 390 ms before `main`, against 7.5 ms
  warm. That time grows with the binary's size, about 6 ms a MB: a test binary took 95 ms when it was 50 KB and
  340-400 ms when padded to 42 MB. So it is the system reading the new binary, not our code. Only a smaller
  binary helps, and most of it is Typst's (its library, image and PDF decoders, WASM, bibliographies), which
  has no features to turn off.
- **Memory after typing** (+15 MB over the shown bar): about half is regexes compiled on first use (Thompson
  NFAs, one-pass DFAs, lazy-DFA caches); the rest is SwiftUI and AppKit.
- **File encryption** (FileCrypt, 256 MB of random bytes): about 1.5 GB/s each way, far beyond the relay's
  upload, so it was left alone.
- **Game frames** couldn't be timed tonight: with the display asleep the display links don't fire, so CPU per
  frame reads 0-4%.

Open, not fixed:
- **Crack and connect 4 spike to ~160 MB for 1-3 s.** Crack does it after each press, connect 4 when a game
  starts. The memory is about 30 window-wide GPU surfaces of 4 MB ("owned unmapped (graphics)"), and it falls
  back to 36-40 MB after.
  - It isn't the dial's Canvas, which a CPU bitmap didn't change, nor the note under the dial.
  - Where the surfaces come from is still unknown.
- Ember's level picker shows "Optional(nil)" at the start of its note (`EmberPanel.swift`, the `note` line).
  This is a display bug, not performance, and it was left for the games work.
