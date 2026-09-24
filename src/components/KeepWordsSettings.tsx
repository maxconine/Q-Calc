import { OnOffSetting } from './ChoiceSetting'

export function KeepWordsSettings({ value, onChange }: { value: boolean; onChange: (next: boolean) => void }) {
  return (
    <OnOffSetting
      title="Keep typed words as text"
      hint="When off, words turn into symbols as you type: sqrt becomes √ and pi becomes π. When on, they stay as typed. +- still becomes ±."
      value={value}
      onChange={onChange}
    />
  )
}
