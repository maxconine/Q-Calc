import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MutableRefObject } from 'react'
import { autofillParens, inferParens } from '../engine/parens'
import { nativeWindow } from '../lib/bridge'

export interface QuickInputHandle {
  insert: (chunk: string) => void
  focus: () => void
  setValue: (text: string) => void
  element: () => HTMLInputElement | null
  highlighted: () => string
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
]

function prettyTokens(text: string, ansPlain?: string): string {
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

  const { leading: leadingCount, trailing: trailingCount } = inferParens(value)
  const prefix = leadingCount > 0 ? '('.repeat(leadingCount) : ''
  const suffix = trailingCount > 0 ? ')'.repeat(trailingCount) : ''

  useLayoutEffect(() => {
    setPrefixWidth(prefixRef.current?.offsetWidth ?? 0)
  }, [prefix])

  const commit = (raw: string, cursor: number) => {
    const before = prettyTokens(raw.slice(0, cursor), ansRef.current)
    const next = prettyTokens(raw, ansRef.current)
    onChangeRef.current(next)
    requestAnimationFrame(() => {
      const el = inputRef.current
      if (!el) return
      const pos = Math.min(before.length, next.length)
      el.setSelectionRange(pos, pos)
    })
  }

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    const api: QuickInputHandle = {
      insert: (chunk: string) => {
        el.focus()
        const start = el.selectionStart ?? el.value.length
        const end = el.selectionEnd ?? start
        const raw = el.value.slice(0, start) + chunk + el.value.slice(end)
        commit(raw, start + chunk.length)
      },
      focus: () => el.focus(),
      setValue: (text: string) => {
        el.focus()
        onChangeRef.current(text)
        requestAnimationFrame(() => {
          el.setSelectionRange(text.length, text.length)
        })
      },
      element: () => el,
      highlighted: () => readHighlight(el) || heldRef.current,
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
    window.addEventListener('keydown', onMeta, true)
    window.addEventListener('keyup', onMetaUp, true)
    return () => {
      for (const t of timers) window.clearTimeout(t)
      window.removeEventListener('keydown', onMeta, true)
      window.removeEventListener('keyup', onMetaUp, true)
      if (handleRef) handleRef.current = null
    }
  }, [handleRef])

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const ctrlOnly = e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey
    const key = e.key.toLowerCase()
    if (ctrlOnly && (key === 'd' || key === 'f')) {
      e.preventDefault()
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      onEnterRef.current()
      return
    }
    if (e.key === 'ArrowUp' && onUpRef.current()) {
      e.preventDefault()
      return
    }
    if (e.key === 'ArrowDown' && onDownRef.current()) {
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
        onSelect={(e) => rememberHighlight(e.currentTarget)}
        onMouseUp={(e) => rememberHighlight(e.currentTarget)}
        onKeyUp={(e) => rememberHighlight(e.currentTarget)}
        onKeyDown={onKeyDown}
      />
    </div>
  )
}
