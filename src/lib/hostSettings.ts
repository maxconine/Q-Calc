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
  // the running q calc's version, its updates' state, and whether they check and install by themselves
  version: string
  update: UpdateStatus
  autoUpdate: boolean
}

// the shell's update loop, as it last said (update.rs's Status)
export type UpdateState = 'off' | 'idle' | 'checking' | 'current' | 'downloading' | 'ready' | 'installing' | 'failed'
export type UpdateStatus = { state: UpdateState; latest: string; checkedAt: number; error: string }

const UPDATE_STATES: readonly UpdateState[] = ['off', 'idle', 'checking', 'current', 'downloading', 'ready', 'installing', 'failed']
export const NO_UPDATES: UpdateStatus = { state: 'off', latest: '', checkedAt: 0, error: '' }

function updateStatus(raw: unknown, base: UpdateStatus): UpdateStatus {
  if (!raw || typeof raw !== 'object') return base
  const o = raw as Record<string, unknown>
  const state = UPDATE_STATES.includes(o.state as UpdateState) ? (o.state as UpdateState) : base.state
  return {
    state,
    latest: typeof o.latest === 'string' ? o.latest : '',
    checkedAt: typeof o.checkedAt === 'number' && Number.isFinite(o.checkedAt) ? o.checkedAt : 0,
    error: typeof o.error === 'string' ? o.error : '',
  }
}

// "5 minutes ago", for when updates were last checked
function ago(then: number, now: number): string {
  const s = Math.max(0, Math.round(now - then))
  if (s < 60) return 'just now'
  const m = Math.round(s / 60)
  if (m < 60) return m === 1 ? '1 minute ago' : `${m} minutes ago`
  const h = Math.round(m / 60)
  if (h < 24) return h === 1 ? '1 hour ago' : `${h} hours ago`
  const d = Math.round(h / 24)
  return d === 1 ? 'yesterday' : `${d} days ago`
}

// the line under "Software update": what the loop is doing, in words. `now` is unix seconds
export function updateLine(u: UpdateStatus, auto: boolean, now: number): string {
  const checked = u.checkedAt ? ` · checked ${ago(u.checkedAt, now)}` : ''
  switch (u.state) {
    case 'off':
      return 'This copy of Q Calc can’t update itself.'
    case 'idle':
      return auto ? 'Checks for updates shortly after Q Calc starts, then daily.' : 'Not checked yet.'
    case 'checking':
      return 'Checking for updates…'
    case 'current':
      return `Up to date${checked}`
    case 'downloading':
      return `Downloading ${u.latest}…`
    case 'ready':
      return auto ? `${u.latest} is ready. It installs when Q Calc is put away and you’re idle, or now.` : `${u.latest} is ready to install.`
    case 'installing':
      return `Installing ${u.latest}…`
    case 'failed':
      return `Couldn’t update${u.error ? `: ${u.error}` : ''}${checked}`
  }
}

// the mac's 5 minutes
export const DEFAULT_SPOT_SECONDS = 300

export const EMPTY_HOST: HostInfo = {
  hotkeyChord: '',
  hotkeyFailed: false,
  hotkeyRefused: '',
  autostart: false,
  spotSeconds: DEFAULT_SPOT_SECONDS,
  version: '',
  update: NO_UPDATES,
  autoUpdate: true,
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
    version: text('version', base.version),
    update: updateStatus(o.update, base.update),
    autoUpdate: flag('autoUpdate', base.autoUpdate),
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
