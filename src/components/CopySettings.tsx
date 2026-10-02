import { OnOffSetting } from './ChoiceSetting'

export function CopyUnitlessSettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Copy answers without units"
      hint="When on, a copied answer is just the number: 15 m² copies as 15."
      value={value}
      onChange={onChange}
    />
  )
}
