// what the windows shell adds to __QCALC_SETTINGS for its settings window; never stored by the page
export type HostInfo = {
  // the show / hide chord the user picked, spelled like the page's keybinds
  hotkeyChord: string
  hotkeyFailed: boolean
  // the chord the shell just couldn't register, so the row can say why nothing changed
  hotkeyRefused: string
  autostart: boolean
  // a moved or widened bar goes back to its usual spot and width once q calc has gone this long unopened; 0 never
  spotSeconds: number
}

// the mac's 5 minutes
export const DEFAULT_SPOT_SECONDS = 300

export const EMPTY_HOST: HostInfo = {
  hotkeyChord: '',
  hotkeyFailed: false,
  hotkeyRefused: '',
  autostart: false,
  spotSeconds: DEFAULT_SPOT_SECONDS,
}

export function hostInfo(raw: unknown, base: HostInfo = EMPTY_HOST): HostInfo {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const text = (key: string, fallback: string) => (typeof o[key] === 'string' ? (o[key] as string) : fallback)
  const flag = (key: string, fallback: boolean) => (typeof o[key] === 'boolean' ? (o[key] as boolean) : fallback)
  const seconds = o.spotSeconds
  return {
    hotkeyChord: text('hotkeyChord', base.hotkeyChord),
    hotkeyFailed: flag('hotkeyFailed', base.hotkeyFailed),
    // only ever about the change just made
    hotkeyRefused: text('hotkeyRefused', ''),
    autostart: flag('autostart', base.autostart),
    spotSeconds: typeof seconds === 'number' && Number.isFinite(seconds) && seconds >= 0 ? Math.round(seconds) : base.spotSeconds,
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

// the mac's choices: never, 1, 5, 15 or 60 minutes
const SPOT_CHOICES: Array<{ id: number; label: string }> = [
  { id: 0, label: 'Never' },
  { id: 60, label: '1 minute' },
  { id: 300, label: '5 minutes' },
  { id: 900, label: '15 minutes' },
  { id: 3600, label: '1 hour' },
]

function spotLabel(seconds: number): string {
  return seconds % 60 === 0 ? `${seconds / 60} minutes` : `${seconds} seconds`
}

// a value set some other way (the settings file, by hand, an earlier version's 30 seconds) still shows
export function spotChoices(current: number): Array<{ id: number; label: string }> {
  return SPOT_CHOICES.some((c) => c.id === current) ? SPOT_CHOICES : [...SPOT_CHOICES, { id: current, label: spotLabel(current) }]
}

// a value set some other way still shows, as on the mac
export function draftChoices(current: number): Array<{ id: number; label: string }> {
  return DRAFT_CHOICES.some((c) => c.id === current) ? DRAFT_CHOICES : [...DRAFT_CHOICES, { id: current, label: `${current} seconds` }]
}
