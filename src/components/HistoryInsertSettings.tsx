import { HISTORY_INSERT_OPTIONS, type HistoryInsert } from '../lib/answer'

export function HistoryInsertSettings({
  value,
  onChange,
}: {
  value: HistoryInsert
  onChange: (next: HistoryInsert) => void
}) {
  return (
    <section className="unit-settings" aria-label="History">
      <div className="unit-settings-head">
        <h2>History</h2>
      </div>
      <p className="unit-settings-hint">
        When you press Enter on a highlighted history row, insert the original expression or just the answer. Clicking an expression or an answer always inserts that side.
      </p>
      <div className="theme-choices">
        {HISTORY_INSERT_OPTIONS.map((option) => (
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
