import { useCallback, useEffect, useRef, useState } from 'react'
import { SIXTY_SEVEN_SETTLE_MS, sixtySevenGate } from '../lib/sixtySeven'

// a little longer than the 2.6s css animation
const SHAKE_MS = 2700

export function useSixtySevenArms(liveN: number | undefined) {
  const [shaking, setShaking] = useState(false)
  const timer = useRef(0)
  const armed = useRef(true)

  const trigger = useCallback(() => {
    setShaking(false)
    window.clearTimeout(timer.current)
    // two frames without the class so the animation restarts
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setShaking(true)
        timer.current = window.setTimeout(() => setShaking(false), SHAKE_MS)
      })
    })
  }, [])

  const settle = useCallback(
    (n: number | undefined) => {
      const gate = sixtySevenGate(armed.current, n)
      armed.current = gate.armed
      if (gate.fire) trigger()
    },
    [trigger],
  )

  useEffect(() => {
    const t = window.setTimeout(() => settle(liveN), SIXTY_SEVEN_SETTLE_MS)
    return () => window.clearTimeout(t)
  }, [liveN, settle])

  useEffect(() => () => window.clearTimeout(timer.current), [])

  return { shaking, settle }
}
