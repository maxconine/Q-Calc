import { useCallback, useEffect, useRef, useState } from 'react'
import { SIXTY_NINE_SETTLE_MS, sixtyNineGate } from '../lib/sixtyNine'

// a little longer than the 2.2s css animation
const FOLD_MS = 2400

export function useSixtyNineFold(liveN: number | undefined) {
  const [folding, setFolding] = useState(false)
  const timer = useRef(0)
  const armed = useRef(true)

  const trigger = useCallback(() => {
    setFolding(false)
    window.clearTimeout(timer.current)
    // two frames without the class so the animation restarts
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setFolding(true)
        timer.current = window.setTimeout(() => setFolding(false), FOLD_MS)
      })
    })
  }, [])

  const settle = useCallback(
    (n: number | undefined) => {
      const gate = sixtyNineGate(armed.current, n)
      armed.current = gate.armed
      if (gate.fire) trigger()
    },
    [trigger],
  )

  useEffect(() => {
    const t = window.setTimeout(() => settle(liveN), SIXTY_NINE_SETTLE_MS)
    return () => window.clearTimeout(t)
  }, [liveN, settle])

  useEffect(() => () => window.clearTimeout(timer.current), [])

  return { folding, settle }
}
