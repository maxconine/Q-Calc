## Q Calc native Windows smoke test

**All required checks passed.**

| # | check | kind | result | detail | screenshot |
|---|---|---|---|---|---|
| L1 | first launch shows the bar | required | pass | shown on the first launch (even with --autostart): True | L1-first-launch-shows-the-bar.png |
| L2 | opening it with nothing running shows the bar | required | pass | shown: True; in front: True ('Q Calc' class QCalcBar (qcalc)) | L2-opening-it-with-nothing-running-shows-the-bar.png |
| L3 | the launch at sign in (--autostart) starts hidden | required | pass | alive: True; bar created: True; visible windows: 0; pipe visible: False | L3-the-launch-at-sign-in-autostart-starts-hidden.png |
| L4 | tray icon | informational | pass | found 'Q Calc' | L4-tray-icon.png |
| H1 | hotkey shows and focuses | required | pass | registered: ctrl+alt+shift+f9 (wanted ctrl+alt+shift+f9); shown in 66 ms: True; in front: True ('Q Calc' class QCalcBar (qcalc)) | H1-hotkey-shows-and-focuses.png |
| H2 | hotkey again hides (toggle) | required | pass | hidden by the second press: True | H2-hotkey-again-hides-toggle.png |
| H3 | hotkey refocuses over another app | required | pass | other app in front first: True; shown: True; in front: True ('Q Calc' class QCalcBar (qcalc)) | H3-hotkey-refocuses-over-another-app.png |
| E1 | esc hides | required | pass | hidden after Esc | E1-esc-hides.png |
| E2 | click elsewhere hides | required | pass | front after the click: 'qcalc-e2e-other' class WindowsForms10.Window.8.app.0.17d3e36_r13_ad1 (powershell); hidden: True | E2-click-elsewhere-hides.png |
| E3 | focus loss hides | required | pass | front after Alt+Tab: 'qcalc-e2e-other' class WindowsForms10.Window.8.app.0.17d3e36_r13_ad1 (powershell); hidden: True | E3-focus-loss-hides.png |
| S1 | a second launch hands off to the first | required | pass | second launch exited: True (code 0); first copy showed the bar: True; my processes alive: 1 | S1-a-second-launch-hands-off-to-the-first.png |
| S2 | a second launch with --autostart stays quiet | required | pass | second launch exited: True; bar stayed hidden: True | S2-a-second-launch-with-autostart-stays-quiet.png |
| T1 | arithmetic answer | required | pass | '2+3*4' shows '14'; answer slot painted: 3.02% ink, 14 colours | T1-arithmetic-answer.png |
| T2 | unit conversion | required | pass | '12 kg to lb' shows '26.4555 lbs'; answer slot painted: 14.13% ink, 15 colours | T2-unit-conversion.png |
| T3 | percent phrase | informational | pass | '20% off 80' shows '64'; answer slot painted: 3.7% ink, 15 colours | T3-percent-phrase.png |
| T4 | solve | informational | pass | '3x+2=11' shows 'x = 3'; answer slot painted: 3.13% ink, 19 colours | T4-solve.png |
| T5 | enter commits to the tape | required | pass | rows 0 -> 1; last '19*3' = '57'; line now '' | T5-enter-commits-to-the-tape.png |
| C1 | ctrl+c copies the answer | required | pass | clipboard after Ctrl+C: '42' | C1-ctrl-c-copies-the-answer.png |
| U1 | the answer shows the hand and a tooltip under the pointer | required | pass | hand over the answer: True; tooltip after a rest: True; gone when the pointer left: True | U1-the-answer-shows-the-hand-and-a-tooltip-under-the-pointer.png |
| T6 | history and the draft survive a restart | required | pass | rows before 2, after the restart 2, 41+1 kept: True; draft back: True (line '100/8') | T6-history-and-the-draft-survive-a-restart.png |
| P1 | graph panel opens and the bar grows | required | pass | height 236 -> 793 px; graph 'sin(x)' with 401 points | P1-graph-panel-opens-and-the-bar-grows.png |
| P2 | sys panel opens | required | pass | height 236 -> 401 px; panel open: True; x+y=3, x-y=1 solved: 'x = 2 y = 1' | P2-sys-panel-opens.png |
| W1 | the bar drags and the spot is kept | required | pass | dragged 200, 120 px: moved 200, 120 px; still shown: True; same spot after hide and show: True | W1-the-bar-drags-and-the-spot-is-kept.png |
| W2 | the left edge widens the bar | required | pass | 1020 px wide, then 1170 px; painted to the new edge: True ('ffffff' vs 'ffffff'); back to the standard width: True | W2-the-left-edge-widens-the-bar.png |
| W3 | double click on the background recentres | informational | pass | bar middle at x 1920 on a 3840 px wide screen | W3-double-click-on-the-background-recentres.png |
| G1 | ctrl+comma opens settings | required | pass | settings window open: True, in front: True; painted: 15.6% ink, 911 colours | G1-ctrl-comma-opens-settings.png |
| G2 | a change applies live (appearance: dark) | required | pass | theme now 'dark', saved 'dark'; settings window luminance 242 -> 45; bar luminance 45 | G2-a-change-applies-live-appearance-dark.png |
| G3 | esc closes settings (and light comes back) | required | pass | back to light: True; closed by Esc: True; bar luminance 254 | G3-esc-closes-settings-and-light-comes-back.png |
| V1 | look over light and dark (rounded corners, shadow) | informational | pass | look-light.png 1020x236 px, answer shown; corner eaeaea vs edge a5a9ae; below aaafb6 vs 40 px down c1c6cf; rounded True, shadow True / look-dark.png 1020x236 px, answer shown; corner 1b1b1b vs edge 474c55; below 232a36 vs 40 px down 28303d; rounded True, shadow True | V1-look-over-light-and-dark-rounded-corners-shadow.png |
| V2 | smoke room grows the window | informational | pass | height 236 px, then 236 px (top moved 0 px), or an effect window: True | V2-smoke-room-grows-the-window.png |
| M1 | a game opens from the bar and closes | required | pass | game window 'pong' opened: True, painted 1.9% ink; closed by Alt+F4: True; app alive: True | M1-a-game-opens-from-the-bar-and-closes.png |
| K1 | hotkey works after another process held it at launch | required | pass | held by the test at launch: True; registered while held: False; taken after release: True; shown by the chord: True; log: [native +23.7ms] hotkey ctrl+alt+shift+f9 registered (launch, attempt 1) / [native +25.1ms] hotkey ctrl+alt+shift+f9 not registered (launch, attempt 1): Hot key is already registered. / [native +1018.5ms] hotkey ctrl+alt+shift+f9 not registered (retry, attempt 2): Hot key is already registered. / [native +2026.3ms] hotkey ctrl+alt+shift+f9 registered (retry, attempt 3) | K1-hotkey-works-after-another-process-held-it-at-launch.png |
| Z1 | idle memory after use, hidden and trimmed | target | pass | private working set, hidden after use 4.777 MB (target < 12.5 MB) (after the trim 0.9 MB; private bytes 6.8 MB) | Z1-idle-memory-after-use-hidden-and-trimmed.png |
| Z2 | keystroke to painted answer | target | pass | keystroke -> answer painted, median 0.584 ms (target < 0.8 ms) (n 92, p90 1.197, p99 29.556, timeouts 0) | Z2-keystroke-to-painted-answer.png |
| Z3 | warm launch to ready | target | pass | warm launch -> ready (in the tray) 29.3 ms (target < 75 ms) (runs: 29.1, 29.7, 30.2, 29.3, 28.7) | Z3-warm-launch-to-ready.png |

### Targets (AGENTS.md; soft)

| what | measured | target | met |
|---|---|---|---|
| private working set, hidden after use | 4.777 MB | < 12.5 MB | yes |
| keystroke -> answer painted, median | 0.584 ms | < 0.8 ms | yes |
| warm launch -> ready (in the tray) | 29.3 ms | < 75 ms | yes |

<details><summary>runner</summary>

- exe: C:\Users\maxco\Documents\GitHub\QCALC\Q-Calc\.claude\worktrees\windows-native\native\target-coord\release\qcalc.exe
- instance: e2e7
- data: C:\Users\maxco\AppData\Local\Temp\qcalc-e2e-e2e7-c46fa303\data
- os: Microsoft Windows NT 10.0.26200.0
- session: 1 (console session 1)
- input desktop: Default
- screen: 3840x2160
- apps use light theme: 1
- foreground lock timeout: 2147483647 ms
- foreground at start: 'Claude' class Chrome_WidgetWin_1 (claude)

</details>
