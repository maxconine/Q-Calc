// `hello` gets a hi back, where an answer would go. not `hi` or `hey`, which could be someone's variable
const GREETING = /^(?:hello|hello there|hiya|howdy)\s*[!.]*$/i

export const GREETING_REPLY = 'hi 👋'

export function isGreeting(text: string): boolean {
  return GREETING.test(text.trim())
}
