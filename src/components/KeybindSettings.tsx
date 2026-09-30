import { useEffect, useState } from 'react'
import {
  chordFromEvent,
  chordLabel,
  chordProblem,
  defaultKeybind,
  KEY_ACTIONS,
  keybindFor,
  keyRecorder,
  setKeybind,
  type KeyAction,
  type Keybinds,
} from '../lib/keybinds'

type Props = {
  value: Keybinds
  onChange: (next: Keybinds) => void
  windows: boolean
  // only a host has a settings window for a key to open
  settingsWindow: boolean
  // the host's global shortcut, which the host registers and may refuse; note says why, or is ''. left out on the web page
  show?: { chord: string; note: string; onPick: (chord: string) => void; onRecording?: (on: boolean) => void }
}

const IDLE_HINT = 'Click a shortcut, then press the keys you want.'
const RECORDING_HINT = 'Press the new keys. Esc cancels, ⌫ leaves it without a key.'

// "Choose keybinds…" opens a row per action; clicking a row's key records the next chord pressed
export function KeybindSettings({ value, onChange, windows, settingsWindow, show }: Props) {
  const [open, setOpen] = useState(false)
  const [recording, setRecording] = useState<KeyAction | null>(null)
  const [problem, setProblem] = useState('')
  const actions = KEY_ACTIONS.filter((a) => (a.id !== 'show' || show) && (a.id !== 'settings' || settingsWindow))
  // conflicts are checked against the global shortcut too
  const all: Keybinds = show ? { ...value, show: show.chord } : value
  const onShowRecording = show?.onRecording

  // the old show / hide key would reach the host instead of this page
  useEffect(() => {
    if (recording !== 'show' || !onShowRecording) return
    onShowRecording(true)
    return () => onShowRecording(false)
  }, [recording, onShowRecording])

  useEffect(() => {
    if (!recording) return
    keyRecorder.active = true
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      const plain = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey
      if (plain && (e.key === 'Escape' || e.key === 'Esc')) {
        setRecording(null)
        setProblem('')
        return
      }
      if (plain && (e.key === 'Backspace' || e.key === 'Delete')) {
        if (recording === 'show') setProblem('Show / hide needs a key')
        else pick(recording, '')
        return
      }
      const chord = chordFromEvent(e)
      if (!chord) return
      const why = chordProblem(chord, recording, all, windows)
      if (why) {
        setProblem(why)
        return
      }
      pick(recording, chord)
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      keyRecorder.active = false
    }
  })

  function pick(action: KeyAction, chord: string) {
    setRecording(null)
    setProblem('')
    if (action === 'show') show?.onPick(chord)
    else onChange(setKeybind(value, action, chord, windows))
  }

  const showMoved = Boolean(show && show.chord !== defaultKeybind('show', windows))
  const changed = Object.keys(value).length > 0 || showMoved
  function resetAll() {
    onChange({})
    if (showMoved) show?.onPick(defaultKeybind('show', windows))
  }

  // a host's complaint about the show / hide key wins over the idle hints
  const note = problem || (recording ? RECORDING_HINT : show?.note || (open ? IDLE_HINT : 'Every shortcut Q Calc answers to.'))

  return (
    <section className="unit-settings keybind-settings" aria-label="Keybinds">
      <div className="unit-settings-head">
        <h2>Keybinds</h2>
        {open ? (
          <button type="button" className="unit-reset" disabled={!changed} onClick={resetAll}>
            Reset all
          </button>
        ) : (
          <button type="button" className="unit-reset" onClick={() => setOpen(true)}>
            Choose keybinds…
          </button>
        )}
      </div>
      <p className={`unit-settings-hint ${problem ? 'keybind-problem' : ''}`} aria-live="polite">
        {note}
      </p>
      {open ? (
        <div className="unit-group-body keybind-list">
          {actions.map(({ id, label }) => {
            const chord = id === 'show' ? (show?.chord ?? '') : keybindFor(value, id, windows)
            const isDefault = chord === defaultKeybind(id, windows)
            const active = recording === id
            return (
              <div className="unit-row" key={id}>
                <span className="unit-row-label">{label}</span>
                <span className="keybind-controls">
                  {isDefault ? null : (
                    <button
                      type="button"
                      className="keybind-default"
                      aria-label={`Put ${label.toLowerCase()} back to ${chordLabel(defaultKeybind(id, windows), windows)}`}
                      title={`Back to ${chordLabel(defaultKeybind(id, windows), windows)}`}
                      onClick={() => pick(id, defaultKeybind(id, windows))}
                    >
                      ↺
                    </button>
                  )}
                  <button
                    type="button"
                    className={`keybind-key ${active ? 'recording' : ''} ${chord ? '' : 'none'}`}
                    aria-pressed={active}
                    aria-label={`${label}: ${chord ? chordLabel(chord, windows) : 'no key'}. Change`}
                    onClick={() => {
                      setProblem('')
                      setRecording(active ? null : id)
                    }}
                  >
                    {active ? 'Press keys…' : chord ? chordLabel(chord, windows) : 'None'}
                  </button>
                </span>
              </div>
            )
          })}
        </div>
      ) : null}
    </section>
  )
}
