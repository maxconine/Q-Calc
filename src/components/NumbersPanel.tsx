import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { autofillParens, fillParens, inferParens } from '../engine/parens'
import { nativeWindow } from '../lib/bridge'
import { plainKey, swallow } from '../lib/gameKeys'
import {
  checkExpr,
  countdownPoints,
  dealCountdown,
  deal24,
  fracText,
  loadStats,
  record24,
  recordCountdown,
  saveStats,
  SECONDS_24,
  SECONDS_COUNTDOWN,
  solves24,
  TARGET_24,
  type Mode,
  type NumbersStats,
} from '../lib/numbers'
import './NumbersPanel.css'

// the numbers game: `24 game` or `countdown` then enter. a start screen picks the mode, then rounds against
// the clock. the panel has its own input; the bar's hidden one must not see these keys (the mac app's boot
// script buffers them for it, so that buffer is dropped on every key)

type Round = {
  mode: Mode
  nums: number[]
  target: number
  solution: string
  // the closest anyone can get (countdown); 24 is always exact
  bestValue: number
  deadline: number
  // over: how it ended
  over: null | { how: 'solved' | 'time' | 'gave up' | 'locked'; value: number | null; points: number }
}

function newRound(mode: Mode, large: number): Round {
  const now = Date.now()
  if (mode === '24') {
    const { nums, solution } = deal24()
    return { mode, nums, target: TARGET_24, solution, bestValue: TARGET_24, deadline: now + SECONDS_24 * 1000, over: null }
  }
  const { nums, target, best } = dealCountdown(large)
  return { mode, nums, target, solution: best.expr, bestValue: best.value, deadline: now + SECONDS_COUNTDOWN * 1000, over: null }
}

// which tiles the typed numbers take, left to right; a number that isn't there takes none
function usedTiles(text: string, nums: readonly number[]): boolean[] {
  const taken = nums.map(() => false)
  for (const m of text.match(/\d+/g) ?? []) {
    const at = nums.findIndex((n, i) => !taken[i] && String(n) === m)
    if (at >= 0) taken[at] = true
  }
  return taken
}

const MODES: { mode: Mode; name: string; blurb: string }[] = [
  { mode: '24', name: '24', blurb: `four numbers, each once · make exactly 24 · ${SECONDS_24}s` },
  { mode: 'countdown', name: 'countdown', blurb: `six numbers, each at most once · get near the target · ${SECONDS_COUNTDOWN}s` },
]

export function NumbersPanel({ onClose }: { onClose: () => void }) {
  const [stats, setStats] = useState<NumbersStats>(loadStats)
  const [round, setRound] = useState<Round | null>(null)
  const [text, setText] = useState('')
  const [note, setNote] = useState('')
  const [now, setNow] = useState(Date.now)
  const inputRef = useRef<HTMLInputElement>(null)

  const update = useCallback((next: (s: NumbersStats) => NumbersStats) => {
    setStats((s) => {
      const out = next(s)
      saveStats(out)
      return out
    })
  }, [])

  const start = useCallback((mode: Mode, large: number) => {
    update((s) => ({ ...s, mode, large }))
    setRound(newRound(mode, large))
    setText('')
    setNote('')
    setNow(Date.now())
  }, [update])

  // like the bar: a missing `)` is read as there, shows faint after the text, and → or tab writes it in
  const filled = fillParens(text)
  const check = useMemo(() => (round ? checkExpr(filled, round.nums, round.mode) : null), [round, filled])

  // read by the clock and finish, which outlive a render
  const roundRef = useRef(round)
  roundRef.current = round
  const textRef = useRef(text)
  textRef.current = text

  // ends the round once; points and streaks are kept as it ends
  const finish = useCallback(
    (how: NonNullable<Round['over']>['how'], value: number | null) => {
      const r = roundRef.current
      if (!r || r.over) return
      const points = r.mode === 'countdown' && how !== 'gave up' ? countdownPoints(r.target, value) : 0
      if (r.mode === '24') update((s) => record24(s, how === 'solved'))
      else update((s) => recordCountdown(s, points))
      const next = { ...r, over: { how, value, points } }
      roundRef.current = next
      setRound(next)
      setNote('')
    },
    [update],
  )

  // the clock; when it runs out, countdown takes what's typed if it's a legal answer
  const playing = Boolean(round && !round.over)
  useEffect(() => {
    if (!playing) return
    const t = window.setInterval(() => {
      const at = Date.now()
      setNow(at)
      const r = roundRef.current
      if (!r || r.over || at < r.deadline) return
      const c = checkExpr(fillParens(textRef.current), r.nums, r.mode)
      finish('time', r.mode === 'countdown' && c.valid && c.value ? c.value.n : null)
    }, 200)
    return () => window.clearInterval(t)
  }, [playing, finish])

  const left = round ? Math.max(0, round.deadline - now) : 0

  // the input has the keys while a round is up; the mac app may have tried to focus the hidden bar on show
  useEffect(() => {
    if (!round) return
    const timers = [0, 40, 120, 280].map((ms) => window.setTimeout(() => inputRef.current?.focus(), ms))
    return () => timers.forEach((t) => window.clearTimeout(t))
  }, [round])

  const submit = useCallback(() => {
    if (!round || round.over || !check) return
    if (round.mode === '24') {
      if (solves24(filled, round.nums)) finish('solved', TARGET_24)
      else if (check.problem) setNote(check.problem)
      else if (check.value) setNote(`that’s ${fracText(check.value)}, not 24`)
      else setNote(text.trim() ? 'not finished' : 'type an expression')
      return
    }
    if (check.valid && check.value) finish('locked', check.value.n)
    else setNote(check.problem ?? (text.trim() ? 'not finished' : 'type an expression'))
  }, [round, check, text, filled, finish])

  const [pick, setPick] = useState<Mode>(stats.mode)
  const [large, setLarge] = useState(stats.large)

  const keys = useRef<(e: KeyboardEvent) => void>(() => {})
  keys.current = (e: KeyboardEvent) => {
    if (e.key === 'Escape' || e.key === 'Esc') return
    if (!plainKey(e)) return
    const input = inputRef.current
    const inField = input != null && e.target === input
    if (!round) {
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') setPick((m) => (m === '24' ? 'countdown' : '24'))
      else if (e.key === 'ArrowLeft') setLarge((n) => Math.max(0, n - 1))
      else if (e.key === 'ArrowRight') setLarge((n) => Math.min(4, n + 1))
      else if (/^[0-4]$/.test(e.key) && pick === 'countdown') setLarge(Number(e.key))
      else if (e.key === 'Enter' && !e.repeat) start(pick, large)
      else if (!(e.key.length === 1 || e.key === 'Backspace' || e.key === 'Tab')) return
      swallow(e)
      return
    }
    if (e.key === 'Enter') {
      if (!e.repeat) {
        if (round.over) start(round.mode, stats.large)
        else submit()
      }
      swallow(e)
      return
    }
    if (e.key === '?') {
      if (!round.over) finish('gave up', null)
      swallow(e)
      return
    }
    if (round.over) {
      if (e.key.length === 1 || e.key === 'Backspace' || e.key.startsWith('Arrow')) swallow(e)
      return
    }
    if (inField && (e.key === 'Tab' || e.key === 'ArrowRight') && input) {
      const written = autofillParens(input.value, input.selectionStart ?? 0, input.selectionEnd ?? 0)
      if (written) {
        setText(written)
        requestAnimationFrame(() => input.setSelectionRange(written.length, written.length))
        swallow(e)
        return
      }
      // tab never leaves the field
      if (e.key === 'Tab') {
        swallow(e)
        return
      }
    }
    if (inField) {
      // typing into our own field: let it, but keep it out of the bar's buffer
      const w = nativeWindow()
      if (w?.__QCALC_KEYS?.length) w.__QCALC_KEYS = []
      e.stopPropagation()
      return
    }
    // focus wandered (a click, the app's show): put the key where it belongs
    if (e.key.length === 1) setText((t) => t + e.key)
    else if (e.key === 'Backspace') setText((t) => t.slice(0, -1))
    else if (!e.key.startsWith('Arrow')) return
    input?.focus()
    swallow(e)
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => keys.current(e)
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [])

  const head = (
    <div className="nums-head">
      <span className="nums-title">{round ? (round.mode === '24' ? '24' : 'countdown') : 'the numbers game'}</span>
      <span className="nums-sub">
        {round?.mode === '24'
          ? `streak ${stats.streak24} · best ${stats.best24}`
          : round
            ? `${stats.points} pts in ${stats.rounds} · exact run ${stats.exactStreak} · best ${stats.bestExact}`
            : 'make the number'}
      </span>
      {round ? (
        <button type="button" className="nums-leave" onClick={() => setRound(null)}>
          modes
        </button>
      ) : null}
      <button type="button" className="nums-leave" onClick={onClose}>
        esc leaves
      </button>
    </div>
  )

  if (!round) {
    return (
      <div className="nums" aria-label="numbers game">
        {head}
        <div className="nums-modes" role="listbox" aria-label="mode">
          {MODES.map((m) => (
            <div
              key={m.mode}
              role="option"
              aria-selected={pick === m.mode}
              className={`nums-mode ${pick === m.mode ? 'on' : ''}`}
              onClick={() => start(m.mode, large)}
              onMouseEnter={() => setPick(m.mode)}
            >
              <span className="nums-mode-name">{m.name}</span>
              <span className="nums-mode-blurb">{m.blurb}</span>
              {m.mode === 'countdown' ? (
                <span className="nums-large" onClick={(e) => e.stopPropagation()}>
                  large
                  {[0, 1, 2, 3, 4].map((n) => (
                    <button type="button" key={n} className={n === large ? 'on' : ''} onClick={() => setLarge(n)}>
                      {n}
                    </button>
                  ))}
                </span>
              ) : null}
            </div>
          ))}
        </div>
        <p className="nums-note">↑↓ pick · ←→ large numbers · ↵ start · esc leaves</p>
      </div>
    )
  }

  const total = (round.mode === '24' ? SECONDS_24 : SECONDS_COUNTDOWN) * 1000
  const taken = usedTiles(text, round.nums)
  // the faint closing parens, only while the text fits, since the ghost doesn't scroll with the input
  const field = inputRef.current
  const fits = !field || field.scrollWidth <= field.clientWidth + 1
  const over = round.over
  const ghost = !over && fits ? ')'.repeat(inferParens(text).trailing) : ''
  let live = ''
  let liveBad = false
  if (note) {
    live = note
    liveBad = true
  } else if (check?.value) {
    // numbers still to use isn't wrong yet, just not done
    live = check.problem ? `= ${fracText(check.value)} · ${check.problem}` : `= ${fracText(check.value)}`
    liveBad = Boolean(check.problem && !check.problem.startsWith('use '))
  } else if (check?.problem) {
    live = check.problem
    liveBad = true
  }

  let result = ''
  if (over) {
    if (round.mode === '24') {
      result = over.how === 'solved' ? `24! streak ${stats.streak24}` : over.how === 'time' ? 'time’s up' : 'gave up'
    } else {
      const off = over.value == null ? null : Math.abs(round.target - over.value)
      const what = over.how === 'gave up' ? 'gave up' : over.value == null ? (over.how === 'time' ? 'time’s up · nothing locked in' : 'nothing') : off === 0 ? `exactly ${round.target}!` : `${over.how === 'time' ? 'time’s up · ' : ''}${over.value}, ${off} away`
      result = `${what} · ${over.points} pts`
    }
  }
  const solutionLine = round.mode === 'countdown' && round.bestValue !== round.target ? `${round.solution} = ${round.bestValue} (closest possible)` : `${round.solution} = ${round.target}`

  return (
    <div className="nums" aria-label={round.mode === '24' ? '24' : 'countdown'}>
      {head}
      <div className="nums-board">
        <div className="nums-target">
          {round.mode === 'countdown' ? <span className="nums-target-label">target</span> : <span className="nums-target-label">make</span>}
          <span className="nums-target-value">{round.target}</span>
        </div>
        <div className={`nums-tiles ${round.mode === 'countdown' ? 'six' : ''}`}>
          {round.nums.map((n, i) => (
            <button
              type="button"
              key={i}
              className={`nums-tile ${taken[i] ? 'used' : ''} ${n >= 25 && round.mode === 'countdown' ? 'large' : ''}`}
              disabled={Boolean(over)}
              onClick={() => {
                setText((t) => (t && /\d$/.test(t) ? `${t} + ${n}` : `${t}${n}`))
                inputRef.current?.focus()
              }}
            >
              {n}
            </button>
          ))}
        </div>
        <div className="nums-clock" aria-hidden>
          <div className={`nums-clock-fill ${left < 10_000 ? 'low' : ''}`} style={{ width: `${over ? 0 : (left / total) * 100}%` }} />
        </div>
        <div className="nums-entry">
          <input
            ref={inputRef}
            autoFocus
            className="nums-input"
            value={text}
            readOnly={Boolean(over)}
            spellCheck={false}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            placeholder={round.mode === '24' ? 'e.g. (8 − 2) × 4' : 'e.g. 100 × 7 − 25'}
            aria-label="your expression"
            onChange={(e) => {
              setText(e.target.value.replace(/\?/g, ''))
              setNote('')
            }}
          />
          {ghost ? (
            <span className="nums-ghost" aria-hidden>
              <span className="nums-ghost-typed">{text}</span>
              {ghost}
            </span>
          ) : null}
          <span className="nums-secs">{over ? '' : `${Math.ceil(left / 1000)}s`}</span>
        </div>
        <p className={`nums-live ${liveBad ? 'bad' : ''}`}>{over ? '' : live || ' '}</p>
        {over ? (
          <div className="nums-over">
            <p className={`nums-result ${over.how === 'solved' || over.points === 10 ? 'win' : ''}`}>{result}</p>
            {over.how !== 'solved' && over.points !== 10 ? <p className="nums-solution">{solutionLine}</p> : null}
          </div>
        ) : null}
      </div>
      <p className="nums-note">
        {over ? '↵ next · ' : round.mode === '24' ? '↵ check · ' : '↵ lock in · '}
        {over ? null : (
          <button type="button" className="nums-link" onClick={() => finish('gave up', null)}>
            ? show a solution
          </button>
        )}
        {over ? '' : ' · '}esc leaves
      </p>
    </div>
  )
}
