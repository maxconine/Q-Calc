import { useLayoutEffect, useRef, type ReactNode, type RefObject } from 'react'
import type { Span } from '../lib/blankReason'
import { boundsIn, isHidden, snapOffset, type Bound, type Slot } from '../lib/bounds'
import { RadicalText } from './Radical'

// a run of text drawn at its own size; its offsets are the text's own, so the caret can find them
function run(text: string, from: number, to: number, cls: string | undefined, mark: Span | null, key: string): ReactNode {
  const pieces: ReactNode[] = []
  const a = mark ? Math.max(from, Math.min(to, mark.start)) : to
  const b = mark ? Math.max(a, Math.min(to, mark.end)) : to
  if (a > from) pieces.push(<RadicalText key="a" text={text.slice(from, a)} />)
  if (b > a) {
    pieces.push(
      <span key="m" className="quick-squiggle">
        <RadicalText text={text.slice(a, b)} />
      </span>,
    )
  }
  if (to > b) pieces.push(<RadicalText key="c" text={text.slice(b, to)} />)
  return (
    <span key={key} className={cls} data-from={from} data-to={to}>
      {from === to && cls ? <span className="bounds-hole" /> : pieces}
    </span>
  )
}

function limit(text: string, slot: Slot | null, where: 'upper' | 'lower', mark: Span | null): ReactNode {
  if (!slot) {
    return (
      <span className={`bounds-${where}`}>
        <span className="bounds-hole" />
      </span>
    )
  }
  return run(text, slot.start, slot.end, `bounds-${where}`, mark, where)
}

function boundNodes(text: string, bounds: Bound[], mark: Span | null): ReactNode[] {
  const out: ReactNode[] = []
  let i = 0
  for (const b of bounds) {
    if (b.sign > i) out.push(run(text, i, b.sign, undefined, mark, `t${i}`))
    out.push(
      <span className={text[b.sign] === '∫' ? 'bounds' : 'bounds bounds-upright'} key={`b${b.sign}`}>
        {run(text, b.sign, b.sign + 1, undefined, null, 's')}
        <span className="bounds-limits">
          {limit(text, b.upper, 'upper', mark)}
          {limit(text, b.lower, 'lower', mark)}
        </span>
      </span>,
    )
    i = b.end
  }
  if (i < text.length || !bounds.length) out.push(run(text, i, text.length, undefined, mark, `t${i}`))
  return out
}

/** An expression with its ∫ and Σ limits drawn small beside the sign; the text itself is unchanged. */
export function MathText({ text }: { text: string }) {
  const bounds = boundsIn(text)
  if (!bounds.length) return <RadicalText text={text} />
  return <span className="bounds-text">{boundNodes(text, bounds, null)}</span>
}

type Point = { x: number; top: number; bottom: number }

// text nodes in order, leaving out the √ glyph copies drawn for the bars
function textNodes(el: Element): Text[] {
  const out: Text[] = []
  const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest('.radical-bar') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  })
  for (let n = walk.nextNode(); n; n = walk.nextNode()) out.push(n as Text)
  return out
}

/** Where a caret at each offset sits, in client coordinates. Slots come before the text around them. */
function caretPoints(root: HTMLElement, text: string): Map<number, Point> {
  const bounds = boundsIn(text)
  const points = new Map<number, Point>()
  const range = document.createRange()
  const runs = [...root.querySelectorAll<HTMLElement>('[data-from]')]
  // limits first, so an offset shared with the text around them lands in the limit
  runs.sort((a, b) => Number(!!b.className) - Number(!!a.className))
  for (const el of runs) {
    const from = Number(el.dataset.from)
    const to = Number(el.dataset.to)
    const box = el.getBoundingClientRect()
    if (from === to) {
      const hole = el.firstElementChild?.getBoundingClientRect() ?? box
      if (!points.has(from)) points.set(from, { x: hole.left + hole.width / 2, top: hole.top, bottom: hole.bottom })
      continue
    }
    const chars: [Text, number][] = []
    for (const node of textNodes(el)) for (let k = 0; k < node.length; k++) chars.push([node, k])
    const edge = (c: number, side: 'left' | 'right') => {
      const hit = chars[c - from]
      if (!hit) return null
      range.setStart(hit[0], hit[1])
      range.setEnd(hit[0], hit[1] + 1)
      const r = range.getBoundingClientRect()
      return side === 'left' ? r.left : r.right
    }
    for (let o = from; o <= to; o++) {
      if (points.has(o) || isHidden(bounds, o)) continue
      const x = o < to ? edge(o, 'left') : edge(o - 1, 'right')
      if (x != null) points.set(o, { x, top: box.top, bottom: box.bottom })
    }
  }
  return points
}

function nearest(points: Map<number, Point>, x: number, y: number): number | null {
  let best: number | null = null
  let score = Infinity
  for (const [o, p] of points) {
    const dy = y < p.top ? p.top - y : y > p.bottom ? y - p.bottom : 0
    const s = Math.abs(x - p.x) + 3 * dy
    if (s < score) {
      score = s
      best = o
    }
  }
  return best
}

const wordAround = (text: string, o: number): [number, number] => {
  let a = o
  let b = o
  while (a > 0 && /\w/.test(text[a - 1]!)) a--
  while (b < text.length && /\w/.test(text[b]!)) b++
  return a === b ? [o, Math.min(text.length, o + 1)] : [a, b]
}

/**
 * The input's text drawn with small limits. The real input keeps the text, the keys and the selection,
 * but its glyphs can't change size, so it's invisible here and this layer draws the caret, the
 * selection and the scrolling from its own measured positions, and maps clicks back to offsets.
 */
export function BoundsInputText({
  value,
  mark,
  input,
}: {
  value: string
  mark: Span | null
  input: RefObject<HTMLInputElement | null>
}) {
  const ref = useRef<HTMLSpanElement>(null)
  const caretRef = useRef<HTMLSpanElement>(null)
  const scrollRef = useRef(0)
  const lastRef = useRef('')
  const bounds = boundsIn(value)

  const draw = () => {
    const el = input.current
    const root = ref.current
    const caret = caretRef.current
    const ghost = root?.parentElement
    if (!el || !root || !caret || !ghost) return
    const points = caretPoints(root, el.value)
    const box = root.getBoundingClientRect()
    const scale = box.width / (root.offsetWidth || 1) || 1
    const local = (x: number) => (x - box.left) / scale
    const top = (y: number) => (y - box.top) / scale
    const start = el.selectionStart ?? 0
    const end = el.selectionEnd ?? start
    const p = points.get(el.selectionDirection === 'backward' ? start : end)
    // keep the caret in view the way the input would
    if (p) {
      let s = scrollRef.current
      const view = (ghost.getBoundingClientRect().right - box.left) / scale - s
      const x = local(p.x)
      if (x > s + view - 3) s = x - view + 3
      if (x < s) s = Math.max(0, x - 24)
      s = Math.max(0, Math.min(s, root.offsetWidth - view + 3))
      if (s !== scrollRef.current) {
        scrollRef.current = s
        const shift = s ? `translateX(${-s}px)` : ''
        for (let n: Element | null = root; n; n = n.nextElementSibling) (n as HTMLElement).style.transform = shift
        // text scrolled off the left goes under the edge, not over the `ans` before it
        root.style.clipPath = s ? `inset(-1em -100vw -1em ${s}px)` : ''
      }
    }
    for (const old of root.querySelectorAll('.bounds-selection')) old.remove()
    const focused = document.activeElement === el
    if (start !== end) {
      caret.style.display = 'none'
      lastRef.current = ''
      const rows = new Map<number, { from: Point; to: Point }>()
      for (let o = start; o <= end; o++) {
        const q = points.get(o)
        if (!q) continue
        const row = rows.get(q.top)
        if (!row) rows.set(q.top, { from: q, to: q })
        else if (q.x > row.to.x) row.to = q
        else if (q.x < row.from.x) row.from = q
      }
      for (const { from, to } of rows.values()) {
        if (to.x - from.x < 0.5) continue
        const sel = document.createElement('span')
        sel.className = focused ? 'bounds-selection' : 'bounds-selection bounds-selection-blur'
        sel.style.left = `${local(from.x)}px`
        sel.style.width = `${(to.x - from.x) / scale}px`
        sel.style.top = `${top(from.top)}px`
        sel.style.height = `${(from.bottom - from.top) / scale}px`
        root.insertBefore(sel, root.firstChild)
      }
      return
    }
    const q = points.get(start)
    if (!q || !focused) {
      caret.style.display = 'none'
      lastRef.current = ''
      return
    }
    caret.style.display = ''
    caret.style.left = `${local(q.x)}px`
    caret.style.top = `${top(q.top)}px`
    caret.style.height = `${(q.bottom - q.top) / scale}px`
    // the blink restarts whenever the caret moves, like the real one
    const key = `${start}:${el.value}`
    if (key !== lastRef.current) {
      lastRef.current = key
      caret.style.animation = 'none'
      void caret.offsetWidth
      caret.style.animation = ''
    }
  }
  const drawRef = useRef(draw)
  drawRef.current = draw

  useLayoutEffect(() => draw())

  useLayoutEffect(() => {
    const el = input.current
    const root = ref.current
    if (!el || !root) return
    let frame = 0
    const later = () => {
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0
          drawRef.current()
        })
      }
    }
    let anchor = 0
    const hit = (e: MouseEvent) => nearest(caretPoints(root, el.value), e.clientX, e.clientY)
    const select = (a: number, b: number) => {
      if (b < a) el.setSelectionRange(b, a, 'backward')
      else el.setSelectionRange(a, b, 'forward')
      later()
    }
    const onMove = (e: MouseEvent) => {
      const o = hit(e)
      if (o != null) select(anchor, o)
    }
    const onUp = () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      later()
    }
    // the input would place the caret by its own full-size glyphs, so clicks are placed here instead
    const onDown = (e: MouseEvent) => {
      if (e.button !== 0) return
      const o = hit(e)
      if (o == null) return
      e.preventDefault()
      el.focus()
      if (e.detail >= 3) return select(0, el.value.length)
      if (e.detail === 2) {
        const [a, b] = wordAround(el.value, o)
        return select(a, b)
      }
      const start = el.selectionStart ?? 0
      const end = el.selectionEnd ?? start
      anchor = e.shiftKey ? (el.selectionDirection === 'backward' ? end : start) : o
      select(anchor, o)
      window.addEventListener('mousemove', onMove)
      window.addEventListener('mouseup', onUp)
    }
    // a caret among the hidden `_` and `^` steps on past them, the way it was going
    let last = el.selectionStart ?? 0
    const snap = () => {
      const at = el.selectionStart ?? 0
      if (at !== el.selectionEnd) return
      const bounds = boundsIn(el.value)
      if (bounds.length && isHidden(bounds, at)) {
        last = snapOffset(bounds, at, at < last ? -1 : 1, el.value.length)
        el.setSelectionRange(last, last)
      } else last = at
      later()
    }
    const events = ['select', 'input', 'keydown', 'keyup', 'focus', 'blur']
    for (const ev of events) el.addEventListener(ev, later)
    for (const ev of ['select', 'keyup', 'mouseup']) el.addEventListener(ev, snap)
    document.addEventListener('selectionchange', snap)
    el.addEventListener('mousedown', onDown)
    return () => {
      if (frame) cancelAnimationFrame(frame)
      for (const ev of events) el.removeEventListener(ev, later)
      for (const ev of ['select', 'keyup', 'mouseup']) el.removeEventListener(ev, snap)
      document.removeEventListener('selectionchange', snap)
      el.removeEventListener('mousedown', onDown)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
      // leaving the limits behind puts the text back where the input scrolls it
      for (let n: Element | null = root; n; n = n.nextElementSibling) (n as HTMLElement).style.transform = ''
    }
  }, [input])

  return (
    <span ref={ref} className="quick-ghost-text bounds-text bounds-input">
      {boundNodes(value, bounds, mark)}
      <span ref={caretRef} className="bounds-caret" />
    </span>
  )
}
