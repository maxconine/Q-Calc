import { useMemo } from 'react'
import { chordLabel, KEY_ACTIONS, keybindFor, type KeyAction, type Keybinds } from '../lib/keybinds'
import { isWindowsHost } from '../lib/platform'

// each action's chosen key as the host writes it; '' for one left without a key
export function useKeyLabels(binds: Keybinds): Record<KeyAction, string> {
  return useMemo(() => {
    const windows = isWindowsHost()
    return Object.fromEntries(KEY_ACTIONS.map(({ id }) => [id, chordLabel(keybindFor(binds, id, windows), windows)])) as Record<
      KeyAction,
      string
    >
  }, [binds])
}
