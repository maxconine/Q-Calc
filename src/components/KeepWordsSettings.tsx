export function KeepWordsSettings({
  value,
  onChange,
}: {
  value: boolean
  onChange: (next: boolean) => void
}) {
  return (
    <section className="unit-settings" aria-label="Keep typed words as text">
      <div className="unit-settings-head">
        <h2>Keep typed words as text</h2>
      </div>
      <p className="unit-settings-hint">
        When off, words turn into symbols as you type: sqrt becomes √ and pi becomes π. When on, they stay as typed. +- still becomes ±.
      </p>
      <div className="theme-choices">
        <button type="button" aria-pressed={value} className={value ? 'active' : ''} onClick={() => onChange(true)}>
          On
        </button>
        <button type="button" aria-pressed={!value} className={!value ? 'active' : ''} onClick={() => onChange(false)}>
          Off
        </button>
      </div>
    </section>
  )
}
