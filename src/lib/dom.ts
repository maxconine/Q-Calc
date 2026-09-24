import { nativeHandler } from './bridge'

export function copyText(text: string): void {
  if (!text) return
  nativeHandler()?.postMessage({ type: 'copy', text })
  void navigator.clipboard.writeText(text).catch(() => {
    // the fallback textarea takes focus; the input gets it back
    const focused = document.activeElement
    const area = document.createElement('textarea')
    area.value = text
    document.body.appendChild(area)
    area.select()
    document.execCommand('copy')
    area.remove()
    if (focused instanceof HTMLElement) focused.focus()
  })
}

export function inputHighlight(el: EventTarget | null): string {
  if (!(el instanceof HTMLInputElement) && !(el instanceof HTMLTextAreaElement)) return ''
  const start = el.selectionStart ?? 0
  const end = el.selectionEnd ?? 0
  return end > start ? el.value.slice(start, end) : ''
}

export function highlightedText(): string {
  return (
    inputHighlight(document.querySelector('.quick-plain')) ||
    inputHighlight(document.activeElement) ||
    window.getSelection()?.toString() ||
    ''
  )
}

// mousedown handler for controls that must not steal focus from the input
export function keepFocus(e: { preventDefault: () => void }): void {
  e.preventDefault()
}

type Scrolled = Pick<HTMLInputElement, 'selectionStart' | 'selectionEnd' | 'value' | 'scrollWidth' | 'scrollLeft'>

// the input narrows when the answer beside it widens after a keystroke (a closed form lands late),
// and it doesn't scroll its caret back into view on its own; a caret at the end is put back
export function keepEndInView(el: Scrolled): void {
  if (el.selectionStart !== el.selectionEnd || el.selectionEnd !== el.value.length) return
  el.scrollLeft = el.scrollWidth
}
