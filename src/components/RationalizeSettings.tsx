import { OnOffSetting } from './ChoiceSetting'

export function RationalizeSettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Rationalize denominators"
      hint="When on, exact answers move square roots out of the denominator: 5/sqrt(41) becomes 5sqrt(41)/41."
      value={value}
      onChange={onChange}
    />
  )
}
