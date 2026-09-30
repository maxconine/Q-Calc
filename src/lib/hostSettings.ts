// what the windows shell adds to __QCALC_SETTINGS for its settings window; never stored by the page
export type HostInfo = {
  // the show / hide chord the user picked, spelled like the page's keybinds
  hotkeyChord: string
  hotkeyFailed: boolean
  // the chord the shell just couldn't register, so the row can say why nothing changed
  hotkeyRefused: string
  autostart: boolean
}

export const EMPTY_HOST: HostInfo = { hotkeyChord: '', hotkeyFailed: false, hotkeyRefused: '', autostart: false }

export function hostInfo(raw: unknown, base: HostInfo = EMPTY_HOST): HostInfo {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const text = (key: string, fallback: string) => (typeof o[key] === 'string' ? (o[key] as string) : fallback)
  const flag = (key: string, fallback: boolean) => (typeof o[key] === 'boolean' ? (o[key] as boolean) : fallback)
  return {
    hotkeyChord: text('hotkeyChord', base.hotkeyChord),
    hotkeyFailed: flag('hotkeyFailed', base.hotkeyFailed),
    // only ever about the change just made
    hotkeyRefused: text('hotkeyRefused', ''),
    autostart: flag('autostart', base.autostart),
  }
}

// '' when the shortcut is fine
export function hotkeyNote(host: HostInfo): string {
  if (host.hotkeyRefused) return `${host.hotkeyRefused} is in use by another app`
  if (host.hotkeyFailed) return 'Your show / hide shortcut is in use by another app'
  return ''
}

const DRAFT_CHOICES: Array<{ id: number; label: string }> = [
  { id: 0, label: 'Don’t keep' },
  { id: 30, label: '30 seconds' },
  { id: 60, label: '1 minute' },
  { id: 120, label: '2 minutes' },
  { id: 300, label: '5 minutes' },
]

// a value set some other way still shows, as on the mac
export function draftChoices(current: number): Array<{ id: number; label: string }> {
  return DRAFT_CHOICES.some((c) => c.id === current) ? DRAFT_CHOICES : [...DRAFT_CHOICES, { id: current, label: `${current} seconds` }]
}
