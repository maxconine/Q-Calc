import { THEME_OPTIONS, type Theme } from '../lib/theme'

export function AppearanceSettings({
  value,
  onChange,
}: {
  value: Theme
  onChange: (next: Theme) => void
}) {
  return (
    <section className="unit-settings" aria-label="Appearance">
      <div className="unit-settings-head">
        <h2>Appearance</h2>
      </div>
      <p className="unit-settings-hint">Light and dark apply to Q Calc only. System matches this device.</p>
      <div className="theme-choices">
        {THEME_OPTIONS.map((option) => (
          <button
            key={option.id}
            type="button"
            aria-pressed={value === option.id}
            className={value === option.id ? 'active' : ''}
            onClick={() => onChange(option.id)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </section>
  )
}
