import { describe, expect, it } from 'vitest'
import type { NativeWindow } from './bridge'
import { SMOKE_ROOM, openSmokeRoom, wideSmokeRoom } from './smokeRoom'

function slot(selector: string | null) {
  return { querySelector: (s: string) => (selector && s === selector ? ({} as Element) : null) }
}

describe('wideSmokeRoom', () => {
  it('needs a native host that says it can grow sideways', () => {
    expect(wideSmokeRoom({ __QCALC_NATIVE: true, __QCALC_SMOKE_ROOM: true } as NativeWindow)).toBe(true)
  })

  it('keeps the old room for hosts that do not say so, and for the web page', () => {
    expect(wideSmokeRoom({ __QCALC_NATIVE: true } as NativeWindow)).toBe(false)
    expect(wideSmokeRoom({ __QCALC_SMOKE_ROOM: true } as NativeWindow)).toBe(false)
    expect(wideSmokeRoom({} as NativeWindow)).toBe(false)
    expect(wideSmokeRoom(undefined)).toBe(false)
  })
})

describe('openSmokeRoom', () => {
  it('asks for the room only while the wide room is laid out', () => {
    expect(openSmokeRoom(slot('.four-twenty-room.wide'))).toEqual(SMOKE_ROOM)
    expect(openSmokeRoom(slot(null))).toBeUndefined()
  })
})
