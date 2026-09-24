import { OnOffSetting } from './ChoiceSetting'

export function TypstCopySettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Copy Typst compatible"
      hint="When on, an equation copied from the bar pastes as Typst math."
      value={value}
      onChange={onChange}
    />
  )
}

export function TypstSettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Typst preview"
      hint="When on, the calculation is typeset under the bar."
      value={value}
      onChange={onChange}
    />
  )
}
