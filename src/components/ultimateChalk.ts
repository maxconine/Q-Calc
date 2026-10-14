// the chalk look for ultimate tic-tac-toe: every line, x and o as an svg path with a hand-drawn wobble.
// the wobble is baked into the path from a seed, so it costs nothing to draw (no svg filters) and a mark keeps
// its shape from one redraw to the next. paths are made once and cached

type Pt = [number, number]

// a small seeded random (mulberry32), so the same seed always wobbles the same way
function seeded(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const f = (n: number) => Math.round(n * 10) / 10

// a stroke from a to b that bows a little either side and overshoots its ends, the way chalk does
function wobble(a: Pt, b: Pt, amp: number, r: () => number): string {
  const dx = b[0] - a[0]
  const dy = b[1] - a[1]
  const len = Math.hypot(dx, dy) || 1
  const [ux, uy] = [dx / len, dy / len]
  const [nx, ny] = [-uy, ux]
  const jit = () => (r() - 0.5) * 2 * amp
  const s: Pt = [a[0] - ux * r() * amp + nx * jit() * 0.5, a[1] - uy * r() * amp + ny * jit() * 0.5]
  const e: Pt = [b[0] + ux * r() * amp + nx * jit() * 0.5, b[1] + uy * r() * amp + ny * jit() * 0.5]
  const o1 = jit()
  const o2 = jit()
  const c1: Pt = [a[0] + dx / 3 + nx * o1, a[1] + dy / 3 + ny * o1]
  const c2: Pt = [a[0] + (2 * dx) / 3 + nx * o2, a[1] + (2 * dy) / 3 + ny * o2]
  return `M${f(s[0])} ${f(s[1])}C${f(c1[0])} ${f(c1[1])} ${f(c2[0])} ${f(c2[1])} ${f(e[0])} ${f(e[1])}`
}

// a loop around (cx, cy) that isn't quite round and runs a little past where it started
function loop(cx: number, cy: number, radius: number, r: () => number): string {
  const n = 14
  const start = r() * Math.PI * 2
  const sweep = Math.PI * 2 * (1.06 + r() * 0.06)
  const squash = 0.92 + r() * 0.1
  const pts: Pt[] = []
  for (let i = 0; i <= n; i++) {
    const t = start + (sweep * i) / n
    const rad = radius * (0.94 + r() * 0.1) * (1 - (i / n) * 0.05)
    pts.push([cx + Math.cos(t) * rad, cy + Math.sin(t) * rad * squash])
  }
  // catmull-rom through the points, as cubic beziers
  let d = `M${f(pts[0]![0])} ${f(pts[0]![1])}`
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)]!
    const p1 = pts[i]!
    const p2 = pts[i + 1]!
    const p3 = pts[Math.min(pts.length - 1, i + 2)]!
    const c1: Pt = [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6]
    const c2: Pt = [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6]
    d += `C${f(c1[0])} ${f(c1[1])} ${f(c2[0])} ${f(c2[1])} ${f(p2[0])} ${f(p2[1])}`
  }
  return d
}

const cache = new Map<string, string>()
function cached(key: string, make: () => string): string {
  let d = cache.get(key)
  if (d == null) {
    d = make()
    cache.set(key, d)
  }
  return d
}

// an x or an o filling the square at (x, y) of side size; seed picks its wobble
export function chalkMark(kind: 'x' | 'o', x: number, y: number, size: number, seed: number): string {
  return cached(`${kind}${x},${y},${size},${seed}`, () => {
    const r = seeded(seed * 7919 + (kind === 'x' ? 1 : 2))
    const pad = size * 0.25
    const amp = size * 0.045
    if (kind === 'o') return loop(x + size / 2, y + size / 2, size / 2 - pad, r)
    return wobble([x + pad, y + pad], [x + size - pad, y + size - pad], amp, r) + wobble([x + size - pad, y + pad], [x + pad, y + size - pad], amp, r)
  })
}

// a chalk line from a to b
export function chalkLine(a: Pt, b: Pt, seed: number, amp = 1.1): string {
  return cached(`l${a},${b},${seed},${amp}`, () => wobble(a, b, amp, seeded(seed)))
}

// a rough box around (x, y, w, h): four strokes that don't quite meet
export function chalkBox(x: number, y: number, w: number, h: number, seed: number): string {
  return cached(`b${x},${y},${w},${h},${seed}`, () => {
    const r = seeded(seed)
    const a = 1.2
    return (
      wobble([x, y], [x + w, y], a, r) +
      wobble([x + w, y], [x + w, y + h], a, r) +
      wobble([x + w, y + h], [x, y + h], a, r) +
      wobble([x, y + h], [x, y], a, r)
    )
  })
}
