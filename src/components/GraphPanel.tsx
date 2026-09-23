import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import {
  buildGraph,
  graphHome,
  graphTicks,
  withMinus,
  type CriticalPoint,
  type GraphPoint,
  type YScale,
} from '../engine/graph'
import { formatNumber } from '../engine/format'
import type { AngleMode } from '../engine/scientific'
import type { UserFunction } from '../engine/types'

const PLOT_PAD = { top: 12, right: 12, bottom: 28, left: 40 }
const SVG_W = 652
const SVG_H = 200
const MIN_DOMAIN_SPAN = 1e-6
const MAX_DOMAIN_SPAN = 1e6
/** How close (svg px) the pointer must come to a root or extremum for the readout to lock onto it. */
const SNAP_PX = 8

export type GraphPanelProps = {
  input: string
  functions?: Record<string, UserFunction>
  variables?: Record<string, number>
  ans?: number
  angleMode?: AngleMode
  /** Called when a critical/root point is clicked — formatted x value to copy. */
  onSelectX?: (xText: string) => void
}

function clampDomain(domain: [number, number]): [number, number] {
  let [lo, hi] = domain
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return [-10, 10]
  if (lo > hi) [lo, hi] = [hi, lo]
  let span = hi - lo
  if (span < MIN_DOMAIN_SPAN) {
    const mid = (lo + hi) / 2
    lo = mid - MIN_DOMAIN_SPAN / 2
    hi = mid + MIN_DOMAIN_SPAN / 2
    span = MIN_DOMAIN_SPAN
  }
  if (span > MAX_DOMAIN_SPAN) {
    const mid = (lo + hi) / 2
    lo = mid - MAX_DOMAIN_SPAN / 2
    hi = mid + MAX_DOMAIN_SPAN / 2
  }
  return [lo, hi]
}

function formatCoord(n: number): string {
  return formatNumber(n, 8)
}

/** Far-off-screen samples are pinned here; the clip path hides them either way. */
const PX_LIMIT = 1e4

/** Curve path (gaps at non-finite samples and at jumps across the whole view) plus isolated samples as dots. */
function pointsToPath(
  points: GraphPoint[],
  xToPx: (x: number) => number,
  yToPx: (y: number) => number,
  top: number,
  bottom: number,
): { d: string; dots: { x: number; y: number }[] } {
  const parts: string[] = []
  const dots: { x: number; y: number }[] = []
  let run: { x: number; y: number }[] = []
  const flush = () => {
    if (run.length === 1) dots.push(run[0]!)
    else if (run.length > 1) {
      parts.push(run.map((p, i) => `${i ? 'L' : 'M'}${p.x.toFixed(2)} ${p.y.toFixed(2)}`).join(' '))
    }
    run = []
  }
  for (const p of points) {
    const x = xToPx(p.x)
    const y = p.y == null ? Number.NaN : yToPx(p.y)
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      flush()
      continue
    }
    const last = run[run.length - 1]
    // One step from above the view to below it is a pole (tan, 1/x), not a line to draw.
    if (last && ((last.y < top && y > bottom) || (last.y > bottom && y < top))) flush()
    run.push({ x, y: Math.max(-PX_LIMIT, Math.min(PX_LIMIT, y)) })
  }
  flush()
  return { d: parts.join(' '), dots }
}

function kindLabel(kind: CriticalPoint['kind']): 'min' | 'max' | 'crit' {
  if (kind === 'min') return 'min'
  if (kind === 'max') return 'max'
  return 'crit'
}

/** `v` rounded to the finest decimal a pixel of width `res` can distinguish. */
function roundToPixel(v: number, res: number): number {
  const step = 10 ** Math.floor(Math.log10(res))
  return Number((Math.round(v / step) * step).toPrecision(12))
}

type Feature = { x: number; y: number; kind: 'min' | 'max' | 'crit' | 'root' }

export function GraphPanel({ input, functions, variables, ans, angleMode, onSelectX }: GraphPanelProps) {
  const clipId = useId()
  const home = useMemo(
    () => graphHome(input, { functions, variables, ans, angleMode }),
    [input, functions, variables, ans, angleMode],
  )
  const [domain, setDomain] = useState<[number, number]>(home)
  const [dragging, setDragging] = useState(false)
  /** Pointer x in svg units while hovering the plot (not while dragging). */
  const [hoverPx, setHoverPx] = useState<number | null>(null)
  const dragRef = useRef<{ startPx: number; domain: [number, number]; moved: boolean } | null>(null)
  const plotRef = useRef<SVGSVGElement>(null)

  // Pointer and wheel events outpace frames; resample at most once per frame.
  const domainRef = useRef(domain)
  domainRef.current = domain
  const pendingRef = useRef<[number, number] | null>(null)
  const frameRef = useRef(0)
  useEffect(() => () => cancelAnimationFrame(frameRef.current), [])
  const updateDomain = useCallback((next: (d: [number, number]) => [number, number]) => {
    pendingRef.current = clampDomain(next(pendingRef.current ?? domainRef.current))
    if (frameRef.current) return
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = 0
      const d = pendingRef.current
      pendingRef.current = null
      if (d) setDomain(d)
    })
  }, [])

  const result = useMemo(
    () => buildGraph(input, { domain, functions, variables, ans, angleMode }),
    [input, domain, functions, variables, ans, angleMode],
  )

  const yScale: YScale = result?.yScale ?? { min: -10, max: 10 }
  const plotW = SVG_W - PLOT_PAD.left - PLOT_PAD.right
  const plotH = SVG_H - PLOT_PAD.top - PLOT_PAD.bottom
  const plotBottom = PLOT_PAD.top + plotH
  const plotRight = PLOT_PAD.left + plotW
  const [xMin, xMax] = result?.domain ?? domain
  const xSpan = Math.max(MIN_DOMAIN_SPAN, xMax - xMin)
  const ySpan = Math.max(MIN_DOMAIN_SPAN, yScale.max - yScale.min)
  const angleUnit = result?.angleUnit ?? null

  const xToPx = useCallback((x: number) => PLOT_PAD.left + ((x - xMin) / xSpan) * plotW, [xMin, xSpan, plotW])
  const yToPx = useCallback(
    (y: number) => PLOT_PAD.top + ((yScale.max - y) / ySpan) * plotH,
    [yScale.max, ySpan, plotH],
  )
  const pxToX = useCallback((px: number) => xMin + ((px - PLOT_PAD.left) / plotW) * xSpan, [xMin, plotW, xSpan])

  const curve = useMemo(
    () => (result ? pointsToPath(result.points, xToPx, yToPx, PLOT_PAD.top, plotBottom) : { d: '', dots: [] }),
    [result, xToPx, yToPx, plotBottom],
  )
  const xTicks = graphTicks(xMin, xMax, angleUnit, 6)
  const yTicks = graphTicks(yScale.min, yScale.max)

  const axisY = yToPx(0)
  const axisX = xToPx(0)
  const showXAxis = axisY >= PLOT_PAD.top && axisY <= plotBottom
  const showYAxis = axisX >= PLOT_PAD.left && axisX <= plotRight

  const critical = result?.criticalPoints ?? []
  const roots = result?.roots ?? []
  const inView = (px: number, py: number) =>
    px >= PLOT_PAD.left && px <= plotRight && py >= PLOT_PAD.top && py <= plotBottom
  const features: Feature[] = [
    ...roots.map((r) => ({ x: r.x, y: 0, kind: 'root' as const })),
    ...critical.map((c) => ({ x: c.x, y: c.y, kind: kindLabel(c.kind) })),
  ].filter((p) => inView(xToPx(p.x), yToPx(p.y)))

  /** Client coordinates → svg viewBox units (exact under any letterboxing). */
  const toSvgX = useCallback((clientX: number, clientY: number) => {
    const svg = plotRef.current
    const ctm = svg?.getScreenCTM()
    if (!svg || !ctm) return null
    return new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse()).x
  }, [])

  const zoomAt = useCallback(
    (px: number, factor: number) => {
      const t = Math.min(1, Math.max(0, (px - PLOT_PAD.left) / plotW))
      updateDomain(([lo, hi]) => {
        const anchor = lo + (hi - lo) * t
        const span = (hi - lo) * factor
        return [anchor - span * t, anchor + span * (1 - t)]
      })
    },
    [plotW, updateDomain],
  )
  const panBy = useCallback(
    (dxPx: number) => updateDomain(([lo, hi]) => {
      const dx = (dxPx / plotW) * (hi - lo)
      return [lo + dx, hi + dx]
    }),
    [plotW, updateDomain],
  )

  useEffect(() => {
    if (result?.error) return
    const svg = plotRef.current
    if (!svg) return
    // Vertical scroll (and pinch, which Chromium sends as ctrl+wheel) zooms about the cursor;
    // a sideways trackpad swipe pans.
    let gesturing = false
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      e.stopPropagation()
      if (gesturing && e.ctrlKey) return
      const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1
      const px = toSvgX(e.clientX, e.clientY)
      if (px == null) return
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY) && !e.ctrlKey) {
        panBy(e.deltaX * unit)
        return
      }
      const k = e.ctrlKey ? 0.01 : 0.002
      zoomAt(px, Math.exp(Math.max(-0.5, Math.min(0.5, e.deltaY * unit * k))))
    }
    // WebKit (the app's WKWebView) reports trackpad pinch as gesture events with a running scale;
    // any ctrl+wheel it sends alongside is ignored so a pinch zooms once.
    type GestureEvent = UIEvent & { scale: number; clientX: number; clientY: number }
    let lastScale = 1
    const onGestureStart = (e: Event) => {
      e.preventDefault()
      gesturing = true
      lastScale = 1
    }
    const onGestureEnd = () => {
      gesturing = false
    }
    const onGestureChange = (e: Event) => {
      e.preventDefault()
      const g = e as GestureEvent
      const px = toSvgX(g.clientX, g.clientY)
      if (px == null || !(g.scale > 0)) return
      zoomAt(px, lastScale / g.scale)
      lastScale = g.scale
    }
    svg.addEventListener('wheel', onWheel, { passive: false })
    svg.addEventListener('gesturestart', onGestureStart)
    svg.addEventListener('gesturechange', onGestureChange)
    svg.addEventListener('gestureend', onGestureEnd)
    return () => {
      svg.removeEventListener('gestureend', onGestureEnd)
      svg.removeEventListener('wheel', onWheel)
      svg.removeEventListener('gesturestart', onGestureStart)
      svg.removeEventListener('gesturechange', onGestureChange)
    }
  }, [panBy, zoomAt, toSvgX, result?.error])

  // Readout: locks onto a nearby root or extremum, otherwise follows the curve at the pointer.
  const snapAt = (px: number) =>
    features.reduce<Feature | null>((best, p) => {
      const d = Math.abs(xToPx(p.x) - px)
      return d <= SNAP_PX && (!best || d < Math.abs(xToPx(best.x) - px)) ? p : best
    }, null)
  const snapped = hoverPx == null ? null : snapAt(hoverPx)
  let readout: { px: number; py: number | null; kind?: string; text: string } | null = null
  if (hoverPx != null && result && !result.error) {
    const deg = angleUnit === 'deg' ? '°' : ''
    if (snapped) {
      readout = {
        px: xToPx(snapped.x),
        py: yToPx(snapped.y),
        kind: snapped.kind,
        text: `(${withMinus(formatCoord(snapped.x))}${deg}, ${withMinus(formatCoord(snapped.y))})`,
      }
    } else {
      const x = roundToPixel(pxToX(hoverPx), xSpan / plotW)
      const y = result.y(x)
      const py = y == null ? null : yToPx(y)
      readout = {
        px: xToPx(x),
        py: py != null && py >= PLOT_PAD.top && py <= plotBottom ? py : null,
        text: `(${withMinus(formatNumber(x, 8))}${deg}, ${y == null ? '—' : withMinus(formatNumber(y, 6))})`,
      }
    }
  }
  const readoutRight = readout != null && readout.px > PLOT_PAD.left + plotW * 0.62

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<SVGSVGElement>) => {
      if (e.button !== 0) return
      const px = toSvgX(e.clientX, e.clientY)
      if (px == null) return
      e.currentTarget.setPointerCapture(e.pointerId)
      dragRef.current = { startPx: px, domain: [...domainRef.current] as [number, number], moved: false }
    },
    [toSvgX],
  )

  const onPointerMove = useCallback(
    (e: ReactPointerEvent<SVGSVGElement>) => {
      const px = toSvgX(e.clientX, e.clientY)
      if (px == null) return
      const drag = dragRef.current
      if (!drag) {
        setHoverPx(px >= PLOT_PAD.left && px <= plotRight ? px : null)
        return
      }
      const dxPx = px - drag.startPx
      if (!drag.moved && Math.abs(dxPx) < 3) return
      if (!drag.moved) {
        drag.moved = true
        setDragging(true)
        setHoverPx(null)
      }
      const [lo, hi] = drag.domain
      const dx = -(dxPx / plotW) * (hi - lo)
      updateDomain(() => [lo + dx, hi + dx])
    },
    [plotW, plotRight, toSvgX, updateDomain],
  )

  const selectX = useCallback((x: number) => onSelectX?.(formatCoord(x)), [onSelectX])

  const endDrag = (e: ReactPointerEvent<SVGSVGElement>) => {
    try {
      e.currentTarget.releasePointerCapture(e.pointerId)
    } catch {
      /* already released */
    }
    const drag = dragRef.current
    dragRef.current = null
    setDragging(false)
    // A click (no drag) on a root or extremum copies its x.
    const px = toSvgX(e.clientX, e.clientY)
    const hit = drag && !drag.moved && e.type === 'pointerup' && px != null ? snapAt(px) : null
    if (hit) selectX(hit.x)
    if (px != null) setHoverPx(px >= PLOT_PAD.left && px <= plotRight ? px : null)
  }

  const resetView = useCallback(() => setDomain(home), [home])

  const label = result?.intent.label ?? 'graph'
  const error = result?.error
  const atHome = Math.abs(domain[0] - home[0]) < 1e-9 && Math.abs(domain[1] - home[1]) < 1e-9

  return (
    <div className="graph" role="region" aria-label={`Graph of ${label}`}>
      <div className="graph-head">
        <span className="graph-label">{label}</span>
        <span className="graph-domain">
          [{domain.map((v) => withMinus(formatCoord(roundToPixel(v, (domain[1] - domain[0]) / plotW)))).join(', ')}]
        </span>
        {angleUnit ? <span className="graph-domain">{angleUnit}</span> : null}
        <button
          type="button"
          className="graph-reset"
          title="Reset view"
          disabled={atHome}
          onMouseDown={(e) => e.preventDefault()}
          onClick={resetView}
        >
          reset
        </button>
      </div>

      {error ? (
        <div className="graph-error">{error}</div>
      ) : (
        <svg
          ref={plotRef}
          className={`graph-plot${dragging ? ' dragging' : ''}${snapped ? ' snapped' : ''}`}
          viewBox={`0 0 ${SVG_W} ${SVG_H}`}
          width="100%"
          height={SVG_H}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onPointerLeave={() => setHoverPx(null)}
        >
          <rect className="graph-plot-bg" x={PLOT_PAD.left} y={PLOT_PAD.top} width={plotW} height={plotH} rx={4} />
          <clipPath id={clipId}>
            <rect x={PLOT_PAD.left} y={PLOT_PAD.top} width={plotW} height={plotH} />
          </clipPath>
          <g className="graph-grid">
            {xTicks.map((t) => (
              <line key={`gx${t.value}`} x1={xToPx(t.value)} y1={PLOT_PAD.top} x2={xToPx(t.value)} y2={plotBottom} />
            ))}
            {yTicks.map((t) => (
              <line key={`gy${t.value}`} x1={PLOT_PAD.left} y1={yToPx(t.value)} x2={plotRight} y2={yToPx(t.value)} />
            ))}
          </g>
          {showXAxis ? <line className="graph-axis" x1={PLOT_PAD.left} y1={axisY} x2={plotRight} y2={axisY} /> : null}
          {showYAxis ? <line className="graph-axis" x1={axisX} y1={PLOT_PAD.top} x2={axisX} y2={plotBottom} /> : null}
          {xTicks.map((t) => (
            <text key={`x${t.value}`} className="graph-tick" x={xToPx(t.value)} y={SVG_H - 12} textAnchor="middle">
              {t.label}
            </text>
          ))}
          {yTicks.map((t) => (
            <text
              key={`y${t.value}`}
              className="graph-tick"
              x={PLOT_PAD.left - 6}
              y={yToPx(t.value)}
              textAnchor="end"
              dominantBaseline="middle"
            >
              {t.label}
            </text>
          ))}
          <g clipPath={`url(#${clipId})`}>
            {readout ? (
              <line className="graph-hover-line" x1={readout.px} y1={PLOT_PAD.top} x2={readout.px} y2={plotBottom} />
            ) : null}
            {curve.d ? <path className="graph-curve" d={curve.d} fill="none" /> : null}
            {curve.dots.map((p) => (
              <circle key={`${p.x}`} className="graph-dot" cx={p.x} cy={p.y} r={2} />
            ))}
            {features.map((p) => (
              <circle key={`${p.kind}${p.x}`} className="graph-mark" cx={xToPx(p.x)} cy={yToPx(p.y)} r={2.75} />
            ))}
            {readout?.py != null ? (
              <circle className="graph-hover-dot" cx={readout.px} cy={readout.py} r={3.5} />
            ) : null}
          </g>
          {readout ? (
            <text
              className="graph-readout"
              x={readout.px + (readoutRight ? -7 : 7)}
              y={PLOT_PAD.top + 13}
              textAnchor={readoutRight ? 'end' : 'start'}
            >
              {readout.kind ? <tspan className="graph-readout-kind">{readout.kind} </tspan> : null}
              {readout.text}
            </text>
          ) : null}
        </svg>
      )}

      <div className="graph-detail">
        <div className="graph-detail-label">Critical points</div>
        {critical.length === 0 && !error ? (
          <div className="graph-detail-empty">None in this domain</div>
        ) : (
          <ul className="graph-detail-list">
            {critical.map((p, i) => (
              <li key={`${p.kind}-${i}`}>
                <button
                  type="button"
                  className="graph-detail-item"
                  title="Copy x"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => selectX(p.x)}
                >
                  <span className="graph-kind">{kindLabel(p.kind)}</span>
                  <span className="graph-coords">
                    ({withMinus(formatCoord(p.x))}, {withMinus(formatCoord(p.y))})
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
        {roots.length > 0 ? (
          <>
            <div className="graph-detail-label">Roots</div>
            <ul className="graph-detail-list">
              {roots.map((r, i) => (
                <li key={`r-${i}`}>
                  <button
                    type="button"
                    className="graph-detail-item"
                    title="Copy x"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => selectX(r.x)}
                  >
                    <span className="graph-kind">root</span>
                    <span className="graph-coords">x = {withMinus(formatCoord(r.x))}</span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        ) : null}
      </div>
    </div>
  )
}
