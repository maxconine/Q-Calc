import { useCallback, useEffect, useRef, useState } from 'react'
import { FOUR_TWENTY_SETTLE_MS, fourTwentyGate } from '../lib/fourTwenty'

// a little longer than the 2.5s css animation
const SMOKE_MS = 2700

export function useFourTwentySmoke(liveN: number | undefined) {
  const [smoking, setSmoking] = useState(false)
  const timer = useRef(0)
  const armed = useRef(true)

  const trigger = useCallback(() => {
    setSmoking(false)
    window.clearTimeout(timer.current)
    // two frames without the class so the animation restarts
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setSmoking(true)
        timer.current = window.setTimeout(() => setSmoking(false), SMOKE_MS)
      })
    })
  }, [])

  const settle = useCallback(
    (n: number | undefined) => {
      const gate = fourTwentyGate(armed.current, n)
      armed.current = gate.armed
      if (gate.fire) trigger()
    },
    [trigger],
  )

  useEffect(() => {
    const t = window.setTimeout(() => settle(liveN), FOUR_TWENTY_SETTLE_MS)
    return () => window.clearTimeout(t)
  }, [liveN, settle])

  useEffect(() => () => window.clearTimeout(timer.current), [])

  return { smoking, settle }
}
