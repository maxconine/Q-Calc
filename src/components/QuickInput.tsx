import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MutableRefObject } from 'react'
import { autofillParens, inferParens } from '../engine/parens'
import { nativeWindow } from '../lib/bridge'
import { inputHighlight } from '../lib/dom'

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
  ansPlain?: string
  onChange: (text: string) => void
  onEnter: () => void
  onUp: () => boolean
  onDown: () => boolean
  onPrefixStep?: (dir: 1 | -1) => void
  // shown in place of the placeholder while the input is empty; a new `id` restarts its fade
  example?: { text: string; id: number } | null
  handleRef?: MutableRefObject<QuickInputHandle | null>
}

// letters around a token make it part of a word (`pint`, `infinity`); digits before it are a coefficient (`2pi`)
const TOKEN_REPLACEMENTS: [RegExp, string][] = [
  [/(?<![A-Za-z])pi(?![A-Za-z0-9])/gi, 'π'],
  [/(?<![A-Za-z])theta(?![A-Za-z0-9])/gi, 'θ'],
  [/(?<![A-Za-z])infty(?![A-Za-z0-9])/gi, '∞'],
  [/(?<![A-Za-z])inf(?![A-Za-z0-9])/gi, '∞'],
  [/(?<![A-Za-z])cbrt(?![A-Za-z0-9])/gi, '∛'],
  [/(?<![\\A-Za-z])dot(?![A-Za-z])/gi, '*'],
  [/-\+/g, '∓'],
  [/\+-/g, '±'],
  [/~/g, '±'],
]

export function flattenPastedText(text: string): string {
  return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').replace(/\n/g, ' ')
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

function replaceTokens(text: string, ansPlain: string | undefined, keepTrailing: boolean): string {
  // a word token at the end may still grow into a longer word (`pi` to `pint`); symbols convert at once
  const swap = (put: string) => (m: string, offset: number, whole: string) =>
    keepTrailing && offset + m.length === whole.length && /[A-Za-z]$/.test(m) ? m : put
  let out = text
  for (const [re, put] of TOKEN_REPLACEMENTS) {
    re.lastIndex = 0
    out = out.replace(re, swap(put))
  }
  if (ansPlain) out = out.replace(/\bans\b/gi, swap(ansPlain))
  return out
}

// with a caret, the token right before it is left for the next keystroke to settle
export function prettyTokens(text: string, ansPlain?: string, caret?: number): string {
  if (caret == null) return replaceTokens(text, ansPlain, false)
  return replaceTokens(text.slice(0, caret), ansPlain, true) + replaceTokens(text.slice(caret), ansPlain, false)
}

export function QuickInput({ value, ansPlain, onChange, onEnter, onUp, onDown, onPrefixStep, example, handleRef }: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const prefixRef = useRef<HTMLSpanElement>(null)
  const [prefixWidth, setPrefixWidth] = useState(0)
  const onChangeRef = useRef(onChange)
  const onEnterRef = useRef(onEnter)
  const onUpRef = useRef(onUp)
  const onDownRef = useRef(onDown)
  const onPrefixStepRef = useRef(onPrefixStep)
  const ansRef = useRef(ansPlain)
  const heldRef = useRef('')
  const metaRef = useRef(false)
  const caretPosRef = useRef<CaretRange>({ start: value.length, end: value.length })
  const holdingArrowRef = useRef(false)
  onChangeRef.current = onChange
  onEnterRef.current = onEnter
  onUpRef.current = onUp
  onDownRef.current = onDown
  onPrefixStepRef.current = onPrefixStep
  ansRef.current = ansPlain

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
  }, [prefix])

  const commit = (raw: string, cursor: number, settle = false) => {
    const caret = settle ? undefined : cursor
    const before = prettyTokens(raw.slice(0, cursor), ansRef.current, caret)
    const next = prettyTokens(raw, ansRef.current, caret)
    const pos = Math.min(before.length, next.length)
    caretPosRef.current = { start: pos, end: pos }
    onChangeRef.current(next)
    requestAnimationFrame(() => {
      const el = inputRef.current
      if (!el) return
      el.setSelectionRange(pos, pos)
    })
  }

  // converts a token still waiting at the caret (`2pi` then enter)
  const finishTokens = (el: HTMLInputElement) => {
    if (prettyTokens(el.value, ansRef.current) === el.value) return
    commit(el.value, el.selectionStart ?? el.value.length, true)
  }

  useLayoutEffect(() => {
    if (!holdingArrowRef.current) return
    pinCaret()
  })

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
    if (e.key === 'Enter') {
      e.preventDefault()
      finishTokens(e.currentTarget)
      onEnterRef.current()
      return
    }
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      return
    }
    // macos webkit maps home/end to page scrolling, not the caret
    if ((e.key === 'Home' || e.key === 'End') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      const el = e.currentTarget
      const to = e.key === 'Home' ? 0 : el.value.length
      const anchor = el.selectionDirection === 'backward' ? (el.selectionEnd ?? to) : (el.selectionStart ?? to)
      if (!e.shiftKey) el.setSelectionRange(to, to)
      else if (to < anchor) el.setSelectionRange(to, anchor, 'backward')
      else el.setSelectionRange(anchor, to, 'forward')
      return
    }
    if (e.key !== 'ArrowRight' || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey) return
    const el = e.currentTarget
    const start = el.selectionStart ?? 0
    const end = el.selectionEnd ?? start
    const filled = autofillParens(el.value, start, end)
    if (!filled) return
    e.preventDefault()
    commit(filled, filled.length)
  }

  const showExample = !value && example != null

  return (
    <div className="quick-field">
      <div className="quick-ghost" aria-hidden>
        <span ref={prefixRef} className="quick-inferred">
          {prefix}
        </span>
        <span className="quick-ghost-text">{value}</span>
        {suffix ? <span className="quick-inferred">{suffix}</span> : null}
        {showExample && example ? (
          <span key={example.id} className="quick-example">
            {example.text}
          </span>
        ) : null}
      </div>
      <input
        ref={inputRef}
        className="quick-plain"
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
          commit(el.value, el.selectionStart ?? el.value.length)
        }}
        onSelect={(e) => {
          if (holdingArrowRef.current) {
            pinCaret(e.currentTarget)
            return
          }
          rememberCaret(e.currentTarget)
          rememberHighlight(e.currentTarget)
        }}
        onBlur={(e) => finishTokens(e.currentTarget)}
        onMouseDown={(e) => rememberCaret(e.currentTarget)}
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
