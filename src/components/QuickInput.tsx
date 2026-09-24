import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MutableRefObject, type ReactNode } from 'react'
import { autofillParens, inferParens } from '../engine/parens'
import { nativeWindow } from '../lib/bridge'
import type { Span } from '../lib/blankReason'
import { completionFor, type CompletionNames } from '../lib/completion'
import { afterTyping, boundKey, boundsIn, wordToSign, type Edit } from '../lib/bounds'
import { copyText, inputHighlight, keepEndInView } from '../lib/dom'
import { knownWordSpans } from '../lib/knownWords'
import { cleanPastedText } from '../lib/paste'
import { breakRun, editKind, recordEdit, redo, undo, undoStart, type EditKind, type Undo, type UndoState } from '../lib/undo'
import { BoundsInputText } from './Bounds'
import { RadicalLayer } from './Radical'

type CaretRange = { start: number; end: number }

export interface QuickInputHandle {
  insert: (chunk: string, at?: CaretRange) => void
  focus: () => void
  setValue: (text: string) => void
  element: () => HTMLInputElement | null
  highlighted: () => string
  caret: () => CaretRange
}

interface Props {
  value: string
  // typed words like sqrt and pi stay as text instead of becoming symbols
  keepWords?: boolean
  onChange: (text: string) => void
  // `alt`: ⌥↵ reuses the other half of a history row
  onEnter: (alt?: boolean) => void
  // `=` at the end of plain arithmetic; true when it saved the line
  onUp: () => boolean
  onDown: () => boolean
  onPrefixStep?: (dir: 1 | -1) => void
  onTab?: (dir: 1 | -1) => void
  // shown in place of the placeholder while the input is empty; a new `id` restarts its fade
  example?: { text: string; id: number } | null
  handleRef?: MutableRefObject<QuickInputHandle | null>
  // a faint `ans` before the text while it continues from the last answer
  chain?: boolean
  // the part a blank answer couldn't read
  squiggle?: Span | null
  completionNames?: CompletionNames
  onSelection?: (range: CaretRange) => void
}

// letters around a word make it part of a longer word (`pint`, `infinity`); digits before it are a coefficient (`2pi`)
const WORD_SYMBOLS: [RegExp, string][] = [
  [/(?<![\\A-Za-z])sqrt(?![A-Za-z])/gi, '√'],
  [/(?<![\\A-Za-z])pi(?![A-Za-z])/gi, 'π'],
  [/(?<![\\A-Za-z])theta(?![A-Za-z0-9])/gi, 'θ'],
  [/(?<![\\A-Za-z])infty(?![A-Za-z0-9])/gi, '∞'],
  [/(?<![\\A-Za-z])inf(?![A-Za-z0-9])/gi, '∞'],
  [/(?<![\\A-Za-z])cbrt(?![A-Za-z])/gi, '∛'],
  [/(?<=\blim(?:it)?\s*_?[({]?\s*[A-Za-z]\s*)->/g, '→'],
  [/(?<![\\A-Za-z])plus[\s.-]?minus(?![A-Za-z])/gi, '±'],
  [/(?<![\\A-Za-z])minus[\s.-]?plus(?![A-Za-z])/gi, '∓'],
  [/(?<![\\A-Za-z])dot(?![A-Za-z])/gi, '*'],
  [/(?<![\\A-Za-z_])sum(?![A-Za-z0-9])/g, 'Σ'],
  [/(?<![\\A-Za-z_])prod(?![A-Za-z0-9])/g, 'Π'],
  [/(?<![\\A-Za-z_])int(?![A-Za-z]|_[A-Za-z])/g, '∫'],
]

// these turn into symbols the moment they're typed; theta, dot and ans still wait a keystroke
const AT_ONCE = new Set(['√', '∛', '±', '∓', '∫', 'π', 'Σ', 'Π', '∞'])

// if the letters after a fresh symbol spell a longer word, the word comes back (`π` then `nt` is `pint`); the `_` is a limit slot
const LONGER_WORDS: [string, string, string[]][] = [
  ['π', 'pi', ['pint', 'pica', 'pico', 'pipe', 'pixel']],
  ['∫', 'int', ['integral', 'integrate', 'integer', 'interest', 'into']],
  ['∞', 'inf', ['infinity', 'info', 'infty']],
  ['Σ', 'sum', ['sums', 'summary', 'summation']],
  ['Π', 'prod', ['product']],
]

function restoreLongerWords(text: string): string {
  let out = text
  for (const [sign, word, longer] of LONGER_WORDS) {
    out = out.replace(new RegExp(`${sign}_?([A-Za-z]+)`, 'g'), (m, rest: string) => {
      const w = (word + rest).toLowerCase()
      const hit = longer.some((l) => l === w || (rest.length >= 2 && (l.startsWith(w) || w.startsWith(l))))
      return hit ? word + rest : m
    })
  }
  return out
}

// convert even when words are kept as text; `5+-3` stays plus negative three
const SHORTCUT_SYMBOLS: [RegExp, string][] = [
  [/-[/.]\+/g, '∓'],
  [/\+[/.]-/g, '±'],
  [/~/g, '±'],
]

export function flattenPastedText(text: string): string {
  return cleanPastedText(text).replace(/\r\n/g, '\n').replace(/\r/g, '\n').replace(/\n/g, ' ')
}

export function spliceText(
  value: string,
  chunk: string,
  start: number,
  end: number,
): { next: string; cursor: number } {
  const a = Math.max(0, Math.min(start, value.length))
  const b = Math.max(a, Math.min(end, value.length))
  return { next: value.slice(0, a) + chunk + value.slice(b), cursor: a + chunk.length }
}

function replaceTokens(text: string, keepTrailing: boolean, keepWords: boolean): string {
  // a word at the end may still grow into a longer one (`theta` to `thetas`) unless its symbol converts at once
  const swap = (put: string) => (m: string, offset: number, whole: string) =>
    keepTrailing && !AT_ONCE.has(put) && offset + m.length === whole.length && /[A-Za-z]$/.test(m) ? m : put
  let out = keepWords ? text : restoreLongerWords(text)
  for (const [re, put] of keepWords ? SHORTCUT_SYMBOLS : [...WORD_SYMBOLS, ...SHORTCUT_SYMBOLS]) {
    re.lastIndex = 0
    out = out.replace(re, swap(put))
  }
  return out
}

// with a caret, the token right before it is left for the next keystroke to settle;
// a typed `ans` stays a word, so the engine reads the last answer at full precision
export function prettyTokens(text: string, caret?: number, keepWords = false): string {
  if (caret == null) return replaceTokens(text, false, keepWords)
  return replaceTokens(text.slice(0, caret), true, keepWords) + replaceTokens(text.slice(caret), false, keepWords)
}

export function QuickInput({
  value,
  keepWords = false,
  onChange,
  onEnter,
  onUp,
  onDown,
  onPrefixStep,
  onTab,
  example,
  handleRef,
  chain = false,
  squiggle = null,
  completionNames,
  onSelection,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const prefixRef = useRef<HTMLSpanElement>(null)
  const [prefixWidth, setPrefixWidth] = useState(0)
  const onChangeRef = useRef(onChange)
  const onEnterRef = useRef(onEnter)
  const onUpRef = useRef(onUp)
  const onDownRef = useRef(onDown)
  const onPrefixStepRef = useRef(onPrefixStep)
  const onTabRef = useRef(onTab)
  const keepWordsRef = useRef(keepWords)
  const heldRef = useRef('')
  const metaRef = useRef(false)
  const caretPosRef = useRef<CaretRange>({ start: value.length, end: value.length })
  const holdingArrowRef = useRef(false)
  const [caretAtEnd, setCaretAtEnd] = useState(true)
  const undoRef = useRef<Undo | null>(null)
  if (!undoRef.current) undoRef.current = undoStart(value)
  const editKindRef = useRef<EditKind>(null)
  onChangeRef.current = onChange
  onEnterRef.current = onEnter
  onUpRef.current = onUp
  onDownRef.current = onDown
  onPrefixStepRef.current = onPrefixStep
  onTabRef.current = onTab
  keepWordsRef.current = keepWords

  // holds the last highlight while ⌘ or ⌃ is down so a copy still finds it
  const rememberHighlight = (el: HTMLInputElement | null) => {
    const live = inputHighlight(el)
    if (live) heldRef.current = live
    else if (!metaRef.current) heldRef.current = ''
  }

  const rememberCaret = (el: HTMLInputElement | null) => {
    if (!el) return
    const start = el.selectionStart ?? el.value.length
    const end = el.selectionEnd ?? start
    caretPosRef.current = { start, end }
    setCaretAtEnd(start === end && end === el.value.length)
    onSelection?.({ start, end })
  }

  const pinCaret = (el: HTMLInputElement | null = inputRef.current) => {
    if (!el) return
    const max = el.value.length
    const start = Math.min(caretPosRef.current.start, max)
    const end = Math.min(caretPosRef.current.end, max)
    el.setSelectionRange(start, end)
  }

  const { leading: leadingCount, trailing: trailingCount } = inferParens(value)
  const prefix = leadingCount > 0 ? '('.repeat(leadingCount) : ''
  const suffix = trailingCount > 0 ? ')'.repeat(trailingCount) : ''

  useLayoutEffect(() => {
    setPrefixWidth(prefixRef.current?.offsetWidth ?? 0)
  }, [prefix, chain])

  const completion = caretAtEnd ? completionFor(value, completionNames) : ''
  const acceptCompletion = (el: HTMLInputElement) => {
    const next = el.value + completion
    commit(next, next.length)
  }
  // the ghost layer doesn't scroll with the input, so a squiggle only shows while the text fits
  const input = inputRef.current
  const fits = !input || input.scrollWidth <= input.clientWidth + 1
  const mark =
    squiggle && fits && squiggle.end <= value.length && !(completion && squiggle.end === value.length) ? squiggle : null
  const bounded = boundsIn(value).length > 0
  // a word the engine knows is painted from the ghost layer, so the input's own text steps aside
  const words = fits && !bounded ? knownWordSpans(value, completionNames?.functions, completionNames?.ans) : []
  const painted = words.length > 0

  const commit = (raw: string, cursor: number, settle = false, typed = false) => {
    const caret = settle ? undefined : cursor
    const before = prettyTokens(raw.slice(0, cursor), caret, keepWordsRef.current)
    let next = prettyTokens(raw, caret, keepWordsRef.current)
    let pos = Math.min(before.length, next.length)
    const slotted = typed ? afterTyping(next, pos) : null
    if (slotted) ({ text: next, caret: pos } = slotted)
    caretPosRef.current = { start: pos, end: pos }
    onChangeRef.current(next)
    requestAnimationFrame(() => {
      const el = inputRef.current
      if (!el) return
      el.setSelectionRange(pos, pos)
    })
  }

  const applyEdit = (el: HTMLInputElement, edit: Edit) => {
    caretPosRef.current = { start: edit.caret, end: edit.caret }
    if (edit.text !== el.value) {
      onChangeRef.current(edit.text)
      requestAnimationFrame(() => inputRef.current?.setSelectionRange(edit.caret, edit.caret))
    } else el.setSelectionRange(edit.caret, edit.caret)
  }

  // converts a token still waiting at the caret (`2pi` then enter)
  const finishTokens = (el: HTMLInputElement) => {
    if (prettyTokens(el.value, undefined, keepWordsRef.current) === el.value) return
    commit(el.value, el.selectionStart ?? el.value.length, true)
  }

  // every value change is a step, whoever made it (typing, a paste, a history insert, esc, enter)
  useLayoutEffect(() => {
    const u = undoRef.current
    if (!u) return
    const { start, end } = caretPosRef.current
    undoRef.current = recordEdit(u, { value, start: Math.min(start, value.length), end: Math.min(end, value.length) }, editKindRef.current, Date.now())
    editKindRef.current = null
  }, [value])

  const applyUndo = (next: Undo | null) => {
    if (!next) return
    const state: UndoState = next.current
    undoRef.current = next
    caretPosRef.current = { start: state.start, end: state.end }
    onChangeRef.current(state.value)
    requestAnimationFrame(() => inputRef.current?.setSelectionRange(state.start, state.end))
  }

  useLayoutEffect(() => {
    if (!holdingArrowRef.current) return
    pinCaret()
  })

  useEffect(() => {
    const el = inputRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => {
      if (document.activeElement === el) keepEndInView(el)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    const api: QuickInputHandle = {
      insert: (chunk: string, at?: CaretRange) => {
        el.focus()
        const start = at?.start ?? caretPosRef.current.start ?? el.selectionStart ?? el.value.length
        const end = at?.end ?? caretPosRef.current.end ?? el.selectionEnd ?? start
        const { next, cursor } = spliceText(el.value, chunk, start, end)
        commit(next, cursor)
      },
      focus: () => el.focus(),
      setValue: (text: string) => {
        el.focus()
        caretPosRef.current = { start: text.length, end: text.length }
        onChangeRef.current(text)
        requestAnimationFrame(() => {
          el.setSelectionRange(text.length, text.length)
        })
      },
      element: () => el,
      highlighted: () => inputHighlight(el) || heldRef.current,
      caret: () => caretPosRef.current,
    }
    if (handleRef) handleRef.current = api
    const w = nativeWindow()
    if (w) {
      w.__qcalcFocus = () => el.focus()
      // keys typed before the input had focus, buffered by the mac app's boot script
      const buffered = w.__QCALC_KEYS
      if (buffered?.length) {
        const text = (el.value || '') + buffered.join('')
        commit(text, text.length)
        w.__QCALC_KEYS = []
      }
    }
    const timers = [0, 40, 120, 280].map((ms) => window.setTimeout(() => el.focus(), ms))
    const onMeta = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Meta' || e.key === 'Control') metaRef.current = true
      if (e.metaKey || e.ctrlKey) rememberHighlight(el)
    }
    const onMetaUp = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Meta' && e.key !== 'Control') return
      metaRef.current = false
      rememberHighlight(el)
    }
    const onArrow = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') {
        // a ↑/↓ whose keyup landed elsewhere must not keep pinning the caret against ←/→
        holdingArrowRef.current = false
        return
      }
      e.preventDefault()
      if (e.altKey) {
        onPrefixStepRef.current?.(e.key === 'ArrowUp' ? 1 : -1)
        return
      }
      const limit = el.selectionStart === el.selectionEnd && !e.metaKey && !e.ctrlKey && !e.shiftKey
        ? boundKey(el.value, el.selectionStart ?? el.value.length, e.key)
        : null
      if (limit) {
        applyEdit(el, limit)
        return
      }
      holdingArrowRef.current = true
      pinCaret(el)
      if (e.key === 'ArrowUp') onUpRef.current()
      else onDownRef.current()
      pinCaret(el)
      requestAnimationFrame(() => pinCaret(el))
    }
    window.addEventListener('keydown', onMeta, true)
    window.addEventListener('keyup', onMetaUp, true)
    window.addEventListener('keydown', onArrow, true)
    return () => {
      for (const t of timers) window.clearTimeout(t)
      window.removeEventListener('keydown', onMeta, true)
      window.removeEventListener('keyup', onMetaUp, true)
      window.removeEventListener('keydown', onArrow, true)
      if (handleRef) handleRef.current = null
    }
  }, [handleRef])

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const ctrlOnly = e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey
    const key = e.key.toLowerCase()
    if (ctrlOnly && (key === 'd' || key === 'f' || key === 'c')) {
      e.preventDefault()
      return
    }
    // done here too because the mac overlay has no edit menu to route ⌘Z, ⌘A and ⌘X
    const cmd = e.metaKey && !e.ctrlKey && !e.altKey
    if (cmd && key === 'z') {
      e.preventDefault()
      const u = undoRef.current
      if (u) applyUndo(e.shiftKey ? redo(u) : undo(u))
      return
    }
    if (cmd && !e.shiftKey && key === 'a') {
      e.preventDefault()
      e.currentTarget.select()
      return
    }
    if (cmd && !e.shiftKey && key === 'x') {
      const el = e.currentTarget
      const start = el.selectionStart ?? 0
      const end = el.selectionEnd ?? start
      if (end <= start) return
      e.preventDefault()
      copyText(el.value.slice(start, end))
      commit(el.value.slice(0, start) + el.value.slice(end), start)
      return
    }
    if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End') {
      if (undoRef.current) undoRef.current = breakRun(undoRef.current)
    }
    // tab belongs to the completion only while its ghost shows
    const plainKey = !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey
    if (completion && plainKey && (e.key === 'Tab' || e.key === 'ArrowRight')) {
      e.preventDefault()
      acceptCompletion(e.currentTarget)
      return
    }
    // → and tab step through ∫ and Σ limits; `^` and space leave a limit too
    const el = e.currentTarget
    const bare = !e.altKey && !e.metaKey && !e.ctrlKey && (!e.shiftKey || e.key === '^' || e.key === ' ')
    if (bare && el.selectionStart === el.selectionEnd) {
      const at = el.selectionStart ?? el.value.length
      const edit =
        boundKey(el.value, at, e.key) ??
        (!keepWordsRef.current && (e.key === 'Tab' || e.key === 'ArrowRight') ? wordToSign(el.value, at) : null)
      if (edit) {
        e.preventDefault()
        applyEdit(el, edit)
        return
      }
    }
    if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // tab never moves focus out of the input; it cycles the answer's forms
      e.preventDefault()
      onTabRef.current?.(e.shiftKey ? -1 : 1)
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      finishTokens(e.currentTarget)
      onEnterRef.current(e.altKey)
      return
    }
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      return
    }
    // macos webkit maps home/end to page scrolling, not the caret
    if ((e.key === 'Home' || e.key === 'End') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      const to = e.key === 'Home' ? 0 : el.value.length
      const anchor = el.selectionDirection === 'backward' ? (el.selectionEnd ?? to) : (el.selectionStart ?? to)
      if (!e.shiftKey) el.setSelectionRange(to, to)
      else if (to < anchor) el.setSelectionRange(to, anchor, 'backward')
      else el.setSelectionRange(anchor, to, 'forward')
      return
    }
    if (e.key !== 'ArrowRight' || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey) return
    const start = el.selectionStart ?? 0
    const end = el.selectionEnd ?? start
    const filled = autofillParens(el.value, start, end)
    if (!filled) return
    e.preventDefault()
    commit(filled, filled.length)
  }

  const showExample = !value && example != null

  return (
    <div className={bounded ? 'quick-field quick-field-bounds' : 'quick-field'}>
      <div className="quick-ghost" aria-hidden>
        <span ref={prefixRef} className="quick-inferred">
          {prefix}
          {chain ? <span className="quick-chain">ans</span> : null}
        </span>
        {bounded ? (
          <BoundsInputText
            value={value}
            input={inputRef}
            mark={squiggle && squiggle.end <= value.length && !(completion && squiggle.end === value.length) ? squiggle : null}
          />
        ) : (
          <span className={painted ? 'quick-ghost-text quick-ghost-painted' : 'quick-ghost-text'}>
            {paintText(value, mark, words)}
          </span>
        )}
        {completion ? <span className="quick-inferred quick-completion">{completion}</span> : null}
        {suffix ? <span className="quick-inferred">{suffix}</span> : null}
        {showExample && example ? (
          <span key={example.id} className="quick-example">
            {example.text}
          </span>
        ) : null}
      </div>
      <RadicalLayer value={bounded ? '' : value} input={inputRef} inset={prefixWidth} />
      <input
        ref={inputRef}
        className={painted ? 'quick-plain quick-plain-painted' : 'quick-plain'}
        value={value}
        size={1}
        autoFocus
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete="off"
        spellCheck={false}
        placeholder={showExample ? '' : 'Calculate'}
        style={prefixWidth ? { paddingLeft: prefixWidth } : undefined}
        onChange={(e) => {
          const el = e.currentTarget
          const typed = e.nativeEvent as InputEvent
          editKindRef.current = editKind(typed.inputType)
          commit(el.value, el.selectionStart ?? el.value.length, false, typed.inputType === 'insertText')
        }}
        data-completion={completion || undefined}
        onSelect={(e) => {
          if (holdingArrowRef.current) {
            pinCaret(e.currentTarget)
            return
          }
          rememberCaret(e.currentTarget)
          rememberHighlight(e.currentTarget)
        }}
        onBlur={(e) => finishTokens(e.currentTarget)}
        onMouseDown={(e) => {
          if (undoRef.current) undoRef.current = breakRun(undoRef.current)
          rememberCaret(e.currentTarget)
        }}
        onMouseUp={(e) => {
          rememberCaret(e.currentTarget)
          rememberHighlight(e.currentTarget)
        }}
        onKeyUp={(e) => {
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            pinCaret(e.currentTarget)
            holdingArrowRef.current = false
            rememberHighlight(e.currentTarget)
            return
          }
          rememberCaret(e.currentTarget)
          rememberHighlight(e.currentTarget)
        }}
        onPaste={(e) => {
          const text = flattenPastedText(e.clipboardData?.getData('text/plain') ?? '')
          if (!text) return
          e.preventDefault()
          const el = e.currentTarget
          const start = el.selectionStart ?? el.value.length
          const end = el.selectionEnd ?? start
          const raw = el.value.slice(0, start) + text + el.value.slice(end)
          commit(raw, start + text.length)
        }}
        onKeyDown={onKeyDown}
      />
    </div>
  )
}

// the input's text split into plain runs, the squiggle and known words; a word keys on its name and
// count, so its pulse plays once when it's finished and not again as text around it changes
function paintText(value: string, mark: Span | null, words: Span[]): ReactNode[] {
  const cuts = [...words.filter((w) => !mark || w.end <= mark.start || w.start >= mark.end), ...(mark ? [mark] : [])]
  cuts.sort((a, b) => a.start - b.start)
  const out: ReactNode[] = []
  const seen = new Map<string, number>()
  let at = 0
  for (const cut of cuts) {
    if (cut.start > at) out.push(value.slice(at, cut.start))
    const text = value.slice(cut.start, cut.end)
    if (cut === mark) {
      out.push(<span key="squiggle" className="quick-squiggle">{text}</span>)
    } else {
      const n = seen.get(text) ?? 0
      seen.set(text, n + 1)
      out.push(<span key={`${text}#${n}`} className="quick-word">{text}</span>)
    }
    at = cut.end
  }
  if (at < value.length) out.push(value.slice(at))
  return out
}
