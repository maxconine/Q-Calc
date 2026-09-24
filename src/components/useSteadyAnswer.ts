import { useEffect, useRef, useState } from 'react'
import { isEditOf, STEADY_FADE_MS, STEADY_MS } from '../lib/steady'

export type SteadyAnswer = { text: string; fading: boolean }

/**
 * The last real answer, for the slot to show dimmed while `q` is briefly invalid mid-edit.
 * `answer` is '' while there is none and null where a held answer makes no sense (graphs, help).
 * `context` is the evaluation settings: an answer from other settings is not held.
 */
export function useSteadyAnswer(q: string, answer: string | null, context: unknown): SteadyAnswer | null {
  const good = useRef<{ q: string; text: string; context: unknown } | null>(null)
  const [fadedFor, setFadedFor] = useState<string | null>(null)
  const [goneFor, setGoneFor] = useState<string | null>(null)
  if (answer) good.current = { q, text: answer, context }
  else if (answer == null || !q.trim()) good.current = null
  const last = good.current
  const held = answer === '' && last != null && last.context === context && isEditOf(q, last.q)

  useEffect(() => {
    if (!held) return
    const t = window.setTimeout(() => setFadedFor(q), STEADY_MS)
    // once faded it lets go of its width too, so the input isn't left beside an empty gap
    const gone = window.setTimeout(() => setGoneFor(q), STEADY_MS + STEADY_FADE_MS)
    return () => {
      window.clearTimeout(t)
      window.clearTimeout(gone)
    }
  }, [held, q])

  return held && last && goneFor !== q ? { text: last.text, fading: fadedFor === q } : null
}
