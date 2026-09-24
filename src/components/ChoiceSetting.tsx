type Choice<T> = { id: T; label: string }

const ON_OFF: Array<Choice<boolean>> = [
  { id: true, label: 'On' },
  { id: false, label: 'Off' },
]

// one settings section: a title, a line of explanation, and a row of mutually exclusive buttons
export function ChoiceSetting<T extends string | boolean>({
  title,
  hint,
  options,
  value,
  onChange,
}: {
  title: string
  hint: string
  options: Array<Choice<T>>
  value: T
  onChange: (next: T) => void
}) {
  return (
    <section className="unit-settings" aria-label={title}>
      <div className="unit-settings-head">
        <h2>{title}</h2>
      </div>
      <p className="unit-settings-hint">{hint}</p>
      <div className="theme-choices">
        {options.map((option) => (
          <button
            key={String(option.id)}
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

export function OnOffSetting(props: { title: string; hint: string; value: boolean; onChange: (next: boolean) => void }) {
  return <ChoiceSetting {...props} options={ON_OFF} />
}
