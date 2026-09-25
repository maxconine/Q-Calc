import { useEffect, useState } from 'react'
import { MAX_SIG_FIGS, MIN_SIG_FIGS } from '../engine/format'
import { nativeHandler } from '../lib/bridge'
import { draftChoices, hostInfo, hotkeyNote, type HostInfo } from '../lib/hostSettings'
import { hostKeys } from '../lib/platform'
import { mergeSettings, settingsEqual, type Settings } from '../lib/settings'
import { calcWindow, loadSettings } from '../lib/storage'
import { applyTheme } from '../lib/theme'
import { AppearanceSettings } from './AppearanceSettings'
import { ChoiceSetting, OnOffSetting } from './ChoiceSetting'
import { HistoryInsertSettings } from './HistoryInsertSettings'
import { KeepWordsSettings } from './KeepWordsSettings'
import { RationalizeSettings } from './RationalizeSettings'
import { TypstCopySettings, TypstSettings } from './TypstSettings'
import { UnitSettings } from './UnitSettings'
import './SettingsPage.css'

const SIG_FIG_CHOICES = Array.from({ length: MAX_SIG_FIGS - MIN_SIG_FIGS + 1 }, (_, i) => {
  const n = MIN_SIG_FIGS + i
  return { id: String(n), label: String(n) }
})

function SelectSetting({
  title,
  hint,
  options,
  value,
  onChange,
}: {
  title: string
  hint: string
  options: Array<{ id: string; label: string }>
  value: string
  onChange: (next: string) => void
}) {
  return (
    <section className="unit-settings" aria-label={title}>
      <div className="unit-settings-head">
        <h2>{title}</h2>
      </div>
      <p className="unit-settings-hint">{hint}</p>
      <select className="setting-select" aria-label={title} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </section>
  )
}

function post(message: Record<string, unknown>): void {
  nativeHandler()?.postMessage(message)
}

// the windows shell's settings window: every row the mac settings window has, in the page's own controls
export function SettingsPage() {
  const [settings, setSettings] = useState<Settings>(loadSettings)
  const [host, setHost] = useState<HostInfo>(() => hostInfo(calcWindow().__QCALC_SETTINGS))
  const set = <K extends keyof Settings>(key: K) => (value: Settings[K]) => setSettings((s) => ({ ...s, [key]: value }))

  useEffect(() => {
    calcWindow().__qcalcApplySettings = (partial) => {
      setSettings((prev) => {
        const next = mergeSettings(partial, prev)
        return settingsEqual(next, prev) ? prev : next
      })
      setHost((prev) => hostInfo(partial, prev))
    }
  }, [])

  useEffect(() => {
    post({ type: 'settings', ...settings })
  }, [settings])

  useEffect(() => {
    applyTheme(settings.theme)
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      if (settings.theme === 'system') applyTheme('system')
    }
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [settings.theme])

  return (
    <main className="settings-page">
      <SelectSetting
        title="Keyboard shortcut"
        hint={hotkeyNote(host)}
        options={host.hotkeys.map((k) => ({ id: k.id, label: k.title }))}
        value={host.hotkeyId}
        onChange={(id) => post({ type: 'hotkey', id })}
      />
      <OnOffSetting
        title="Open at login"
        hint="Start Q Calc in the tray when you sign in."
        value={host.autostart}
        onChange={(on) => post({ type: 'autostart', on })}
      />
      <AppearanceSettings value={settings.theme} onChange={set('theme')} />
      <ChoiceSetting
        title="Show history"
        hint="Recent means the last 5 minutes."
        options={[
          { id: 'recent', label: 'Recent' },
          { id: 'always', label: 'Always' },
          { id: 'arrow', label: 'Only on ↑' },
        ]}
        value={settings.historyShow}
        onChange={set('historyShow')}
      />
      <HistoryInsertSettings value={settings.historyInsert} onChange={set('historyInsert')} />
      <SelectSetting
        title="Keep unfinished input"
        hint="How long a half-typed calculation waits for you after Q Calc hides."
        options={draftChoices(settings.draftSeconds).map((c) => ({ id: String(c.id), label: c.label }))}
        value={String(settings.draftSeconds)}
        onChange={(v) => set('draftSeconds')(Number(v))}
      />

      <h2 className="settings-heading">Answers</h2>
      <ChoiceSetting
        title="Angles"
        hint={hostKeys('Switch with ⌃D')}
        options={[
          { id: 'deg', label: 'Degrees' },
          { id: 'rad', label: 'Radians' },
        ]}
        value={settings.angleMode}
        onChange={set('angleMode')}
      />
      <ChoiceSetting
        title="Answer form"
        hint="The form answers show in."
        options={[
          { id: 'exact', label: 'Exact' },
          { id: 'approx', label: 'Approximate' },
        ]}
        value={settings.answerForm}
        onChange={set('answerForm')}
      />
      <OnOffSetting
        title="Fractions"
        hint={hostKeys('Show answers as fractions · ⌃F')}
        value={settings.fractionMode}
        onChange={set('fractionMode')}
      />
      <RationalizeSettings value={settings.rationalize} onChange={set('rationalize')} />
      <KeepWordsSettings value={settings.keepWords} onChange={set('keepWords')} />
      <TypstSettings value={settings.typstPreview} onChange={set('typstPreview')} />
      <TypstCopySettings value={settings.typstCopy} onChange={set('typstCopy')} />

      <h2 className="settings-heading">Significant figures</h2>
      <SelectSetting
        title="Digits shown"
        hint="Answers round to this many significant figures."
        options={SIG_FIG_CHOICES}
        value={String(settings.sigFigs)}
        onChange={(v) => set('sigFigs')(Number(v))}
      />
      <OnOffSetting
        title="Propagate from input"
        hint={hostKeys('Match the precision you typed · ⌃S')}
        value={settings.sigFigMode}
        onChange={set('sigFigMode')}
      />

      <h2 className="settings-heading">Units</h2>
      <UnitSettings value={settings.defaultUnits} onChange={set('defaultUnits')} />
    </main>
  )
}
