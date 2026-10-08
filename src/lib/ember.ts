// opening ember & frost from the bar
export const EMBER_HINT = '↵ play ember & frost'

export function isEmberCommand(text: string): boolean {
  const t = text.trim().replace(/\s+/g, ' ')
  return /^(ember( ?(&|and|\+|n) ?frost)?|frost ?(&|and|\+|n) ?ember|fire ?(&|and|\+|n)? ?ice)$/i.test(t)
}
