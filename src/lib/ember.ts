// opening ember & frost from the bar
export const EMBER_HINT = '↵ play ember & frost'

// 'ember', 'ember & frost', 'ember and frost', 'frost & ember', 'fire ice', 'fire & ice', 'fire and ice'.
// whole names only, so 'ember2', 'fire' or 'emberandfrost' stay calculator input. symbols may hug the
// words; 'and' needs its spaces
const AND = '(?: ?[&+] ?| and )'
const EMBER_RE = new RegExp(`^(?:ember(?:${AND}frost)?|frost${AND}ember|fire(?:${AND}| )ice)$`, 'i')

export function isEmberCommand(text: string): boolean {
  return EMBER_RE.test(text.trim().replace(/\s+/g, ' '))
}
