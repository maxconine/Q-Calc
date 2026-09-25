// what the windows shell adds to __QCALC_SETTINGS for its settings window; never stored by the page
export type HotKeyChoice = { id: string; title: string }

export type HostInfo = {
  hotkeyId: string
  hotkeyFailed: boolean
  // the preset the shell just couldn't register, so the row can say why nothing changed
  hotkeyRefused: string
  hotkeys: HotKeyChoice[]
  autostart: boolean
}

export const EMPTY_HOST: HostInfo = { hotkeyId: '', hotkeyFailed: false, hotkeyRefused: '', hotkeys: [], autostart: false }

function choices(raw: unknown): HotKeyChoice[] | null {
  if (!Array.isArray(raw)) return null
  return raw.flatMap((c: unknown) => {
    const { id, title } = (c ?? {}) as Partial<HotKeyChoice>
    return typeof id === 'string' && typeof title === 'string' && id && title ? [{ id, title }] : []
  })
}

export function hostInfo(raw: unknown, base: HostInfo = EMPTY_HOST): HostInfo {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>
  const text = (key: string, fallback: string) => (typeof o[key] === 'string' ? (o[key] as string) : fallback)
  const flag = (key: string, fallback: boolean) => (typeof o[key] === 'boolean' ? (o[key] as boolean) : fallback)
  return {
    hotkeyId: text('hotkeyId', base.hotkeyId),
    hotkeyFailed: flag('hotkeyFailed', base.hotkeyFailed),
    // only ever about the change just made
    hotkeyRefused: text('hotkeyRefused', ''),
    hotkeys: choices(o.hotkeys) ?? base.hotkeys,
    autostart: flag('autostart', base.autostart),
  }
}

export function hotkeyNote(host: HostInfo): string {
  if (host.hotkeyRefused) return `${host.hotkeyRefused} is in use by another app`
  if (host.hotkeyFailed) return 'Your shortcut is in use by another app'
  return 'Shows and hides Q Calc from anywhere'
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
