import { useEffect, useRef, useState } from 'react'
import {
  chordFromEvent,
  chordLabel,
  chordProblem,
  defaultKeybind,
  KEY_ACTIONS,
  keybindFor,
  keyRecorder,
  heldLabel,
  missedKeyText,
  setKeybind,
  type KeyAction,
  type Keybinds,
} from '../lib/keybinds'
import { conflictText, type HotkeyVerdict } from '../lib/hostSettings'

type Props = {
  value: Keybinds
  onChange: (next: Keybinds) => void
  windows: boolean
  // only a host has a settings window for a key to open
  settingsWindow: boolean
  // the host's global shortcut, which the host registers and may refuse; note says why, or is ''. left out on the web page
  show?: {
    chord: string
    note: string
    onPick: (chord: string) => void
    onRecording?: (on: boolean) => void
    // the windows shell tries a recorded chord before it's kept and answers with verdict; while the row records it
    // also passes on alt+space, which web view2 gives it instead of this page, with a 'qcalc-hotkey' event
    check?: (chord: string) => void
    verdict?: HotkeyVerdict | null
    // a free chord the note offers instead of a taken one
    suggest?: string
  }
}

const IDLE_HINT = 'Click a shortcut, then press the keys you want.'
const RECORDING_HINT = 'Press the new keys. Esc cancels, ⌫ leaves it without a key.'
// the shell keeps windows' window menu off alt+space while this records
const HOST_RECORDING_HINT = 'Press the new keys, Alt+Space too. Esc cancels.'

function heldMods(e: KeyboardEvent): string {
  const held = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, cmd: e.metaKey }
  return (Object.keys(held) as Array<keyof typeof held>).filter((m) => held[m]).join('+')
}

// the recording key shows what it hears
function recordingLabel(pending: string, held: string, windows: boolean): string {
  if (pending) return `${chordLabel(pending, windows)}…`
  return heldLabel(held, windows) || 'Press keys…'
}

// "Choose keybinds…" opens a row per action; clicking a row's key records the next chord pressed
export function KeybindSettings({ value, onChange, windows, settingsWindow, show }: Props) {
  const [open, setOpen] = useState(false)
  const [recording, setRecording] = useState<KeyAction | null>(null)
  const [problem, setProblem] = useState('')
  // what the row has heard so far: modifiers down, then a chord the shell is trying
  const [held, setHeld] = useState('')
  const [pending, setPending] = useState('')
  const [offer, setOffer] = useState('')
  // the modifiers held so far in this press, and whether a whole chord came through before they went up
  const sawMods = useRef('')
  const sawChord = useRef(false)
  const actions = KEY_ACTIONS.filter((a) => (a.id !== 'show' || show) && (a.id !== 'settings' || settingsWindow))
  // conflicts are checked against the global shortcut too
  const all: Keybinds = show ? { ...value, show: show.chord } : value
  const onShowRecording = show?.onRecording
  const hostChecks = recording === 'show' && Boolean(show?.check)
  const verdict = show?.verdict

  // the old show / hide key would reach the host instead of this page
  useEffect(() => {
    if (recording !== 'show' || !onShowRecording) return
    onShowRecording(true)
    return () => onShowRecording(false)
  }, [recording, onShowRecording])

  // the shell's answer: a free chord is kept, a taken one says who has it and the row keeps listening
  useEffect(() => {
    if (!pending || !verdict || verdict.chord !== pending) return
    setPending('')
    if (verdict.free) {
      pick('show', pending)
      return
    }
    setProblem(conflictText(verdict))
    setOffer(verdict.suggest)
  })

  useEffect(() => {
    if (!recording) return
    keyRecorder.active = true
    const take = (chord: string) => {
      setHeld('')
      const why = chordProblem(chord, recording, all, windows)
      if (why) {
        setProblem(why)
        return
      }
      if (recording === 'show' && show?.check) {
        setProblem('')
        setOffer('')
        setPending(chord)
        show.check(chord)
        return
      }
      pick(recording, chord)
    }
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      const plain = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey
      if (plain && (e.key === 'Escape' || e.key === 'Esc')) {
        setRecording(null)
        forget()
        return
      }
      if (plain && (e.key === 'Backspace' || e.key === 'Delete')) {
        if (recording === 'show') setProblem('Show / hide needs a key')
        else pick(recording, '')
        return
      }
      const chord = chordFromEvent(e)
      if (chord) {
        sawChord.current = true
        take(chord)
        return
      }
      const mods = heldMods(e)
      setHeld(mods)
      if (mods.length > sawMods.current.length) sawMods.current = mods
    }
    const onKeyUp = (e: KeyboardEvent) => {
      const mods = heldMods(e)
      setHeld(mods)
      if (mods) return
      // every modifier is up again with no chord between: on windows, another app's shortcut took the key
      const missed = hostChecks && !sawChord.current ? missedKeyText(sawMods.current) : ''
      sawMods.current = ''
      sawChord.current = false
      if (missed) setProblem(missed)
    }
    // alt+space, which web view2 gives the windows shell rather than this page; the shell passes it on
    const onHost = (e: Event) => {
      const heard = ((e as CustomEvent).detail as { heard?: unknown } | null)?.heard
      if (typeof heard === 'string' && heard) take(heard)
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('keyup', onKeyUp, true)
    if (hostChecks) window.addEventListener('qcalc-hotkey', onHost)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('keyup', onKeyUp, true)
      window.removeEventListener('qcalc-hotkey', onHost)
      keyRecorder.active = false
    }
  })

  function forget() {
    setProblem('')
    setHeld('')
    setPending('')
    setOffer('')
  }

  function pick(action: KeyAction, chord: string) {
    setRecording(null)
    forget()
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
  const recordingHint = hostChecks ? HOST_RECORDING_HINT : RECORDING_HINT
  const note = problem || (recording ? recordingHint : show?.note || (open ? IDLE_HINT : 'Every shortcut Q Calc answers to.'))
  // a free chord, offered with a taken one's note
  const suggestion = recording === 'show' ? (problem ? offer : '') : !recording && show?.note ? (show.suggest ?? '') : ''

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
      {suggestion ? (
        <button type="button" className="unit-reset keybind-suggest" onClick={() => pick('show', suggestion)}>
          Use {chordLabel(suggestion, windows)}
        </button>
      ) : null}
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
                      forget()
                      setRecording(active ? null : id)
                    }}
                  >
                    {active ? recordingLabel(pending, held, windows) : chord ? chordLabel(chord, windows) : 'None'}
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
