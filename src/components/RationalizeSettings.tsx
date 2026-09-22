export function RationalizeSettings({
  value,
  onChange,
}: {
  value: boolean
  onChange: (next: boolean) => void
}) {
  return (
    <section className="unit-settings" aria-label="Rationalize denominators">
      <div className="unit-settings-head">
        <h2>Rationalize denominators</h2>
      </div>
      <p className="unit-settings-hint">
        When on, exact answers move square roots out of the denominator: 5/sqrt(41) becomes 5sqrt(41)/41.
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
