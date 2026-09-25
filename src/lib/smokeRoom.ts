import type { NativeWindow } from './bridge'

// room the host adds around the bar while the 420 smoke plays, in css px; the bar keeps its size and spot
export type SmokeRoom = { top: number; side: number; bottom: number }

// keep in step with the .four-twenty-room.wide rules in index.css
export const SMOKE_ROOM: SmokeRoom = { top: 160, side: 100, bottom: 64 }

// only a host that says it can grow the window sideways gets the wide room; others keep the old room above
export function wideSmokeRoom(w: NativeWindow | undefined): boolean {
  return w?.__QCALC_NATIVE === true && w.__QCALC_SMOKE_ROOM === true
}

// what the size message carries: the room, while the page has it laid out
export function openSmokeRoom(slot: Pick<ParentNode, 'querySelector'>): SmokeRoom | undefined {
  return slot.querySelector('.four-twenty-room.wide') ? SMOKE_ROOM : undefined
}
