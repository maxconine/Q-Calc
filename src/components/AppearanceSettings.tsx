import { THEME_OPTIONS, type Theme } from '../lib/theme'
import { ChoiceSetting } from './ChoiceSetting'

export function AppearanceSettings({ value, onChange }: { value: Theme; onChange: (next: Theme) => void }) {
  return (
    <ChoiceSetting
      title="Appearance"
      hint="Light and dark apply to Q Calc only. System matches this device."
      options={THEME_OPTIONS}
      value={value}
      onChange={onChange}
    />
  )
}
