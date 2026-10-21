// the windows shell's sheet windows (sheet.html): which sheet a window shows, from the query the shell opened it on
export type SheetWhich = { kind: 'periodic' } | { kind: 'identities'; id: string }

export function sheetFromSearch(search: string): SheetWhich | null {
  const params = new URLSearchParams(search)
  const kind = params.get('kind')
  if (kind === 'periodic') return { kind }
  const id = params.get('id')
  if (kind === 'identities' && id) return { kind, id }
  return null
}

// esc and ctrl+w close the sheet's own window, when it has the keys at all (it never takes them by being clicked)
export function closesSheet(e: { key: string; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }): boolean {
  if (e.key === 'Escape') return true
  return e.key.toLowerCase() === 'w' && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey
}

// the table is laid out at one size and scaled to fit the window, keeping its shape, as the mac's is
export function fitScale(natural: { width: number; height: number }, room: { width: number; height: number }): number {
  if (natural.width <= 0 || natural.height <= 0) return 1
  return Math.max(0.4, Math.min(room.width / natural.width, room.height / natural.height))
}
