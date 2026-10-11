# ember & frost 2: the plan

Why: levels were 30 seconds and too easy. Version 2 has big scrolling stages, multi-stage levels, checkpoints,
nine new mechanics, a camera, darkness, fullscreen, and nothing locked.

The contract is `src/lib/emberTypes.ts`. Read it first, all of it: every glyph, thing, state field, event and
constant is defined and commented there. This file says who builds what, in which files, and how we know it's done.

## What is already in (the foundation commit)

- `emberTypes.ts`: the whole version 2 contract. `EMBER_VERSION = 2`, `LEVEL_FORMAT = 2`.
- `emberCamera.ts` (+ test): `cameraFor(stage, state, aspect)`, `easeCamera`, `baseView`, `cameraScale`. Done.
- `emberEngine.ts`: maps of any size, stages (`'clear'` phase, then the next stage), `respawn`, checkpoints
  (taken when both stand in the zone; a death returns to the snapshot), compiling and validating every new
  glyph and thing, light-only / dark-only blocks following `state.lit`. Every other mechanic is carried in the
  state but inert: `T` and `~` are walls, `=` is air, locks are walls, lamps / switches / arrows / keys / cannons
  do nothing. Search `TODO(ember-2:engine)`.
- `emberLevels/`: `index.ts` (the order), three packs, `legacy.ts` (the 15 old levels as one-stage levels, all
  with passing walkthroughs), `testkit.ts`.
- protocol, session, renderer, panel, solver: made to compile and pass, each with a `TODO(ember-2:<owner>)`
  header saying what's missing. Levels are already unlocked; `retry()` goes to the checkpoint; `restart()` exists.
- Fullscreen (⌃F) is already on this branch as a shared hook: `src/lib/GAME_FULLSCREEN.md`. PANEL adopts it.

## Rules for everyone

1. One shared worktree, branch `ember-2`. **Edit only the files you own** (table below). Need something in
   someone else's file? Tell the coordinator; don't touch it.
2. **Frozen, nobody edits**: `emberTypes.ts`, `emberCamera.ts`, `emberLevels/index.ts`. A contract change goes
   through the coordinator, who makes it and tells every owner.
3. Commit small and often, only your own paths: `git commit <your files> -m "..."` (never `git add -A`, never
   `git commit -a`: other agents' half-done work is in the same tree). If the index is locked, wait and retry.
   Repo style: one short lowercase line, ending with the Co-Authored-By line. Don't push.
4. Before each commit: `npx tsc -b` and your own tests. If `tsc` fails in a file you don't own, it's someone
   mid-edit: wait a minute and retry; don't fix it for them.
5. The sim stays deterministic: fixed 60 Hz, no `Math.random`, `Date`, or Set / object-key iteration in
   `emberEngine.ts`. State stays plain JSON. No new dependencies.
6. Tests: `npx vitest run src/lib/ember src/components/ember --maxWorkers=4`.

## Who owns what

| Agent | Owns (exclusive) |
|---|---|
| ENGINE | `src/lib/emberEngine.ts`, `src/lib/emberEngine.test.ts` |
| RENDER | `src/components/emberDraw.ts`, `src/components/emberFx.ts`, `src/components/emberFx.test.ts`, `src/lib/emberSound.ts`, new `src/components/emberDraw.test.ts`, new `src/lib/emberSound.test.ts` |
| NET | `src/lib/emberSession.ts`, `src/lib/emberSession.test.ts`, `src/lib/emberProtocol.ts`, new `src/lib/emberProtocol.test.ts` |
| PANEL | `src/components/EmberPanel.tsx`, `src/components/EmberPanel.css`, `src/lib/emberProgress.ts`, `src/lib/emberProgress.test.ts`, `src/lib/ember.ts` (+ test); `macos/` only if fullscreen needs it |
| SOLVER | `src/lib/emberSolver.ts`, `src/lib/emberLevels.test.ts` (set-wide and solver tests), `src/lib/emberLevels/testkit.ts` (additive only: keep every export's signature) |
| LEVELS-A | `src/lib/emberLevels/basics.ts`, `basics.test.ts`, `legacy.ts` (to delete) |
| LEVELS-B | `src/lib/emberLevels/depths.ts`, new `depths.test.ts` |
| LEVELS-C | `src/lib/emberLevels/summit.ts`, new `summit.test.ts` |
| nobody | `emberTypes.ts`, `emberCamera.ts` (+ test), `emberLevels/index.ts`, `gameFullscreen*`, `useGameFullscreen.ts` |

## Interfaces between agents (frozen)

- **Engine API** (everyone): `validateLevel(def)`, `compileLevel(def): Level`, `initState(level, deaths?)`,
  `startStage(level, index, from?)`, `step(level, state, inputs)`, `respawn(level, state)`,
  `stageOf(level, state): Stage`, `tileAt(stage, state, tx, ty)`, `gateRect(stage, state, i)`,
  `traceBeams(stage, state)`, `scoreOf(level, state)`. ENGINE may add exports (e.g. `lockRect`), never change these.
- **Renderer API** (RENDER → PANEL): `new EmberRenderer(canvas)`, `resize()`, `reset()`, `refreshColors()`,
  `draw(stage, prev, cur, alpha, events, nowMs, opts)`, `drawThumb(canvas, stage)`. PANEL passes
  `stageOf(compiled, state)`. `DrawOpts` is `{ me, reduceMotion }`; RENDER may add optional fields only.
  The renderer owns the camera: it calls `cameraFor` with the canvas's own aspect and eases with `easeCamera`,
  jumping (no ease) when the `stage` object changes or a `respawn` event arrives. `resize()` must take whatever
  css box the canvas has; today it forces 16:9.
- **Session API** (NET → PANEL): `EmberView` and `EmberGame` in `emberTypes.ts`. No `unlocked` any more.
  `prev === state` whenever the stage changed or the players respawned, so nothing blends across a jump.
  `createLocalGame(now, start?)`, `createOnlineGame(role, send, now, start?)`.
- **Sound** (RENDER → PANEL): `createEmberSound()` → `{ play(event), setMuted, muted }`, unchanged.
- **Levels**: `LEVELS` from `./emberLevels`. `LEVELS[0]` is always `first-light`, one stage, one screen.
- **Level tests**: `describeLevel(def, walkthrough, checks?)`, `runWalkthrough`, `play`, `traceScript` from `testkit.ts`.

## The design in short

**Stages.** `LevelDef.stages` is 1–6 `StageDef`s, each a map of 32×18 up to 128×72 tiles with its own things.
`GameState.stage` is the index; every per-thing array in the state belongs to that stage. Both players in their
doors: on the last stage `'won'`; otherwise phase `'clear'` for `STAGE_TICKS`, then `startStage(next)` carrying
the clock, the deaths and the gems banked. Gravity, keys and lights don't carry over. Events `clear`, `stage`.
The clock (`tick`) only counts in `'play'`, and never resets within a level. `par` is for the whole level.

**Checkpoints.** A `'C'` glyph or `checkpoint` thing: a flag and a zone. Taken the first tick both players are
alive, grounded, not in a cannon and overlapping the zone. `state.saved` is then a copy of the whole state
(`Snapshot`), `state.cp` the flag. A death: phase `'dead'` for `DEATH_TICKS`, then `respawn()`: the snapshot
again (both players where they stood, every lever, box, gem, key as it was) with the clock of the moment of
death and one more death. No flag taken yet in this stage: the stage's start. `saved` is shared by reference,
never mutated, never sent over the network.

**Camera.** `cameraFor(stage, state, aspect)` is pure: the smallest rect of the canvas's aspect that is at least
one view (32×18 tiles) and holds both players with margins, centred between them, kept inside the stage (or
centred over a stage smaller than the rect). Zoom 1 until they're ~19 tiles apart sideways or ~9 up and down,
then out to `CAM_MIN_ZOOM` 0.25, which holds the biggest stage whole: **both players are always on screen**.

## The mechanics (what ENGINE implements; the exact numbers are in the contract)

Proposed tick order: 1 clocks (cooldowns, lamp timer, arrow and crumble timers) · 2 interact, in this priority
when several overlap: lever, lamp, gravity switch, mirror · 3 channels (`CH_LIT` follows `lit`, but is never
listed in `state.on`) · 4 gates, movers · 5 thin water, ice · 6 boxes · 7 players · 8 pools, gems, keys, locks,
arrows, portals, cannons, doors · 9 checkpoints, phase. Keep the header comment in the engine true.

| Mechanic | Rule | The co-op puzzle |
|---|---|---|
| **Lights** (`dark`, `lamp`, `'O'`, `'D'`, `dark` patches) | `lit` toggles on a lamp press; a lamp with `secs` sets the non-starting state for that long (`litT`). `'O'` blocks are solid lit, `'D'` solid dark. A body inside a block that turns solid is pushed out against its gravity up to `LIGHT_EJECT_TILES`, else dies (`crush`). Event `light`. | One stands at the switch while the other crosses a bridge that only exists in the dark, lit only by their own glow; ember's bigger glow makes ember the guide. |
| **Trampoline** `'T'` | Landing along gravity on its top (or, flipped, its underside) launches at `clamp(impact × GAIN + ADD)`. Releasing jump cuts it. Boxes don't bounce. Event `bounce` with `v`. | One drops from high up to reach a ledge the other can't; a gate held open decides how far the fall is. |
| **Double-jump arrow** `'^'` | Touched in the air with `air` 0: `air = 1`, arrow away for `ARROW_RESPAWN_TICKS`. A jump press in mid-air (no coyote) spends it at `AIR_JUMP_V`; landing loses it. Events `arrow`, `airjump`, `arrowback`. | One arrow, two players: who takes it, and the other waits for it to come back while holding something. |
| **Gravity switch** `gswitch` | Flips `grav` for `who` (`self`, `other`, `fire`, `frost`, `both`). Everything vertical in the player's step mirrors: gravity, jump, fall cap, ground, coyote, landing, riding the underside of movers. Fans always blow up. Pools only harm an upright player standing on them; their sides and undersides are plain wall. Doors need an upright player. Plates and buttons with `ceil` sit on ceilings. Boxes never flip. Events `gswitch`, `flip`. | One walks the ceiling over a lava floor to a ceiling plate that opens the other's path; `who: 'other'` makes each the other's remote control. |
| **One-way platform** `'='` | A slab over the top `ONEWAY_H` px. Solid only to a body moving along its own gravity whose feet started beyond it: upright lands on top and jumps up through; flipped lands on the underside and jumps down through. Boxes land on it. Never solid sideways. | Tall climbs with no way back down except together; a floor for one is a ceiling for the flipped other. |
| **Crumbling block** `'~'` | A player grounded on it starts it (`shake`); `CRUMBLE_TICKS` later it's gone (`break`) whoever is on it; `CRUMBLE_GONE_TICKS` later it returns once nothing is inside (`mend`). Boxes don't start it but fall with it. | A bridge only one can cross at a time; the first across has to hold a plate before it grows back for the second. |
| **Keys and locks** `'k'`, `lock` | Either player touching a key adds one to the team's `keys`. A player touching a closed lock with `keys > 0` spends one and opens it for good; open locks are gone (not solid, don't block beams). Events `key`, `unlock`. One key per lock (validated). | The key is down ember's lava lane, the lock is on frost's path: fetch for each other. |
| **Cannon** `cannon` | A player whose centre enters an empty cannon's tile (and `cannonCd` 0) is loaded: pinned, no control, immune. Fires when `fire` holds, or after `CANNON_FUSE_TICKS` with no `fire`. The shot sets vx, vy; `fly` ticks of no steering, gravity on, ending on any hit or landing. Events `load`, `fire`. | One climbs in, the partner stands on the plate that fires them across a gap nobody can jump. |

Interplay ENGINE must pin with tests: trampoline under a one-way; arrow jump while flipped; a flipped player on
a crumbling block's underside; a cannon shot into a portal; lights changing under a box; a checkpoint snapshot
taken mid-lamp-timer, mid-crumble and with a key carried, then restored.

## ENGINE

Depends on: the contract only. Everyone else depends on ENGINE, so **land mechanics in this order and commit
after each**, each with tests, saying in the commit message which mechanic landed:

1. `'='` one-way, `'T'` trampoline, `'~'` crumbling (tile mechanics; every pack uses them)
2. `'^'` arrows
3. lights: lamp, timed lamp, eject / crush, `light` events
4. keys and locks
5. gravity: `grav` through the whole player step, `gswitch`, ceiling plates, mirrored riding
6. cannons
7. hardening: determinism and JSON round-trip tests over a state with every new field in use; a 128×72 stage
   steps in under 0.1 ms on average (the guest replays up to 40 steps per host state)

Also yours: keep `stage` / `respawn` / checkpoint behaviour as tested (extend, don't change); a state that came
off the network has no `saved`, no `plates`, no `sensors` and must still step.

Acceptance: all existing engine tests still pass untouched in meaning; each mechanic has tests for its rule,
its events, and its interplay rows above; `step` never mutates its input; `grep -c "TODO(ember-2:engine)"` is 0.

## RENDER

Depends on: contract, `emberCamera.ts`, engine API. Can start at once: every state field and stage array
already exists, so art can be drawn against hand-made states before ENGINE moves them.

- Camera: one world → canvas transform from `cameraFor` + `easeCamera`; any canvas size and aspect; overhang
  filled with wall. Cull everything to the camera rect: a 128×72 stage must not cost more per frame than a
  32×18 one at zoom 1. The static layer can't be one bitmap at canvas scale any more: bake it in chunks, or once
  per stage at a capped resolution.
- Darkness: when `!lit`, everything outside the glows is dark (inside `stage.darks` if there are any, else
  everywhere). Glows: `GLOW_EMBER`, `GLOW_FROST` around the players, `GLOW_SMALL` around lava, beams, gems,
  lamps, lit checkpoints, portals. Fade over ~0.3 s on a `light` event. `'O'` / `'D'` blocks show a ghost
  outline while absent, so a puzzle is readable.
- Art for: trampoline (squash on `bounce`), one-way, crumbling (shake by `state.crumble`, gone, mending),
  arrow (absent while `state.arrows[i] > 0`), key and lock, lamp, gravity switch (shows `who`), cannon (aim,
  loaded, fuse), checkpoint flag (lit when `i === state.cp`), ceiling plates, flipped players (upside down,
  with a clear tell), a held-arrow tell on the player (`air`), keys carried.
- Stage change: a short wipe during `'clear'`; `respawn` snaps the camera.
- Sound + particles for every new event kind in `GameEvent` (the switch in `emberSound.ts` must cover them all).
- `drawThumb(canvas, stage)` letterboxes a stage of any shape.

Acceptance: `emberDraw.test.ts` covers the pure parts (world↔canvas mapping, culling bounds, light mask
geometry); no draw call throws for a state with every array empty or every mechanic active; at 1920×1080 on
the biggest stage at widest zoom a frame stays under 6 ms; one-screen stages look as they did; `TODO(ember-2:render)` gone.

## NET

Depends on: contract, engine API. Can start at once on the wire format; gravity-flip prediction tests wait
for ENGINE step 5.

- The wire: every new state field and event round-trips today, but plainly. Make it compact: leave out
  all-default arrays and trailing default player fields, send `thin` / `crumble` / `arrows` sparsely, and fill
  the defaults back in against the stage on receipt (`fitsLevel` is where a state meets its level).
  Budget: a typical state message on a big stage ≤ 1.2 kB, never over `MAX_TEXT`; **still `STATE_HZ` 30, and
  no new per-frame messages** (the relay bills per message). Never send `saved`.
- Stage changes and respawns: the guest holds its prediction at every phase change until the host's state
  arrives (it has no `saved`, so it can't respawn itself); corrections snap, not ease, across a stage change, a
  respawn, a portal, a cannon shot and a gravity flip. Key frames keep counting across both (same attempt).
- `retry` (checkpoint) and `restart` (level) from either side; notes for both.
- Version mismatch still ends with the update line (`EMBER_VERSION` is 2).
- `emberProtocol.test.ts`: round trip of a state with every field used, every event kind, malformed input → null.

Acceptance: host and guest agree after a stage change, a death at a checkpoint, a lamp press and a gravity
flip, at 80 ms lag and with dropped messages; message-rate tests still pass; frame-size test for a 128×72
stage with 60 gems, 40 crumbling tiles, 30 thin tiles; `TODO(ember-2:net)` gone.

## PANEL

Depends on: session API, renderer API, `GAME_FULLSCREEN.md`. Can start at once.

- Fullscreen: adopt `useGameFullscreen` (⌃F, esc), show `fs.hint` on entering a level, size the canvas from
  the hook. Outside fullscreen the canvas stays 16:9; nothing else may assume it.
- Select: everything open (done), highlight starts on `firstUnfinished`. Each card: stars, best time, gems,
  and how long it is (stages, or a size word). More than 12 levels must fit and scroll.
- HUD: stage `2 / 3` on multi-stage levels, the stage's name and hint when it begins, gems for the whole level
  (`scoreOf`), keys carried, the clock (never resets), deaths.
- Death and checkpoints: a small "checkpoint" note on the `checkpoint` event; after a death say where they
  came back; `R` back to the checkpoint, `⇧R` the whole level; both listed in the keys line.
- Win: time against par, gems, deaths, stars. Progress records the same three bests as before; if you add
  deaths, keep old saved data readable.

Acceptance: `npm run build`; progress tests; by hand in the browser: a one-screen level looks unchanged, ⌃F
in and out mid-level keeps playing, a two-stage level shows its stage change, `R` and `⇧R` do what they say,
online select and play still work in two windows; `TODO(ember-2:panel)` gone.

## SOLVER (and level tools)

Depends on: contract. Start at once: the designers need this before they can verify anything.

1. First, today: walkthrough tooling in `testkit.ts` (additive). `traceScript` exists; add what makes long
   scripts practical, e.g. `walkTo(player, tileX)` / `jump` builders that emit `Script` rows, and a helper that
   reports where a walkthrough died.
2. `reachability(stage)` for the new tiles and things, optimistic like the rest: `'='` stands from above and
   passes from below; `'T'` gives a tall jump (up to 8 tiles); `'~'` is floor; `'O'` and `'D'` are floor and
   air both; a lock is open once any key is reachable by either player (fixed point, like ice); an arrow is a
   mid-air standing cell worth 2 more tiles up; a cannon reaches the cells along its arc; once a gravity switch
   that can flip a player is reachable, that player also gets ceiling reach (the mirrored rules); ceiling plates
   need nothing. Stages up to 128×72 in well under 100 ms.
3. `emberLevels.test.ts`: the set-wide checks; at the end, exactly 14 levels, ids unique, `first-light` first.

Acceptance: solver tests for each rule above (a reachable and an unreachable case each); no shipped level sets
`skipReach`; `TODO(ember-2:solver)` gone.

## LEVELS (three designers)

14 levels in all. Positions are the order in `LEVELS` (`index.ts` concatenates basics, depths, summit).
"Screen" = 32×18 tiles. Times are the scripted walkthrough's clock, which is faster than people play.

| # | Pack | Level | Size | Mechanics | Walkthrough |
|---|---|---|---|---|---|
| 1 | A | `first-light`, kept exactly as it is: the tutorial | 1 screen | pools, gems, doors | ~25 s |
| 2 | A | boxes and plates, from `crates` / `hold-the-door` / `lockstep` | 2 wide | + one-ways, 1 checkpoint | 45–60 s |
| 3 | A | ice and thin water, from `thaw` / `white-crossing` / `cold-storage` | 2 wide × 2 tall | + crumbling, 2 checkpoints | 60–75 s |
| 4 | A | fans and lifts, from `updraft` / `the-lifts` | 1 wide × 3 tall | + trampolines, arrows, 2 checkpoints | 70–90 s |
| 5 | A | beams and mirrors, from `break-the-beam` … `last-light` | 2 stages, each 2 wide | + keys and locks | 90–110 s |
| 6 | B | first dark: a lamp, `O` / `D` bridges | 2 wide | lights, 1 checkpoint | 50–60 s |
| 7 | B | a shaft climbed in the dark | 1 wide × 4 tall | trampolines, arrows, one-ways | 70–90 s |
| 8 | B | the timed lamp: one lights, one runs | 3 wide | timed lamp, crumbling, 2 checkpoints | 80–100 s |
| 9 | B | a dark maze with `dark` patches | 2 wide × 2 tall | keys, locks, portals, boxes | 100–120 s |
| 10 | B | two stages, scrolling, half in the dark | 2 stages, 2–3 wide | everything so far | 110–140 s |
| 11 | C | gravity: your own switch | 2 wide × 2 tall | `gswitch` self, ceiling plates | 70–90 s |
| 12 | C | one flipped, one not | 3 wide × 2 tall | `other` / `fire` / `frost` switches, lava ceilings, one-ways | 100–120 s |
| 13 | C | cannons: your partner fires you | 2 stages | cannons, trampolines, arrows | 120–150 s |
| 14 | C | the finale | 3 stages, each ≥ 2 screens | everything | 150–180 s |

Rules for every level:

- New ids for reworked levels (old bests were earned on easier maps); only `first-light` keeps its id. Never
  reuse an id from `legacy.ts`.
- Real co-op: each player must do something the other can't at least twice per stage. No stretch a single
  player could solo.
- Hard but fair: a checkpoint every 30–45 s of walkthrough, at a place both pass through (size the zone to
  cover both lanes if they run side by side). **No softlocks**: nothing spendable (keys, melted ice, a box
  pushed off a ledge) may leave a checkpointed state unwinnable. If a stuck state can exist, a pool must be in
  reach to end it.
- When both need to read the screen, keep them within two screens of each other (zoom ≥ 0.5).
- At least two gems of each colour per level; on long levels about one pair per 20 s, some off the main path.
- `par` = the walkthrough's time × 1.6, rounded up to 5 s.
- Every level ships in its pack's test file through `describeLevel(def, walkthrough, { minSecs })`: one script
  per stage, winning in the real engine with zero deaths. `minSecs` is the low end of the column above.
- The solver's reachability check passes for every stage (`skipReach` only while SOLVER hasn't landed the
  mechanic; none left at the end).
- A solution note above each level, as the old ones have: tile coordinates, who does what, in order.

LEVELS-A also: keep the old walkthroughs passing until each old level is replaced, then delete `legacy.ts`.

## Order

- Day one, in parallel: ENGINE step 1; SOLVER step 1; RENDER camera; NET wire; PANEL fullscreen and select;
  all three designers **design and write maps** (the compiler accepts every glyph and thing now, so
  `validateLevel` and the structure tests run today).
- A designer can only **verify a walkthrough** once ENGINE has landed the mechanics that level uses:
  levels 2–3 after step 1 · 4 after step 2 · 5 after step 4 · 6, 8 after step 3 · 7 after step 2 ·
  9, 10 after step 4 · 11, 12 after step 5 · 13, 14 after step 6. Write maps for later levels while waiting.
- Reachability for a level waits on SOLVER step 2 for its mechanics.
- NET's gravity tests wait on ENGINE step 5; RENDER's art doesn't wait on anything.
- Last, the coordinator: `grep -rn "TODO(ember-2" src --include="*.ts*"` is empty, `legacy.ts` is gone, 14 levels, full test
  run, `npm run build`, `npm run lint`, and a play-through of levels 1, 8 and 14 locally and online.
