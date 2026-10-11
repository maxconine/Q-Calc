import { OnOffSetting } from './ChoiceSetting'

export function SpaceThousandsSettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Spaces group thousands"
      hint="When on, 1 000 000 is one million. When off, a space between two numbers multiplies them: 3 4 is 12."
      value={value}
      onChange={onChange}
    />
  )
}
