import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MutableRefObject } from 'react'
import { autofillParens, inferParens } from '../engine/parens'
import { nativeWindow } from '../lib/bridge'

export type CaretRange = { start: number; end: number }

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
  handleRef?: MutableRefObject<QuickInputHandle | null>
}

const TOKEN_REPLACEMENTS: [RegExp, string][] = [
  [/\bpi\b/gi, 'π'],
  [/\btheta\b/gi, 'θ'],
  [/\binfty\b/gi, '∞'],
  [/\binf\b/gi, '∞'],
  [/\bcbrt\b/gi, '∛'],
  [/(?<![\\A-Za-z])dot(?![A-Za-z])/gi, '*'],
]

export function flattenPastedText(text: string): string {
  return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').replace(/\n/g, ' ')
}

/** Insert `chunk` at a caret or selection, replacing the selected range when present. */
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

export function prettyTokens(text: string, ansPlain?: string): string {
  let out = text
  for (const [re, put] of TOKEN_REPLACEMENTS) {
    re.lastIndex = 0
    out = out.replace(re, put)
  }
  if (ansPlain) out = out.replace(/\bans\b/gi, ansPlain)
  return out
}

export function QuickInput({ value, ansPlain, onChange, onEnter, onUp, onDown, handleRef }: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const prefixRef = useRef<HTMLSpanElement>(null)
  const [prefixWidth, setPrefixWidth] = useState(0)
  const onChangeRef = useRef(onChange)
  const onEnterRef = useRef(onEnter)
  const onUpRef = useRef(onUp)
  const onDownRef = useRef(onDown)
  const ansRef = useRef(ansPlain)
  const heldRef = useRef('')
  const metaRef = useRef(false)
  const caretPosRef = useRef<CaretRange>({ start: value.length, end: value.length })
  const holdingArrowRef = useRef(false)
  onChangeRef.current = onChange
  onEnterRef.current = onEnter
  onUpRef.current = onUp
  onDownRef.current = onDown
  ansRef.current = ansPlain

  const readHighlight = (el: HTMLInputElement | null): string => {
    if (!el) return ''
    const start = el.selectionStart ?? 0
    const end = el.selectionEnd ?? 0
    return end > start ? el.value.slice(start, end) : ''
  }

  const rememberHighlight = (el: HTMLInputElement | null) => {
    const live = readHighlight(el)
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

  const commit = (raw: string, cursor: number) => {
    const before = prettyTokens(raw.slice(0, cursor), ansRef.current)
    const next = prettyTokens(raw, ansRef.current)
    const pos = Math.min(before.length, next.length)
    caretPosRef.current = { start: pos, end: pos }
    onChangeRef.current(next)
    requestAnimationFrame(() => {
      const el = inputRef.current
      if (!el) return
      el.setSelectionRange(pos, pos)
    })
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
      highlighted: () => readHighlight(el) || heldRef.current,
      caret: () => caretPosRef.current,
    }
    if (handleRef) handleRef.current = api
    const w = nativeWindow()
    if (w) {
      w.__qcalcFocus = () => el.focus()
      const buffered = w.__QCALC_KEYS
      if (buffered?.length) {
        commit((el.value || '') + buffered.join(''), ((el.value || '') + buffered.join('')).length)
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
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
      e.preventDefault()
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
      onEnterRef.current()
      return
    }
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
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

  return (
    <div className="quick-field">
      <div className="quick-ghost" aria-hidden>
        <span ref={prefixRef} className="quick-inferred">
          {prefix}
        </span>
        <span className="quick-ghost-text">{value}</span>
        {suffix ? <span className="quick-inferred">{suffix}</span> : null}
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
        placeholder="Calculate"
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
