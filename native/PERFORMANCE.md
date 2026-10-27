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
