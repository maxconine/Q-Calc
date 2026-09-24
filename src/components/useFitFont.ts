import { useLayoutEffect, useRef } from 'react'

// steps the font down a few px before the text gives up and ellipsizes
export function useFitFont<T extends HTMLElement>(text: string, maxDrop = 4) {
  const ref = useRef<T>(null)
  const fitted = useRef<{ el: T | null; text: string }>({ el: null, text: '' })
  useLayoutEffect(() => {
    const el = ref.current
    if (!el || (fitted.current.el === el && fitted.current.text === text)) return
    fitted.current = { el, text }
    el.style.fontSize = ''
    if (!text || el.scrollWidth <= el.clientWidth) return
    const basePx = parseFloat(getComputedStyle(el).fontSize)
    for (let drop = 1; drop <= maxDrop && el.scrollWidth > el.clientWidth; drop++) {
      el.style.fontSize = `${basePx - drop}px`
    }
  })
  return ref
}
