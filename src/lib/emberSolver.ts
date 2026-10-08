import type { Element, LevelDef, ThingDef } from './emberTypes'

// a conservative reachability check for ember & frost levels, used by tests to catch broken maps (a door or gem
// nobody can get to, a typo'd wall). it works on the tile grid, not the physics: a player is a 1x2-tile body whose
// feet sit in a "standing cell" above something solid. mechanics are treated optimistically (gates open, plates
// held, movers anywhere along their path, fans on) since the point is geometry, not proving the puzzle.
// ice and thin water are the exceptions: ice only melts where fire can reach it, thin water only freezes where
// frost can, so those run as a small fixed point.

export type ReachOpts = {
  // how many tiles a jump can climb. 3 matches the physics (a stretch); 2 is the comfortable design rule
  jump?: number
  // how many tiles sideways a jump carries at a climb of 2 or less (a flat gap of carry - 1 tiles)
  carry?: number
}

export type Reach = {
  fireDoor: boolean
  frostDoor: boolean
  fireGems: number
  frostGems: number
  gemTotals: [fire: number, frost: number]
  // gems the solver can't get its own element to, as tile coordinates
  missedGems: Array<{ x: number; y: number; el: Element }>
  // per tile, row-major over the map: a standing cell (feet) the player reaches, and any cell their body passes
  stand: [fire: boolean[], frost: boolean[]]
  touched: [fire: boolean[], frost: boolean[]]
  melted: number // ice blocks fire can melt
  frozen: number // thin water tiles frost can freeze
}

type Ctx = {
  w: number
  h: number
  map: string[]
  iceAt: Int16Array // ice block index + 1 per cell, 0 for none
  melted: boolean[] // per ice block
  frozen: boolean[] // per cell (only thin cells matter)
  open: Uint8Array // gate and mover cells: passable whatever the map says
  floor: Uint8Array // gate tops and mover paths: solid to stand on
  fanAt: Int16Array // fan index + 1
  fans: Array<Extract<ThingDef, { k: 'fan' }>>
  portalAt: Int16Array // portal index + 1 (both tiles)
  portals: Array<Extract<ThingDef, { k: 'portal' }>>
  portalOther: number[]
}

const SOLID = '#LWGw'

const NONE = 0
const SAFE = 1
const DEADLY = 2

function build(def: LevelDef): Ctx {
  const map = def.map
  const h = map.length
  const w = map[0]?.length ?? 0
  const n = w * h
  const at = (x: number, y: number) => y * w + x
  const inside = (x: number, y: number) => x >= 0 && y >= 0 && x < w && y < h
  const iceAt = new Int16Array(n)
  const open = new Uint8Array(n)
  const floor = new Uint8Array(n)
  const fanAt = new Int16Array(n)
  const portalAt = new Int16Array(n)
  const fans: Ctx['fans'] = []
  const portals: Ctx['portals'] = []
  let ices = 0
  for (const t of def.things) {
    if (t.k === 'ice') {
      ices++
      for (let y = t.y; y < t.y + t.h; y++) for (let x = t.x; x < t.x + t.w; x++) if (inside(x, y)) iceAt[at(x, y)] = ices
    } else if (t.k === 'gate') {
      // open: its cells are free. closed: its top is a floor someone might stand on
      for (let y = t.y; y < t.y + t.h; y++) for (let x = t.x; x < t.x + t.w; x++) if (inside(x, y)) open[at(x, y)] = 1
      for (let x = t.x; x < t.x + t.w; x++) if (inside(x, t.y)) floor[at(x, t.y)] = 1
    } else if (t.k === 'mover') {
      // every tile-step along the path: the platform could be parked there, so it's both free and a floor
      const steps = Math.max(1, Math.ceil(Math.max(Math.abs(t.to.x - t.x), Math.abs(t.to.y - t.y))))
      for (let i = 0; i <= steps; i++) {
        const px = Math.round(t.x + ((t.to.x - t.x) * i) / steps)
        const py = Math.round(t.y + ((t.to.y - t.y) * i) / steps)
        for (let y = py; y < py + t.h; y++)
          for (let x = px; x < px + t.w; x++)
            if (inside(x, y)) {
              open[at(x, y)] = 1
              floor[at(x, y)] = 1
            }
      }
    } else if (t.k === 'fan') {
      fans.push(t)
      for (let y = t.y - t.h + 1; y <= t.y; y++) for (let x = t.x; x < t.x + t.w; x++) if (inside(x, y)) fanAt[at(x, y)] = fans.length
    } else if (t.k === 'portal') {
      portals.push(t)
      for (const y of [t.y, t.y - 1]) if (inside(t.x, y)) portalAt[at(t.x, y)] = portals.length
    }
  }
  const portalOther = portals.map((p, i) => portals.findIndex((q, j) => j !== i && q.pair === p.pair))
  return {
    w,
    h,
    map,
    iceAt,
    melted: new Array<boolean>(ices).fill(false),
    frozen: new Array<boolean>(n).fill(false),
    open,
    floor,
    fanAt,
    fans,
    portalAt,
    portals,
    portalOther,
  }
}

function chr(c: Ctx, x: number, y: number): string {
  return c.map[y]![x] ?? '#'
}

function passable(c: Ctx, x: number, y: number): boolean {
  if (x < 0 || y < 0 || x >= c.w || y >= c.h) return false
  const i = y * c.w + x
  if (c.open[i]) return true
  const ice = c.iceAt[i]!
  if (ice && !c.melted[ice - 1]) return false
  return !SOLID.includes(chr(c, x, y))
}

// what the tile under a player's feet does to them
function support(c: Ctx, el: Element, x: number, y: number): number {
  if (x < 0 || y < 0 || x >= c.w || y >= c.h) return SAFE
  const i = y * c.w + x
  const ice = c.iceAt[i]!
  if (ice && !c.melted[ice - 1]) return SAFE
  if (c.floor[i]) return SAFE
  switch (chr(c, x, y)) {
    case '#':
      return SAFE
    case 'L':
      return el === 'fire' ? SAFE : DEADLY
    case 'W':
      return el === 'frost' ? SAFE : DEADLY
    case 'G':
      return DEADLY
    case 'w':
      return el === 'frost' || c.frozen[i] ? SAFE : DEADLY
    default:
      return NONE
  }
}

// one player's reach on the current ice/thin state. returns standing cells and touched body cells
function reach(c: Ctx, el: Element, start: { x: number; y: number }, o: Required<ReachOpts>) {
  const n = c.w * c.h
  const stand = new Array<boolean>(n).fill(false)
  const touched = new Array<boolean>(n).fill(false)
  const fanDone = new Array<boolean>(c.fans.length).fill(false)
  const portalDone = new Array<boolean>(c.portals.length).fill(false)
  const queue: number[] = []
  const later: Array<() => void> = []
  const clear = (x: number, y: number) => passable(c, x, y) && passable(c, x, y - 1)

  const addStand = (x: number, y: number) => {
    const i = y * c.w + x
    if (stand[i]) return
    stand[i] = true
    queue.push(i)
  }
  const commit = (path: number[]) => {
    for (const i of path) touched[i] = true
  }
  // a body cell the player is passing through. fans and portals grab whoever touches them
  const visit = (path: number[], x: number, y: number) => {
    for (const yy of [y, y - 1]) {
      if (yy < 0) continue
      const i = yy * c.w + x
      path.push(i)
      const f = c.fanAt[i]!
      if (f && !fanDone[f - 1]) {
        fanDone[f - 1] = true
        commit(path)
        later.push(() => fan(f - 1))
      }
      const p = c.portalAt[i]!
      if (p && !portalDone[p - 1]) {
        portalDone[p - 1] = true
        commit(path)
        later.push(() => portal(p - 1))
      }
    }
  }
  // falls straight down from feet at (x, y) until something holds them up
  const fall = (path: number[], x: number, y: number) => {
    for (; y < c.h; y++) {
      if (!clear(x, y)) return
      visit(path, x, y)
      const s = support(c, el, x, y + 1)
      if (s === SAFE) {
        commit(path)
        addStand(x, y)
        return
      }
      if (s === DEADLY) return
    }
  }
  // airborne at feet (x, y), drifting up to `carry` tiles one way, landing or falling from any column on the way
  const drift = (base: number[], x: number, y: number, carry: number) => {
    for (const d of [-1, 1]) {
      const path = base.slice()
      for (let k = 1; k <= carry; k++) {
        const cx = x + d * k
        if (!clear(cx, y)) break
        visit(path, cx, y)
        const s = support(c, el, cx, y + 1)
        if (s === SAFE) {
          commit(path)
          addStand(cx, y)
          break
        }
        if (s === DEADLY) break
        fall(path.slice(), cx, y + 1)
      }
    }
  }
  const fan = (fi: number) => {
    const f = c.fans[fi]!
    for (let cx = f.x; cx < f.x + f.w; cx++) {
      const path: number[] = []
      // bottom up through the column, plus a tile of overshoot when it blows them out the top
      for (let r = f.y; r >= f.y - f.h; r--) {
        if (!clear(cx, r)) break
        visit(path, cx, r)
        commit(path)
        if (support(c, el, cx, r + 1) === SAFE) addStand(cx, r)
        drift(path, cx, r, 3)
      }
    }
  }
  const portal = (pi: number) => {
    const o = c.portals[c.portalOther[pi]!]
    if (o) fall([], o.x, o.y)
  }

  fall([], start.x, start.y)
  for (;;) {
    while (queue.length) {
      const i = queue.shift()!
      const x = i % c.w
      const y = (i - x) / c.w
      const here: number[] = []
      visit(here, x, y)
      commit(here)
      // walk off either side
      for (const d of [-1, 1]) if (clear(x + d, y)) fall([], x + d, y)
      // jump: rise a tiles, then drift sideways at that height
      const up: number[] = []
      for (let a = 1; a <= o.jump; a++) {
        if (!clear(x, y - a)) break
        visit(up, x, y - a)
        commit(up)
        drift(up, x, y - a, a <= 2 ? o.carry : a === 3 ? 2 : 1)
      }
    }
    const next = later.shift()
    if (!next) break
    next()
  }
  return { stand, touched }
}

function find(def: LevelDef, ch: string): { x: number; y: number } | undefined {
  for (let y = 0; y < def.map.length; y++) {
    const x = def.map[y]!.indexOf(ch)
    if (x >= 0) return { x, y }
  }
  return undefined
}

export function reachability(def: LevelDef, opts: ReachOpts = {}): Reach {
  const o = { jump: opts.jump ?? 3, carry: opts.carry ?? 4 }
  const c = build(def)
  const n = c.w * c.h
  const spawn = [find(def, '1'), find(def, '2')]
  const empty = () => ({ stand: new Array<boolean>(n).fill(false), touched: new Array<boolean>(n).fill(false) })
  const fire = empty()
  const frost = empty()
  const merge = (into: typeof fire, r: typeof fire) => {
    for (let i = 0; i < n; i++) {
      into.stand[i] ||= r.stand[i]!
      into.touched[i] ||= r.touched[i]!
    }
  }
  const thin: number[] = []
  for (let i = 0; i < n; i++) if (chr(c, i % c.w, Math.floor(i / c.w)) === 'w') thin.push(i)
  // fire melts what it can touch, frost freezes what it gets near; repeat until neither changes.
  // reach is the union over rounds, since the players can choose to do things in any order
  for (let round = 0; round < 50; round++) {
    let changed = false
    if (spawn[0]) merge(fire, reach(c, 'fire', spawn[0], o))
    for (let i = 0; i < n; i++) {
      if (!fire.touched[i]) continue
      const x = i % c.w
      const y = (i - x) / c.w
      for (const [dx, dy] of [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1]] as const) {
        const xx = x + dx
        const yy = y + dy
        if (xx < 0 || yy < 0 || xx >= c.w || yy >= c.h) continue
        const ice = c.iceAt[yy * c.w + xx]!
        if (ice && !c.melted[ice - 1]) {
          c.melted[ice - 1] = true
          changed = true
        }
      }
    }
    if (spawn[1]) merge(frost, reach(c, 'frost', spawn[1], o))
    for (const t of thin) {
      if (c.frozen[t]) continue
      const tx = t % c.w
      const ty = (t - tx) / c.w
      // frost's centre sits about mid-tile in its feet cell. only standing spots count: frost has to wait there
      for (let i = 0; i < n && !c.frozen[t]; i++) {
        if (!frost.stand[i]) continue
        const x = i % c.w
        const y = (i - x) / c.w
        if (Math.hypot(x - tx, y - ty) <= 2.2) {
          c.frozen[t] = true
          changed = true
        }
      }
    }
    if (!changed) break
  }
  const gems: Array<{ x: number; y: number; el: Element }> = []
  def.map.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      if (row[x] === 'r') gems.push({ x, y, el: 'fire' })
      if (row[x] === 'b') gems.push({ x, y, el: 'frost' })
    }
  })
  const got = (g: { x: number; y: number; el: Element }) => (g.el === 'fire' ? fire : frost).touched[g.y * c.w + g.x]!
  const door = (ch: string, r: typeof fire) => {
    const d = find(def, ch)
    return !!d && r.stand[d.y * c.w + d.x]!
  }
  return {
    fireDoor: door('R', fire),
    frostDoor: door('B', frost),
    fireGems: gems.filter((g) => g.el === 'fire' && got(g)).length,
    frostGems: gems.filter((g) => g.el === 'frost' && got(g)).length,
    gemTotals: [gems.filter((g) => g.el === 'fire').length, gems.filter((g) => g.el === 'frost').length],
    missedGems: gems.filter((g) => !got(g)),
    stand: [fire.stand, frost.stand],
    touched: [fire.touched, frost.touched],
    melted: c.melted.filter(Boolean).length,
    frozen: c.frozen.filter(Boolean).length,
  }
}

// the map with standing cells marked, for eyeballing a level: '+' fire stands there, '-' frost, '=' both.
// only empty tiles get marked, so the map's own characters stay readable
export function renderReach(def: LevelDef, r: Reach = reachability(def)): string {
  return def.map
    .map((row, y) =>
      [...row]
        .map((ch, x) => {
          if (ch !== '.' && ch !== ' ') return ch
          const i = y * row.length + x
          const f = r.stand[0][i]
          const b = r.stand[1][i]
          return f && b ? '=' : f ? '+' : b ? '-' : ch
        })
        .join(''),
    )
    .join('\n')
}
