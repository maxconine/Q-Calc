import { nativeHandler } from './bridge'
import { isWindowsHost } from './platform'

// `show` is how a line reads, with ^{…} raised and _{…} lowered; `copy` is what a click puts on the clipboard,
// written the way the bar takes it
export type Identity = { show: string; copy: string; note?: string }
export type IdentityGroup = { title: string; items: Identity[] }
export type IdentitySection = { title: string; groups: IdentityGroup[] }
export type IdentitySheet = { id: 'identities' | 'calculus'; title: string; hint: string; sections: IdentitySection[] }

// a run of text, raised or lowered or neither; supsub is a subscript inside a superscript, as in b^{log_{b}x}.
// the page and the mac window both draw these
export type Run = { text: string; shift?: 'sup' | 'sub' | 'supsub' }

const id = (show: string, copy: string, note?: string): Identity => (note ? { show, copy, note } : { show, copy })

const TRIG: IdentitySection = {
  title: 'Trig',
  groups: [
    {
      title: 'Pythagorean',
      items: [
        id('sin^{2}x + cos^{2}x = 1', 'sin(x)^2 + cos(x)^2 = 1'),
        id('1 + tan^{2}x = sec^{2}x', '1 + tan(x)^2 = sec(x)^2'),
        id('1 + cot^{2}x = csc^{2}x', '1 + cot(x)^2 = csc(x)^2'),
      ],
    },
    {
      title: 'Reciprocal',
      items: [
        id('csc x = 1/sin x', 'csc(x) = 1/sin(x)'),
        id('sec x = 1/cos x', 'sec(x) = 1/cos(x)'),
        id('cot x = 1/tan x', 'cot(x) = 1/tan(x)'),
      ],
    },
    {
      title: 'Quotient',
      items: [id('tan x = sin x / cos x', 'tan(x) = sin(x)/cos(x)'), id('cot x = cos x / sin x', 'cot(x) = cos(x)/sin(x)')],
    },
    {
      title: 'Cofunction',
      items: [
        id('sin(π/2 − x) = cos x', 'sin(pi/2 - x) = cos(x)'),
        id('cos(π/2 − x) = sin x', 'cos(pi/2 - x) = sin(x)'),
        id('tan(π/2 − x) = cot x', 'tan(pi/2 - x) = cot(x)'),
        id('cot(π/2 − x) = tan x', 'cot(pi/2 - x) = tan(x)'),
        id('sec(π/2 − x) = csc x', 'sec(pi/2 - x) = csc(x)'),
        id('csc(π/2 − x) = sec x', 'csc(pi/2 - x) = sec(x)'),
      ],
    },
    {
      title: 'Even and odd',
      items: [
        id('sin(−x) = −sin x', 'sin(-x) = -sin(x)'),
        id('cos(−x) = cos x', 'cos(-x) = cos(x)'),
        id('tan(−x) = −tan x', 'tan(-x) = -tan(x)'),
      ],
    },
    {
      title: 'Sum and difference',
      items: [
        id('sin(x + y) = sin x cos y + cos x sin y', 'sin(x + y) = sin(x)cos(y) + cos(x)sin(y)'),
        id('sin(x − y) = sin x cos y − cos x sin y', 'sin(x - y) = sin(x)cos(y) - cos(x)sin(y)'),
        id('cos(x + y) = cos x cos y − sin x sin y', 'cos(x + y) = cos(x)cos(y) - sin(x)sin(y)'),
        id('cos(x − y) = cos x cos y + sin x sin y', 'cos(x - y) = cos(x)cos(y) + sin(x)sin(y)'),
        id('tan(x + y) = (tan x + tan y)/(1 − tan x tan y)', 'tan(x + y) = (tan(x) + tan(y))/(1 - tan(x)tan(y))'),
        id('tan(x − y) = (tan x − tan y)/(1 + tan x tan y)', 'tan(x - y) = (tan(x) - tan(y))/(1 + tan(x)tan(y))'),
      ],
    },
    {
      title: 'Double angle',
      items: [
        id('sin 2x = 2 sin x cos x', 'sin(2x) = 2sin(x)cos(x)'),
        id('cos 2x = cos^{2}x − sin^{2}x', 'cos(2x) = cos(x)^2 - sin(x)^2'),
        id('cos 2x = 2cos^{2}x − 1', 'cos(2x) = 2cos(x)^2 - 1'),
        id('cos 2x = 1 − 2sin^{2}x', 'cos(2x) = 1 - 2sin(x)^2'),
        id('tan 2x = 2 tan x/(1 − tan^{2}x)', 'tan(2x) = 2tan(x)/(1 - tan(x)^2)'),
      ],
    },
    {
      title: 'Half angle',
      items: [
        id('sin(x/2) = ±√((1 − cos x)/2)', 'sin(x/2) = ±sqrt((1 - cos(x))/2)'),
        id('cos(x/2) = ±√((1 + cos x)/2)', 'cos(x/2) = ±sqrt((1 + cos(x))/2)'),
        id('tan(x/2) = (1 − cos x)/sin x', 'tan(x/2) = (1 - cos(x))/sin(x)'),
        id('tan(x/2) = sin x/(1 + cos x)', 'tan(x/2) = sin(x)/(1 + cos(x))'),
      ],
    },
    {
      title: 'Power reduction',
      items: [
        id('sin^{2}x = (1 − cos 2x)/2', 'sin(x)^2 = (1 - cos(2x))/2'),
        id('cos^{2}x = (1 + cos 2x)/2', 'cos(x)^2 = (1 + cos(2x))/2'),
        id('tan^{2}x = (1 − cos 2x)/(1 + cos 2x)', 'tan(x)^2 = (1 - cos(2x))/(1 + cos(2x))'),
      ],
    },
    {
      title: 'Product to sum',
      items: [
        id('sin x sin y = ½[cos(x − y) − cos(x + y)]', 'sin(x)sin(y) = (cos(x - y) - cos(x + y))/2'),
        id('cos x cos y = ½[cos(x − y) + cos(x + y)]', 'cos(x)cos(y) = (cos(x - y) + cos(x + y))/2'),
        id('sin x cos y = ½[sin(x + y) + sin(x − y)]', 'sin(x)cos(y) = (sin(x + y) + sin(x - y))/2'),
        id('cos x sin y = ½[sin(x + y) − sin(x − y)]', 'cos(x)sin(y) = (sin(x + y) - sin(x - y))/2'),
      ],
    },
    {
      title: 'Sum to product',
      items: [
        id('sin x + sin y = 2 sin((x + y)/2) cos((x − y)/2)', 'sin(x) + sin(y) = 2sin((x + y)/2)cos((x - y)/2)'),
        id('sin x − sin y = 2 cos((x + y)/2) sin((x − y)/2)', 'sin(x) - sin(y) = 2cos((x + y)/2)sin((x - y)/2)'),
        id('cos x + cos y = 2 cos((x + y)/2) cos((x − y)/2)', 'cos(x) + cos(y) = 2cos((x + y)/2)cos((x - y)/2)'),
        id('cos x − cos y = −2 sin((x + y)/2) sin((x − y)/2)', 'cos(x) - cos(y) = -2sin((x + y)/2)sin((x - y)/2)'),
      ],
    },
  ],
}

const LOGS: IdentitySection = {
  title: 'Logs and exponents',
  groups: [
    {
      title: 'Log rules',
      items: [
        id('log_{b}(xy) = log_{b}x + log_{b}y', 'log(x y, b) = log(x, b) + log(y, b)'),
        id('log_{b}(x/y) = log_{b}x − log_{b}y', 'log(x/y, b) = log(x, b) - log(y, b)'),
        id('log_{b}(x^{r}) = r log_{b}x', 'log(x^r, b) = r log(x, b)'),
        id('log_{b}x = ln x / ln b', 'log(x, b) = ln(x)/ln(b)', 'change of base'),
        id('log_{b}1 = 0', 'log(1, b) = 0'),
        id('log_{b}b = 1', 'log(b, b) = 1'),
      ],
    },
    {
      title: 'Inverses',
      items: [
        id('log_{b}(b^{x}) = x', 'log(b^x, b) = x'),
        id('b^{log_{b}x} = x', 'b^log(x, b) = x'),
        id('ln(e^{x}) = x', 'ln(e^x) = x'),
        id('e^{ln x} = x', 'e^ln(x) = x'),
      ],
    },
    {
      title: 'Exponent rules',
      items: [
        id('a^{m}a^{n} = a^{m+n}', 'a^m a^n = a^(m + n)'),
        id('a^{m}/a^{n} = a^{m−n}', 'a^m/a^n = a^(m - n)'),
        id('(a^{m})^{n} = a^{mn}', '(a^m)^n = a^(m n)'),
        id('(ab)^{n} = a^{n}b^{n}', '(a b)^n = a^n b^n'),
        id('(a/b)^{n} = a^{n}/b^{n}', '(a/b)^n = a^n/b^n'),
        id('a^{0} = 1', 'a^0 = 1'),
        id('a^{−n} = 1/a^{n}', 'a^(-n) = 1/a^n'),
        id('a^{1/n} = ^{n}√a', 'a^(1/n) = nthroot(a, n)'),
      ],
    },
  ],
}

const ALGEBRA: IdentitySection = {
  title: 'Algebra',
  groups: [
    {
      title: 'Squares',
      items: [
        id('(a + b)^{2} = a^{2} + 2ab + b^{2}', '(a + b)^2 = a^2 + 2a b + b^2'),
        id('(a − b)^{2} = a^{2} − 2ab + b^{2}', '(a - b)^2 = a^2 - 2a b + b^2'),
        id('a^{2} − b^{2} = (a + b)(a − b)', 'a^2 - b^2 = (a + b)(a - b)'),
      ],
    },
    {
      title: 'Cubes',
      items: [
        id('a^{3} + b^{3} = (a + b)(a^{2} − ab + b^{2})', 'a^3 + b^3 = (a + b)(a^2 - a b + b^2)'),
        id('a^{3} − b^{3} = (a − b)(a^{2} + ab + b^{2})', 'a^3 - b^3 = (a - b)(a^2 + a b + b^2)'),
        id('(a + b)^{3} = a^{3} + 3a^{2}b + 3ab^{2} + b^{3}', '(a + b)^3 = a^3 + 3a^2 b + 3a b^2 + b^3'),
        id('(a − b)^{3} = a^{3} − 3a^{2}b + 3ab^{2} − b^{3}', '(a - b)^3 = a^3 - 3a^2 b + 3a b^2 - b^3'),
      ],
    },
    {
      title: 'Binomial theorem',
      items: [
        id('(a + b)^{n} = Σ_{k=0}^{n} C(n, k) a^{n−k}b^{k}', '(a + b)^n = Σ nCr(n, k) a^(n - k) b^k, k = 0..n'),
      ],
    },
  ],
}

const DERIVATIVES: IdentitySection = {
  title: 'Derivatives',
  groups: [
    {
      title: 'Powers, exponentials and logs',
      items: [
        id('d/dx x^{n} = nx^{n−1}', 'd/dx x^n = n x^(n - 1)'),
        id('d/dx e^{x} = e^{x}', 'd/dx e^x = e^x'),
        id('d/dx b^{x} = b^{x} ln b', 'd/dx b^x = b^x ln(b)'),
        id('d/dx ln x = 1/x', 'd/dx ln(x) = 1/x'),
        id('d/dx log_{b}x = 1/(x ln b)', 'd/dx log(x, b) = 1/(x ln(b))'),
      ],
    },
    {
      title: 'Trig',
      items: [
        id('d/dx sin x = cos x', 'd/dx sin(x) = cos(x)'),
        id('d/dx cos x = −sin x', 'd/dx cos(x) = -sin(x)'),
        id('d/dx tan x = sec^{2}x', 'd/dx tan(x) = sec(x)^2'),
        id('d/dx cot x = −csc^{2}x', 'd/dx cot(x) = -csc(x)^2'),
        id('d/dx sec x = sec x tan x', 'd/dx sec(x) = sec(x)tan(x)'),
        id('d/dx csc x = −csc x cot x', 'd/dx csc(x) = -csc(x)cot(x)'),
      ],
    },
    {
      title: 'Inverse trig',
      items: [
        id('d/dx arcsin x = 1/√(1 − x^{2})', 'd/dx arcsin(x) = 1/sqrt(1 - x^2)'),
        id('d/dx arccos x = −1/√(1 − x^{2})', 'd/dx arccos(x) = -1/sqrt(1 - x^2)'),
        id('d/dx arctan x = 1/(1 + x^{2})', 'd/dx arctan(x) = 1/(1 + x^2)'),
      ],
    },
    {
      title: 'Rules',
      items: [
        id('(fg)′ = f′g + fg′', "d/dx f(x)g(x) = f'(x)g(x) + f(x)g'(x)", 'product'),
        id('(f/g)′ = (f′g − fg′)/g^{2}', "d/dx f(x)/g(x) = (f'(x)g(x) - f(x)g'(x))/g(x)^2", 'quotient'),
        id('f(g(x))′ = f′(g(x)) g′(x)', "d/dx f(g(x)) = f'(g(x))g'(x)", 'chain'),
      ],
    },
  ],
}

const INTEGRALS: IdentitySection = {
  title: 'Integrals',
  groups: [
    {
      title: 'Powers, exponentials and logs',
      items: [
        id('∫ x^{n} dx = x^{n+1}/(n + 1) + C', '∫ x^n dx = x^(n + 1)/(n + 1) + C', 'n ≠ −1'),
        id('∫ 1/x dx = ln|x| + C', '∫ 1/x dx = ln(abs(x)) + C'),
        id('∫ e^{x} dx = e^{x} + C', '∫ e^x dx = e^x + C'),
        id('∫ b^{x} dx = b^{x}/ln b + C', '∫ b^x dx = b^x/ln(b) + C'),
      ],
    },
    {
      title: 'Trig',
      items: [
        id('∫ sin x dx = −cos x + C', '∫ sin(x) dx = -cos(x) + C'),
        id('∫ cos x dx = sin x + C', '∫ cos(x) dx = sin(x) + C'),
        id('∫ sec^{2}x dx = tan x + C', '∫ sec(x)^2 dx = tan(x) + C'),
        id('∫ csc^{2}x dx = −cot x + C', '∫ csc(x)^2 dx = -cot(x) + C'),
        id('∫ sec x tan x dx = sec x + C', '∫ sec(x)tan(x) dx = sec(x) + C'),
        id('∫ csc x cot x dx = −csc x + C', '∫ csc(x)cot(x) dx = -csc(x) + C'),
        id('∫ tan x dx = ln|sec x| + C', '∫ tan(x) dx = ln(abs(sec(x))) + C'),
      ],
    },
    {
      title: 'Inverse trig forms',
      items: [
        id('∫ 1/√(1 − x^{2}) dx = arcsin x + C', '∫ 1/sqrt(1 - x^2) dx = arcsin(x) + C'),
        id('∫ 1/(1 + x^{2}) dx = arctan x + C', '∫ 1/(1 + x^2) dx = arctan(x) + C'),
        id('∫ 1/√(a^{2} − x^{2}) dx = arcsin(x/a) + C', '∫ 1/sqrt(a^2 - x^2) dx = arcsin(x/a) + C'),
        id('∫ 1/(a^{2} + x^{2}) dx = (1/a) arctan(x/a) + C', '∫ 1/(a^2 + x^2) dx = arctan(x/a)/a + C'),
      ],
    },
    {
      title: 'By parts',
      items: [id('∫ u dv = uv − ∫ v du', '∫ u dv = u v - ∫ v du')],
    },
  ],
}

export const IDENTITY_SHEET: IdentitySheet = {
  id: 'identities',
  title: 'Identities',
  hint: 'identity sheet',
  sections: [TRIG, LOGS, ALGEBRA],
}

export const CALCULUS_SHEET: IdentitySheet = {
  id: 'calculus',
  title: 'Derivatives and Integrals',
  hint: 'derivatives and integrals',
  sections: [DERIVATIVES, INTEGRALS],
}

const IDENTITY_WORDS = /^(?:(?:trig(?:onometric|onometry)?|log(?:arithm(?:ic)?)?|exponent|algebra(?:ic)?|math)\s+)?identit(?:y|ies)$/i
const CALCULUS_WORDS = /^(?:(?:common\s+)?(?:derivatives?|integrals?)|calculus(?:\s+(?:identities|sheet|rules))?|derivative\s+rules)$/i

/** The sheet a bar line opens on enter, or null. */
export function identitySheetFor(text: string): IdentitySheet | null {
  const t = text.trim().replace(/\s+/g, ' ')
  if (IDENTITY_WORDS.test(t)) return IDENTITY_SHEET
  if (CALCULUS_WORDS.test(t)) return CALCULUS_SHEET
  return null
}

export function isIdentityCommand(text: string): boolean {
  return identitySheetFor(text) != null
}

/** Splits a `show` line into plain, raised and lowered runs; braces nest one level, for b^{log_{b}x}. */
export function identityRuns(show: string): Run[] {
  const runs: Run[] = []
  const push = (text: string, shift?: Run['shift']) => {
    if (!text) return
    const last = runs[runs.length - 1]
    if (last && last.shift === shift) last.text += text
    else runs.push(shift ? { text, shift } : { text })
  }
  let i = 0
  const walk = (shift: Run['shift'], close: boolean) => {
    while (i < show.length) {
      const c = show[i]!
      if (close && c === '}') {
        i++
        return
      }
      if ((c === '^' || c === '_') && show[i + 1] === '{') {
        i += 2
        const inner: Run['shift'] = c === '^' ? 'sup' : 'sub'
        // deeper than one level keeps the outer shift: it would be too small to read
        walk(!shift ? inner : shift === 'sup' && inner === 'sub' ? 'supsub' : shift, true)
        continue
      }
      push(c, shift)
      i++
    }
  }
  walk(undefined, false)
  return runs
}

/** The plain text a screen reader gets for a line. */
export function identityLabel(show: string): string {
  return identityRuns(show)
    .map((r) => r.text)
    .join('')
}

// the mac app draws the sheet in its own window from this, so the data lives in one place; false means no native window.
// the windows shell has none, so it gets the page's own sheet
export function openNativeIdentitySheet(sheet: IdentitySheet): boolean {
  const native = nativeHandler()
  if (!native || isWindowsHost()) return false
  const sections = sheet.sections.map((s) => ({
    title: s.title,
    groups: s.groups.map((g) => ({
      title: g.title,
      items: g.items.map((item) => ({ runs: identityRuns(item.show), copy: item.copy, note: item.note ?? '' })),
    })),
  }))
  native.postMessage({ type: 'identities', id: sheet.id, title: sheet.title, sections })
  return true
}
