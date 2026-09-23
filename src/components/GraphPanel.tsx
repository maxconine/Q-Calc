import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import {
  DEFAULT_GRAPH_DOMAIN,
  buildGraph,
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

export type GraphPanelProps = {
  input: string
  functions?: Record<string, UserFunction>
  variables?: Record<string, number>
  angleMode?: AngleMode
  ans?: number
  /** Called when a critical/root point is clicked — formatted x value. */
  onSelectX?: (xText: string) => void
}

function clampDomain(domain: [number, number]): [number, number] {
  let [lo, hi] = domain
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) {
    return [DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]]
  }
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

function pointsToPath(
  points: GraphPoint[],
  xToPx: (x: number) => number,
  yToPx: (y: number) => number,
): string {
  const parts: string[] = []
  let drawing = false
  for (const p of points) {
    if (p.y == null || !Number.isFinite(p.y)) {
      drawing = false
      continue
    }
    const x = xToPx(p.x)
    const y = yToPx(p.y)
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      drawing = false
      continue
    }
    if (!drawing) {
      parts.push(`M${x.toFixed(2)} ${y.toFixed(2)}`)
      drawing = true
    } else {
      parts.push(`L${x.toFixed(2)} ${y.toFixed(2)}`)
    }
  }
  return parts.join(' ')
}

function kindLabel(kind: CriticalPoint['kind']): string {
  if (kind === 'min') return 'min'
  if (kind === 'max') return 'max'
  return 'crit'
}

export function GraphPanel({
  input,
  functions,
  variables,
  angleMode,
  ans,
  onSelectX,
}: GraphPanelProps) {
  const [domain, setDomain] = useState<[number, number]>([
    DEFAULT_GRAPH_DOMAIN[0],
    DEFAULT_GRAPH_DOMAIN[1],
  ])
  const [dragging, setDragging] = useState(false)
  const dragRef = useRef<{ startX: number; domain: [number, number] } | null>(null)
  // Pointer moves outpace frames; resample at most once per frame.
  const dragFrameRef = useRef(0)
  const dragDomainRef = useRef<[number, number] | null>(null)
  useEffect(() => () => cancelAnimationFrame(dragFrameRef.current), [])
  const plotRef = useRef<SVGSVGElement>(null)

  // Reset domain when the graph command identity changes (not on every keystroke of unrelated edits).
  const intentKey = useMemo(() => input.trim().toLowerCase(), [input])
  useEffect(() => {
    setDomain([DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]])
  }, [intentKey])

  const result = useMemo(
    () =>
      buildGraph(input, {
        domain,
        functions,
        variables,
        angleMode,
        ans,
      }),
    [input, domain, functions, variables, angleMode, ans],
  )

  const yScale: YScale = result?.yScale ?? { min: -10, max: 10 }
  const plotW = SVG_W - PLOT_PAD.left - PLOT_PAD.right
  const plotH = SVG_H - PLOT_PAD.top - PLOT_PAD.bottom
  const [xMin, xMax] = result?.domain ?? domain
  const xSpan = Math.max(MIN_DOMAIN_SPAN, xMax - xMin)
  const ySpan = Math.max(MIN_DOMAIN_SPAN, yScale.max - yScale.min)

  const xToPx = useCallback(
    (x: number) => PLOT_PAD.left + ((x - xMin) / xSpan) * plotW,
    [xMin, xSpan, plotW],
  )
  const yToPx = useCallback(
    (y: number) => PLOT_PAD.top + ((yScale.max - y) / ySpan) * plotH,
    [yScale.max, ySpan, plotH],
  )
  const pxToX = useCallback(
    (px: number) => xMin + ((px - PLOT_PAD.left) / plotW) * xSpan,
    [xMin, plotW, xSpan],
  )

  const pathD = useMemo(
    () => (result ? pointsToPath(result.points, xToPx, yToPx) : ''),
    [result, xToPx, yToPx],
  )

  const axisY = yToPx(0)
  const axisX = xToPx(0)
  const showXAxis = axisY >= PLOT_PAD.top && axisY <= PLOT_PAD.top + plotH
  const showYAxis = axisX >= PLOT_PAD.left && axisX <= PLOT_PAD.left + plotW

  const zoomAt = useCallback(
    (clientX: number, factor: number) => {
      const svg = plotRef.current
      if (!svg) return
      const rect = svg.getBoundingClientRect()
      const px = ((clientX - rect.left) / rect.width) * SVG_W
      const anchor = pxToX(px)
      setDomain((prev) => {
        const [lo, hi] = clampDomain(prev)
        const span = hi - lo
        const nextSpan = span * factor
        const t = span === 0 ? 0.5 : (anchor - lo) / span
        return clampDomain([anchor - nextSpan * t, anchor + nextSpan * (1 - t)])
      })
    },
    [pxToX],
  )

  const onWheel = useCallback(
    (e: WheelEvent) => {
      e.preventDefault()
      e.stopPropagation()
      const factor = e.deltaY > 0 ? 1.12 : 1 / 1.12
      zoomAt(e.clientX, factor)
    },
    [zoomAt],
  )

  useEffect(() => {
    if (result?.error) return
    const svg = plotRef.current
    if (!svg) return
    svg.addEventListener('wheel', onWheel, { passive: false })
    return () => svg.removeEventListener('wheel', onWheel)
  }, [onWheel, result?.error])

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<SVGSVGElement>) => {
      if (e.button !== 0) return
      e.currentTarget.setPointerCapture(e.pointerId)
      dragRef.current = { startX: e.clientX, domain: [...domain] as [number, number] }
      setDragging(true)
    },
    [domain],
  )

  const onPointerMove = useCallback(
    (e: ReactPointerEvent<SVGSVGElement>) => {
      const drag = dragRef.current
      const svg = plotRef.current
      if (!drag || !svg) return
      const rect = svg.getBoundingClientRect()
      const dxPx = ((e.clientX - drag.startX) / rect.width) * SVG_W
      const [lo, hi] = drag.domain
      const span = hi - lo
      const dx = -(dxPx / plotW) * span
      dragDomainRef.current = clampDomain([lo + dx, hi + dx])
      if (dragFrameRef.current) return
      dragFrameRef.current = requestAnimationFrame(() => {
        dragFrameRef.current = 0
        if (dragDomainRef.current) setDomain(dragDomainRef.current)
      })
    },
    [plotW],
  )

  const endDrag = useCallback((e: ReactPointerEvent<SVGSVGElement>) => {
    try {
      e.currentTarget.releasePointerCapture(e.pointerId)
    } catch {
      /* already released */
    }
    dragRef.current = null
    setDragging(false)
  }, [])

  const resetView = useCallback(() => {
    setDomain([DEFAULT_GRAPH_DOMAIN[0], DEFAULT_GRAPH_DOMAIN[1]])
  }, [])

  const selectX = useCallback(
    (x: number) => {
      const text = formatCoord(x)
      onSelectX?.(text)
    },
    [onSelectX],
  )

  const label = result?.intent.label ?? 'graph'
  const error = result?.error
  const critical = result?.criticalPoints ?? []
  const roots = result?.roots ?? []
  const atDefault =
    Math.abs(domain[0] - DEFAULT_GRAPH_DOMAIN[0]) < 1e-9 &&
    Math.abs(domain[1] - DEFAULT_GRAPH_DOMAIN[1]) < 1e-9

  return (
    <div className="graph" role="region" aria-label={`Graph of ${label}`}>
      <div className="graph-head">
        <span className="graph-label">{label}</span>
        <span className="graph-domain">
          [{formatCoord(domain[0])}, {formatCoord(domain[1])}]
        </span>
        <button
          type="button"
          className="graph-reset"
          title="Reset zoom to [-10, 10]"
          disabled={atDefault}
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
          className={`graph-plot ${dragging ? 'dragging' : ''}`}
          viewBox={`0 0 ${SVG_W} ${SVG_H}`}
          width="100%"
          height={SVG_H}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
        >
          <rect
            className="graph-plot-bg"
            x={PLOT_PAD.left}
            y={PLOT_PAD.top}
            width={plotW}
            height={plotH}
            rx={4}
          />
          {showXAxis ? (
            <line
              className="graph-axis"
              x1={PLOT_PAD.left}
              y1={axisY}
              x2={PLOT_PAD.left + plotW}
              y2={axisY}
            />
          ) : null}
          {showYAxis ? (
            <line
              className="graph-axis"
              x1={axisX}
              y1={PLOT_PAD.top}
              x2={axisX}
              y2={PLOT_PAD.top + plotH}
            />
          ) : null}
          <text className="graph-tick" x={PLOT_PAD.left} y={SVG_H - 8}>
            {formatCoord(xMin)}
          </text>
          <text
            className="graph-tick"
            x={PLOT_PAD.left + plotW}
            y={SVG_H - 8}
            textAnchor="end"
          >
            {formatCoord(xMax)}
          </text>
          <text className="graph-tick" x={8} y={PLOT_PAD.top + 4}>
            {formatCoord(yScale.max)}
          </text>
          <text className="graph-tick" x={8} y={PLOT_PAD.top + plotH}>
            {formatCoord(yScale.min)}
          </text>
          {pathD ? <path className="graph-curve" d={pathD} fill="none" /> : null}
          {roots.map((r, i) => {
            const cx = xToPx(r.x)
            const cy = yToPx(0)
            if (!Number.isFinite(cx) || !Number.isFinite(cy)) return null
            if (cx < PLOT_PAD.left || cx > PLOT_PAD.left + plotW) return null
            return (
              <circle
                key={`root-${i}`}
                className="graph-mark graph-mark-root"
                cx={cx}
                cy={cy}
                r={4}
                onClick={(e) => {
                  e.stopPropagation()
                  selectX(r.x)
                }}
              >
                <title>
                  root x = {formatCoord(r.x)}
                </title>
              </circle>
            )
          })}
          {critical.map((p, i) => {
            const cx = xToPx(p.x)
            const cy = yToPx(p.y)
            if (!Number.isFinite(cx) || !Number.isFinite(cy)) return null
            if (
              cx < PLOT_PAD.left ||
              cx > PLOT_PAD.left + plotW ||
              cy < PLOT_PAD.top ||
              cy > PLOT_PAD.top + plotH
            ) {
              return null
            }
            return (
              <circle
                key={`crit-${i}`}
                className={`graph-mark graph-mark-${p.kind}`}
                cx={cx}
                cy={cy}
                r={5}
                onClick={(e) => {
                  e.stopPropagation()
                  selectX(p.x)
                }}
              >
                <title>
                  {kindLabel(p.kind)} ({formatCoord(p.x)}, {formatCoord(p.y)})
                </title>
              </circle>
            )
          })}
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
                  title="Copy / insert x"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => selectX(p.x)}
                >
                  <span className={`graph-kind graph-kind-${p.kind}`}>{kindLabel(p.kind)}</span>
                  <span className="graph-coords">
                    ({formatCoord(p.x)}, {formatCoord(p.y)})
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
                    title="Copy / insert x"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => selectX(r.x)}
                  >
                    <span className="graph-kind graph-kind-root">root</span>
                    <span className="graph-coords">x = {formatCoord(r.x)}</span>
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
