import { useEffect, useRef, useState } from 'react'
import { plainKey, swallow } from '../lib/gameKeys'
import { HIT_WINDOW, loadBest, press, saveBest, startLevel, tick, type CrackEvent, type CrackState } from '../lib/crack'
import './CrackPanel.css'

// the dial, in svg units: the track the needle and dot ride on, and the tick ring inside it
const C = 100
const TRACK_R = 76
const TICK_R = 58
const DOT_R = Math.round(TRACK_R * HIT_WINDOW * 0.85)
const TICKS = Array.from({ length: 40 }, (_, i) => i)

const polar = (angle: number, r: number) => ({ x: C + r * Math.sin(angle), y: C - r * Math.cos(angle) })

// crack: `crack` then enter. one player, no link; esc leaves through the bar's escapeLayer
export function CrackPanel({ onClose }: { onClose: () => void }) {
  const [best, setBest] = useState(loadBest)
  const [s, setS] = useState<CrackState>(() => startLevel(best, performance.now(), Math.random))
  // the last popped dot, keyed so each pop replays its animation
  const [pop, setPop] = useState<{ n: number; angle: number } | null>(null)
  const ref = useRef(s)
  const hitRef = useRef<() => void>(() => {})

  useEffect(() => {
    let raf = 0
    const apply = (next: { state: CrackState; event: CrackEvent }) => {
      const prev = ref.current
      ref.current = next.state
      if (next.event === 'hit' || next.event === 'clear') setPop((p) => ({ n: (p?.n ?? 0) + 1, angle: prev.dot }))
      if (next.state.level > prev.level) {
        saveBest(next.state.level)
        setBest((b) => Math.max(b, next.state.level))
      }
      setS(next.state)
    }
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame)
      const cur = ref.current
      if (cur.phase === 'ready') return
      apply(tick(cur, now, Math.random))
    }
    raf = requestAnimationFrame(frame)
    const hit = () => apply(press(ref.current, performance.now(), Math.random))

    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape') return
      if (e.key === ' ' || e.key === 'Enter') {
        if (!e.repeat) hit()
      } else if (e.key.length !== 1 && e.key !== 'Backspace') return
      swallow(e)
    }
    const onUp = (e: KeyboardEvent) => {
      if (plainKey(e) && (e.key === ' ' || e.key === 'Enter')) swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('keyup', onUp, true)
    hitRef.current = hit
    return () => {
      cancelAnimationFrame(raf)
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('keyup', onUp, true)
    }
  }, [])

  const needleIn = polar(s.angle, TRACK_R - 13)
  const needleOut = polar(s.angle, TRACK_R + 13)
  const dot = polar(s.dot, TRACK_R)
  const popAt = pop ? polar(pop.angle, TRACK_R) : null
  const big = s.phase === 'clear' ? 'open' : String(s.level)
  const small = s.phase === 'clear' ? `level ${s.level} cracked` : s.phase === 'fail' ? 'missed' : `${s.left} to go`

  const note =
    s.phase === 'ready' ? 'space, ↵ or a click starts the needle · esc leaves' : 'press as the needle crosses the dot · esc leaves'

  return (
    <div className="pong crack" aria-label="Crack">
      <div className="pong-head">
        <span className="pong-title">crack</span>
        <span className="pong-sub">best level {best}</span>
        <button type="button" className="pong-leave" onClick={onClose}>
          esc
        </button>
      </div>
      <div className="crack-stage">
        <svg
          viewBox="0 0 200 200"
          className={`crack-dial ${s.phase}`}
          role="button"
          aria-label={`level ${s.level}, ${s.left} to go`}
          onPointerDown={(e) => {
            e.preventDefault()
            hitRef.current()
          }}
        >
          <circle className="crack-bezel" cx={C} cy={C} r={TRACK_R + 18} />
          <circle className="crack-track" cx={C} cy={C} r={TRACK_R} />
          <g className="crack-ticks">
            {TICKS.map((i) => {
              const a = (i / TICKS.length) * Math.PI * 2
              const p = polar(a, TICK_R)
              const q = polar(a, TICK_R - (i % 5 === 0 ? 7 : 3.5))
              return <line key={i} x1={p.x} y1={p.y} x2={q.x} y2={q.y} className={i % 5 === 0 ? 'major' : ''} />
            })}
          </g>
          {s.phase !== 'clear' ? <circle className="crack-dot" cx={dot.x} cy={dot.y} r={DOT_R} /> : null}
          {popAt ? <circle key={pop!.n} className="crack-pop" cx={popAt.x} cy={popAt.y} r={DOT_R} /> : null}
          <line className="crack-needle" x1={needleIn.x} y1={needleIn.y} x2={needleOut.x} y2={needleOut.y} />
          <text className="crack-level" x={C} y={C + 4}>
            {big}
          </text>
          <text className="crack-left" x={C} y={C + 24}>
            {small}
          </text>
        </svg>
      </div>
      <p className="pong-note crack-note">{note}</p>
    </div>
  )
}
