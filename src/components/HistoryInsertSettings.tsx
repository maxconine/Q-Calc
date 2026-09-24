import { HISTORY_INSERT_OPTIONS, type HistoryInsert } from '../lib/answer'
import { ChoiceSetting } from './ChoiceSetting'

export function HistoryInsertSettings({ value, onChange }: { value: HistoryInsert; onChange: (next: HistoryInsert) => void }) {
  return (
    <ChoiceSetting
      title="History"
      hint="When you press Enter on a highlighted history row, insert the original expression or just the answer. Clicking an expression or an answer always inserts that side."
      options={HISTORY_INSERT_OPTIONS}
      value={value}
      onChange={onChange}
    />
  )
}
