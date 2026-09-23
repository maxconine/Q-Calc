import { useEffect, useRef, type RefObject } from 'react'

type TapeWheel = {
  isOpen: () => boolean
  hasHistory: () => boolean
  open: () => void
  close: () => void
}

// scrolling up over the overlay opens the history tape; scrolling down past its end closes it
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
    const onWheel = (e: WheelEvent) => {
      if (e.target instanceof Element && e.target.closest('.graph')) return
      const { isOpen, hasHistory, open, close } = handlersRef.current
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
