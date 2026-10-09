import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { copyText } from '../lib/dom'
import {
  currentStreak,
  dailyAnswer,
  dailyNumber,
  dateKey,
  generate,
  glyph,
  guessProblem,
  isDone,
  isWon,
  keyMarks,
  LEN,
  loadSave,
  recordDaily,
  score,
  shareText,
  storeSave,
  TRIES,
  type EqualSave,
  type Mark,
} from '../lib/equal'
import { plainKey, swallow } from '../lib/gameKeys'
import './EqualPanel.css'

// equal: `equal` then enter. guess the hidden 8-character equation in six. daily is one puzzle a day for
// everyone, saved with the streak; practice deals a fresh one each time. esc leaves through escapeLayer

type Mode = 'daily' | 'practice'

const ROWS = ['1234567890', '+-*/=']

const newPractice = () => generate(Math.random)

export function EqualPanel({ onClose }: { onClose: () => void }) {
  const [mode, setMode] = useState<Mode>('daily')
  const [save, setSave] = useState<EqualSave>(loadSave)
  const [today, setToday] = useState(() => dateKey(new Date()))
  const [practice, setPractice] = useState(() => ({ answer: newPractice(), guesses: [] as string[] }))
  const [input, setInput] = useState('')
  const [note, setNote] = useState('')
  const [shake, setShake] = useState(0)
  // the row that was just entered flips over
  const [fresh, setFresh] = useState(-1)
  const [copied, setCopied] = useState(false)

  const daily = mode === 'daily'
  const answer = daily ? dailyAnswer(today) : practice.answer
  const guesses = useMemo(
    () => (daily ? (save.daily?.date === today ? save.daily.guesses : []) : practice.guesses),
    [daily, practice.guesses, save.daily, today],
  )
  const over = isDone(guesses, answer)
  const won = isWon(guesses, answer)
  const title = daily ? `equal #${dailyNumber(today)}` : 'equal practice'

  const switchTo = useCallback((m: Mode) => {
    setMode(m)
    setInput('')
    setNote('')
    setFresh(-1)
    setCopied(false)
    // past midnight, daily moves on to the new day's puzzle
    if (m === 'daily') setToday(dateKey(new Date()))
  }, [])

  const nextPractice = useCallback(() => {
    setPractice({ answer: newPractice(), guesses: [] })
    setInput('')
    setNote('')
    setFresh(-1)
    setCopied(false)
  }, [])

  const refuse = useCallback((text: string) => {
    setNote(text)
    setShake((n) => n + 1)
  }, [])

  const submit = useCallback(() => {
    if (over) return
    const problem = guessProblem(input)
    if (problem) return refuse(problem)
    const next = [...guesses, input]
    setFresh(guesses.length)
    setInput('')
    setNote('')
    if (!daily) return setPractice((p) => ({ ...p, guesses: next }))
    const stats = isDone(next, answer) ? recordDaily(save.stats, today, next, answer) : save.stats
    const saved = { daily: { date: today, guesses: next }, stats }
    storeSave(saved)
    setSave(saved)
  }, [answer, daily, guesses, input, over, refuse, save.stats, today])

  const press = useCallback(
    (k: string) => {
      if (k === 'Enter') {
        if (over && !daily) nextPractice()
        else submit()
      } else if (over) {
        if (daily) setNote('today’s is done · P for practice')
      } else if (k === 'Backspace') {
        setInput((s) => s.slice(0, -1))
        setNote('')
      } else if (input.length < LEN) setInput((s) => (s.length < LEN ? s + k : s))
    },
    [daily, input.length, nextPractice, over, submit],
  )

  const copy = useCallback(() => {
    copyText(shareText(guesses, answer, title))
    setCopied(true)
  }, [answer, guesses, title])

  const act = useRef({ press, copy, switchTo, nextPractice, over, daily })
  act.current = { press, copy, switchTo, nextPractice, over, daily }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!plainKey(e) || e.key === 'Escape') return
      const k = e.key === 'x' || e.key === 'X' || e.key === '×' ? '*' : e.key === '÷' ? '/' : e.key
      const a = act.current
      if (/^[0-9+\-*/=]$/.test(k) || k === 'Backspace' || k === 'Enter') {
        if (!(k === 'Enter' && e.repeat)) a.press(k)
      } else if (k === 'd' || k === 'D') a.switchTo('daily')
      else if (k === 'p' || k === 'P') a.switchTo('practice')
      // C copies and N deals again only once a game is over
      else if ((k === 'c' || k === 'C') && a.over) a.copy()
      else if ((k === 'n' || k === 'N') && a.over && !a.daily) a.nextPractice()
      else if (k.length !== 1 && !k.startsWith('Arrow')) return
      swallow(e)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [])

  const keys = keyMarks(guesses, answer)
  const streak = currentStreak(save.stats, today)
  const { played, wins } = save.stats

  return (
    <div className="pong equal" aria-label="equal">
      <div className="pong-head">
        <span className="pong-title">equal</span>
        <span className="pong-sub">{daily ? `#${dailyNumber(today)} · streak ${streak}` : 'practice'}</span>
        <span className="equal-modes" role="tablist">
          {(['daily', 'practice'] as const).map((m) => (
            <button
              type="button"
              role="tab"
              aria-selected={mode === m}
              key={m}
              className={mode === m ? 'on' : ''}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => switchTo(m)}
            >
              {m}
            </button>
          ))}
        </span>
        <button type="button" className="pong-leave equal-leave" onClick={onClose}>
          esc
        </button>
      </div>

      <div className="equal-grid" role="grid" aria-label="guesses">
        {Array.from({ length: TRIES }, (_, r) => {
          const done = r < guesses.length
          const text = done ? guesses[r] : r === guesses.length && !over ? input : ''
          const marks: Mark[] | null = done ? score(guesses[r], answer) : null
          const current = r === guesses.length && !over
          return (
            <div key={current ? `in-${shake}` : r} className={`equal-row ${current && shake ? 'shake' : ''}`} role="row">
              {Array.from({ length: LEN }, (_, i) => {
                const c = text[i] ?? ''
                const cls = ['equal-tile']
                if (marks) cls.push(marks[i])
                else if (c) cls.push('typed')
                if (marks && r === fresh) cls.push('flip')
                return (
                  <span key={i} className={cls.join(' ')} role="gridcell" style={r === fresh ? { animationDelay: `${i * 70}ms` } : undefined}>
                    {glyph(c)}
                  </span>
                )
              })}
            </div>
          )
        })}
      </div>

      {over ? (
        <div className="equal-summary" aria-live="polite">
          <p className={`equal-result ${won ? 'won' : ''}`}>
            {won ? `got it in ${guesses.length}` : 'out of guesses'} · the answer was <b>{[...answer].map(glyph).join('')}</b>
          </p>
          {daily ? (
            <p className="pong-note">
              streak {streak} · best {save.stats.best} · played {played} · {played ? Math.round((wins / played) * 100) : 0}% won · next one tomorrow
            </p>
          ) : null}
          <div className="pong-actions">
            <button type="button" className="pong-button" onMouseDown={(e) => e.preventDefault()} onClick={copy}>
              {copied ? 'copied' : 'copy result'} <kbd>C</kbd>
            </button>
            {daily ? (
              <button type="button" className="pong-button" onMouseDown={(e) => e.preventDefault()} onClick={() => switchTo('practice')}>
                practice <kbd>P</kbd>
              </button>
            ) : (
              <button type="button" className="pong-button" onMouseDown={(e) => e.preventDefault()} onClick={nextPractice}>
                next puzzle <kbd>↵</kbd>
              </button>
            )}
          </div>
          {note ? <p className="pong-note">{note}</p> : null}
        </div>
      ) : (
        <>
          <p className="equal-note" aria-live="polite">
            {note || `${TRIES - guesses.length} ${TRIES - guesses.length === 1 ? 'guess' : 'guesses'} left`}
          </p>
          <div className="equal-keys">
            {ROWS.map((row, r) => (
              <div className="equal-keyrow" key={r}>
                {[...row].map((k) => (
                  <button
                    type="button"
                    key={k}
                    className={`equal-key ${keys[k] ?? ''}`}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => press(k)}
                  >
                    {glyph(k)}
                  </button>
                ))}
                {r === 1 ? (
                  <>
                    <button type="button" className="equal-key wide" aria-label="delete" onMouseDown={(e) => e.preventDefault()} onClick={() => press('Backspace')}>
                      ⌫
                    </button>
                    <button type="button" className="equal-key wide enter" aria-label="enter" onMouseDown={(e) => e.preventDefault()} onClick={() => press('Enter')}>
                      ↵
                    </button>
                  </>
                ) : null}
              </div>
            ))}
          </div>
          <p className="pong-note equal-help">green: right spot · yellow: elsewhere · grey: not in it · D daily · P practice · esc leaves</p>
        </>
      )}
    </div>
  )
}
