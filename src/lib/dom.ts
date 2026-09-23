import { nativeHandler } from './bridge'

export function copyText(text: string): void {
  if (!text) return
  nativeHandler()?.postMessage({ type: 'copy', text })
  void navigator.clipboard.writeText(text).catch(() => {
    const el = document.createElement('textarea')
    el.value = text
    document.body.appendChild(el)
    el.select()
    document.execCommand('copy')
    el.remove()
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
