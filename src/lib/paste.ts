// text copied out of PDFs, web pages and Word carries look-alike characters the engine can't read.
// only swaps that can't change a meaning: en and em dashes stay, since `5–10` is a range as often as a minus
const SPACES = /[  -   　]/g
const INVISIBLE = /[­​-‍⁠﻿]/g
const HYPHENS = /[‐‑﹣]/g
const FULL_WIDTH = /[！-～]/g
const LIGATURES = /[ﬀ-ﬆ]/g

export function cleanPastedText(text: string): string {
  return text
    .replace(/[\u0085\u2028\u2029]/g, '\n')
    .replace(INVISIBLE, '')
    .replace(SPACES, ' ')
    .replace(HYPHENS, '-')
    .replace(/[⁄∕]/g, '/')
    .replace(/∗/g, '*')
    .replace(FULL_WIDTH, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0))
    .replace(LIGATURES, (c) => c.normalize('NFKC'))
}
