import { useEffect, useRef, type RefObject } from 'react'

type TapeWheel = {
  isOpen: () => boolean
  hasHistory: () => boolean
  open: () => void
  close: () => void
  // the input, when its expression is too long to show; up and down scroll through it there
  longInput?: () => HTMLInputElement | null
}

// a scroll that coasts on past either end of a long input is still the same gesture, so it doesn't open the tape
const SETTLE_MS = 350

// scroll up over the overlay to open the history tape; scroll down past its end to close it. over a long input,
// up and down move through the expression first (up toward its start), and only past its start does up open the tape
export function useTapeWheel(
  rootRef: RefObject<HTMLElement | null>,
  tapeRef: RefObject<HTMLElement | null>,
  handlers: TapeWheel,
) {
  const handlersRef = useRef(handlers)
  handlersRef.current = handlers

  useEffect(() => {
    const root = rootRef.current
    if (!root) return
    let lastInputScroll = -Infinity
    const onWheel = (e: WheelEvent) => {
      if (e.target instanceof Element && e.target.closest('.graph')) return
      // a sideways two-finger swipe scrolls a long input, so it's left to the browser
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return
      const { isOpen, hasHistory, open, close, longInput } = handlersRef.current
      const input = longInput?.()
      if (input && e.target instanceof Element && e.target.closest('.composer')) {
        const max = input.scrollWidth - input.clientWidth
        const atEnd = e.deltaY < 0 ? input.scrollLeft <= 0 : input.scrollLeft >= max - 1
        const now = performance.now()
        if (!atEnd) {
          e.preventDefault()
          input.scrollLeft = Math.max(0, Math.min(max, input.scrollLeft + e.deltaY))
          lastInputScroll = now
          return
        }
        if (now - lastInputScroll < SETTLE_MS) {
          e.preventDefault()
          lastInputScroll = now
          return
        }
      }
      const tape = tapeRef.current
      const overTape = Boolean(tape && e.target instanceof Node && tape.contains(e.target))

      if (e.deltaY < 0) {
        if (!hasHistory()) {
          if (!overTape) e.preventDefault()
          return
        }
        if (!isOpen()) {
          e.preventDefault()
          open()
          return
        }
        if (!overTape) {
          e.preventDefault()
          tape?.scrollBy({ top: e.deltaY })
        }
        return
      }

      if (!isOpen() || !tape) {
        e.preventDefault()
        return
      }
      if (tape.scrollTop + tape.clientHeight >= tape.scrollHeight - 1) {
        e.preventDefault()
        close()
        return
      }
      if (!overTape) {
        e.preventDefault()
        tape.scrollTop += e.deltaY
      }
    }
    root.addEventListener('wheel', onWheel, { passive: false })
    return () => root.removeEventListener('wheel', onWheel)
  }, [rootRef, tapeRef])
}
