import { useLayoutEffect, useRef, type ReactNode, type RefObject } from 'react'
import { radicalAnswer, radicalSpans, type RadicalSpan } from '../lib/radical'

// the vinculum is a slice of the √ glyph's own bar stretched over the radicand, so its height,
// thickness and antialiasing match the glyph exactly. the bar overhangs the glyph's advance, and the
// slice is taken from that overhang (in em past the advance), where the glyph is nothing but bar
const SLICE_FROM = 0.01
const SLICE_TO = 0.05

function textWidth(node: Node): number {
  const range = document.createRange()
  range.selectNodeContents(node)
  return range.getBoundingClientRect().width
}

// where an ellipsis cuts the text off, or null while it all fits
function ellipsisEdge(root: HTMLElement): number | null {
  for (let n = root.parentElement; n; n = n.parentElement) {
    const style = getComputedStyle(n)
    if (style.textOverflow !== 'ellipsis') continue
    if (n.scrollWidth <= n.clientWidth) return null
    const box = n.getBoundingClientRect()
    return box.right - parseFloat(style.paddingRight) - parseFloat(style.fontSize)
  }
  return null
}

function fitRadicals(root: HTMLElement | null) {
  if (!root) return
  // a bar is drawn, not text, so the ellipsis doesn't hide it with the radicand it covers
  const edge = ellipsisEdge(root)
  for (const el of root.querySelectorAll<HTMLElement>('.radical')) {
    const glyph = el.firstElementChild?.firstElementChild as HTMLElement | null | undefined
    const sign = el.childNodes[1]
    if (!glyph || !sign) continue
    const style = getComputedStyle(el)
    const size = parseFloat(style.fontSize)
    // the sign's advance without its tracking: the copy has none, so its ink lines up with the sign's
    const advance = textWidth(sign) - (parseFloat(style.letterSpacing) || 0)
    // the span's own box: its transformed bar doesn't widen it
    const rect = el.getBoundingClientRect()
    const total = rect.width
    const a = advance + SLICE_FROM * size
    const b = advance + SLICE_TO * size
    if (!(size > 0) || total <= b || (edge != null && rect.right > edge)) {
      glyph.style.visibility = 'hidden'
      continue
    }
    const scale = (total - a) / (b - a)
    glyph.style.visibility = 'visible'
    glyph.style.clipPath = `polygon(${a}px -50%, ${b}px -50%, ${b}px 150%, ${a}px 150%)`
    glyph.style.transform = `translateX(${a - scale * a}px) scaleX(${scale})`
  }
}

function build(text: string, spans: RadicalSpan[], from: number, to: number): ReactNode[] {
  const out: ReactNode[] = []
  let i = from
  for (const span of spans) {
    if (span.sign < i || span.sign >= to) continue
    if (span.sign > i) out.push(text.slice(i, span.sign))
    const end = Math.min(span.end, to)
    const sign = text[span.sign]
    const inner = spans.filter((s) => s.sign > span.sign && s.sign < end)
    out.push(
      <span className="radical" key={span.sign}>
        <span className="radical-bar" aria-hidden>
          <span className="radical-glyph">{sign}</span>
        </span>
        {sign}
        {build(text, inner, span.sign + 1, end)}
      </span>,
    )
    i = end
  }
  if (i < to) out.push(text.slice(i, to))
  return out
}

/** `text` with a bar over each radicand; the text itself is unchanged. */
function radicalNodes(text: string): ReactNode {
  const spans = radicalSpans(text)
  return spans.length ? build(text, spans, 0, text.length) : text
}

function useFitRadicals(ref: RefObject<HTMLElement | null>, text: string) {
  useLayoutEffect(() => {
    fitRadicals(ref.current)
  })
  useLayoutEffect(() => {
    const el = ref.current
    if (!el || !/[√∛]/.test(text) || typeof ResizeObserver === 'undefined') return
    // the answer's font steps down after this renders; the glyphs resize with it
    const ro = new ResizeObserver(() => fitRadicals(el))
    for (const g of el.querySelectorAll('.radical-glyph')) ro.observe(g)
    return () => ro.disconnect()
  }, [ref, text])
}

/** The input's bars, laid over its text and scrolled with it. */
export function RadicalLayer({ value, input, inset }: { value: string; input: RefObject<HTMLInputElement | null>; inset: number }) {
  const ref = useRef<HTMLSpanElement>(null)
  const barred = /[√∛]/.test(value)
  useFitRadicals(ref, value)
  useLayoutEffect(() => {
    const el = input.current
    if (!el || !barred) return
    const sync = () => {
      if (ref.current) ref.current.style.transform = el.scrollLeft ? `translateX(${-el.scrollLeft}px)` : ''
    }
    // the caret scrolls the input after the event that moved it
    const later = () => requestAnimationFrame(sync)
    sync()
    later()
    const events = ['scroll', 'input', 'keydown', 'keyup', 'select', 'mousedown', 'mousemove', 'mouseup', 'focus', 'blur']
    for (const e of events) el.addEventListener(e, later)
    return () => {
      for (const e of events) el.removeEventListener(e, later)
    }
  })
  if (!barred) return null
  return (
    <div className="quick-ghost quick-radicals" aria-hidden style={inset ? { left: inset } : undefined}>
      <span ref={ref} className="quick-ghost-text">
        {radicalNodes(value)}
      </span>
    </div>
  )
}

/** An expression or answer with its radicals barred; `answer` also shows `sqrt(3)` as `√3`. */
export function RadicalText({ text, answer = false }: { text: string; answer?: boolean }) {
  const shown = answer ? radicalAnswer(text) : text
  const ref = useRef<HTMLSpanElement>(null)
  useFitRadicals(ref, shown)
  if (!/[√∛]/.test(shown)) return <>{shown}</>
  return <span ref={ref}>{radicalNodes(shown)}</span>
}
