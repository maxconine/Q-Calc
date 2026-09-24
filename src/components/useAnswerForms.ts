import { useCallback, useRef, useState } from 'react'
import { answerForms, type AnswerView, type FormSource } from '../lib/answerForms'

type Cycle = { key: string; index: number; forms: AnswerView[]; tick: number }

/**
 * Tab through the answer's other forms. `key` names the answer as shown (input, display, exact):
 * any change to it drops back to the answer as the engine gives it.
 */
export function useAnswerForms(key: string, source: FormSource | null) {
  const [cycle, setCycle] = useState<Cycle | null>(null)
  // dropped at once, so retyping the same input later starts from the engine's answer again
  if (cycle && cycle.key !== key) setCycle(null)
  const sourceRef = useRef(source)
  sourceRef.current = source
  const keyRef = useRef(key)
  keyRef.current = key

  const active = cycle && cycle.key === key && cycle.index > 0 ? cycle : null
  const form = active ? (active.forms[active.index - 1] ?? null) : null

  // false when there is nothing to cycle, so the caller can leave tab alone
  const step = useCallback((dir: 1 | -1): boolean => {
    const src = sourceRef.current
    const k = keyRef.current
    const forms = cycle?.key === k ? cycle.forms : src ? answerForms(src) : []
    if (!forms.length) return false
    const at = cycle?.key === k ? cycle.index : 0
    const count = forms.length + 1
    setCycle({ key: k, index: (at + dir + count) % count, forms, tick: (cycle?.tick ?? 0) + 1 })
    return true
  }, [cycle])

  // bumps on every step through this answer's forms, for the crossfade
  const tick = cycle?.key === key ? cycle.tick : 0
  return { form, tick, step }
}
