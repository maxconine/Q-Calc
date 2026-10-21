import { chordLabel } from './keybinds'

// what the windows shell adds to __QCALC_SETTINGS for its settings window; never stored by the page
export type HostInfo = {
  // the show / hide chord the user picked, spelled like the page's keybinds
  hotkeyChord: string
  hotkeyFailed: boolean
  // the chord the shell just couldn't register, so the row can say why nothing changed
  hotkeyRefused: string
  // the chord that's taken, refused just now or failed at launch, so the page can ask who holds it
  hotkeyTaken: string
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
  hotkeyTaken: '',
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
    hotkeyTaken: text('hotkeyTaken', ''),
    autostart: flag('autostart', base.autostart),
    spotSeconds: typeof seconds === 'number' && Number.isFinite(seconds) && seconds >= 0 ? Math.round(seconds) : base.spotSeconds,
    version: text('version', base.version),
    update: updateStatus(o.update, base.update),
    autoUpdate: flag('autoUpdate', base.autoUpdate),
  }
}

// the shell's answer when the settings page tries a chord: free, or taken and by whom it guesses
export type HotkeyVerdict = {
  chord: string
  // false when no shortcut can be this chord
  usable: boolean
  free: boolean
  // an id from HOTKEY_OWNERS, or 'windows'; '' when the shell can't tell
  owner: string
  // its own settings name the chord; otherwise the owner is a guess
  sure: boolean
  // a chord that's free right now
  suggest: string
}

export function hotkeyVerdict(raw: unknown): HotkeyVerdict | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  if (typeof o.chord !== 'string' || typeof o.free !== 'boolean') return null
  const text = (v: unknown) => (typeof v === 'string' ? v : '')
  return { chord: o.chord, usable: o.usable !== false, free: o.free, owner: text(o.owner), sure: o.sure === true, suggest: text(o.suggest) }
}

// windows won't say which app registered a shortcut, so the shell guesses from the launchers that are running
export const HOTKEY_OWNERS: Record<string, { name: string; fix: string }> = {
  'powertoys-run': { name: 'PowerToys Run', fix: 'PowerToys → PowerToys Run → Activation shortcut' },
  'command-palette': { name: 'Command Palette', fix: 'PowerToys → Command Palette → Activation shortcut' },
  'flow-launcher': { name: 'Flow Launcher', fix: 'Flow Launcher’s settings → Hotkey' },
  wox: { name: 'Wox', fix: 'Wox’s settings → Hotkey' },
  raycast: { name: 'Raycast', fix: 'Raycast’s settings' },
  keypirinha: { name: 'Keypirinha', fix: 'Keypirinha’s configuration (hotkey_run)' },
}

// "Alt+Space is used by PowerToys Run. Change it in …, or pick another here. Ctrl+Alt+Space is free."
export function conflictText(v: HotkeyVerdict): string {
  const key = chordLabel(v.chord, true)
  const free = v.suggest ? ` ${chordLabel(v.suggest, true)} is free.` : ''
  if (!v.usable) return `Q Calc can’t use ${key} as a shortcut. Pick another.${free}`
  const owner = Object.hasOwn(HOTKEY_OWNERS, v.owner) ? HOTKEY_OWNERS[v.owner] : null
  if (owner) {
    const who = v.sure ? `${key} is used by ${owner.name}.` : `${key} is in use, probably by ${owner.name}.`
    return `${who} Change it in ${owner.fix}, or pick another here.${free}`
  }
  if (v.owner === 'windows') return `${key} is in use, probably by Windows. Pick another.${free}`
  return `${key} is in use by another app. Pick another.${free}`
}

// '' when the shortcut is fine. with the shell's verdict on the taken chord, the note names who holds it
export function hotkeyNote(host: HostInfo, verdict?: HotkeyVerdict | null): string {
  if (verdict && !verdict.free && host.hotkeyTaken && verdict.chord === host.hotkeyTaken) {
    // a launch that fell back opens with another chord meanwhile
    const fellBack = host.hotkeyFailed && !host.hotkeyRefused && host.hotkeyChord && host.hotkeyChord !== host.hotkeyTaken
    return conflictText(verdict) + (fellBack ? ` ${chordLabel(host.hotkeyChord, true)} opens Q Calc for now.` : '')
  }
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
