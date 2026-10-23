import { nativeHandler } from './bridge'
import { isWindowsHost } from './platform'

// `show` is how a line reads, with ^{…} raised and _{…} lowered; `copy` is what a click puts on the clipboard,
// written the way the bar takes it
// `lead` is a row's leading table cells, drawn in fixed columns ahead of `show`: a prefix's name and symbol
export type Identity = { show: string; copy: string; note?: string; lead?: string[] }
export type IdentityGroup = { title: string; items: Identity[] }
// a `wide` section's groups go one under another at the sheet's full width, for rows too long for a column
export type IdentitySection = { title: string; groups: IdentityGroup[]; wide?: boolean }
export type IdentitySheet = { id: 'identities' | 'calculus' | 'conversions' | 'constants'; title: string; hint: string; sections: IdentitySection[] }

// a run of text, raised or lowered or neither; supsub is a subscript inside a superscript, as in b^{log_{b}x}.
// the page and the mac window both draw these
export type Run = { text: string; shift?: 'sup' | 'sub' | 'supsub' }

/** The widths, in points, of a row's `lead` cells. */
export const LEAD_WIDTHS = [64, 28]

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
        id('∫ cot x dx = ln|sin x| + C', '∫ cot(x) dx = ln(abs(sin(x))) + C'),
        id('∫ sec x dx = ln|sec x + tan x| + C', '∫ sec(x) dx = ln(abs(sec(x) + tan(x))) + C'),
        id('∫ csc x dx = −ln|csc x + cot x| + C', '∫ csc(x) dx = -ln(abs(csc(x) + cot(x))) + C'),
        id('∫ sin^{2}x dx = x/2 − sin 2x/4 + C', '∫ sin(x)^2 dx = x/2 - sin(2x)/4 + C'),
        id('∫ cos^{2}x dx = x/2 + sin 2x/4 + C', '∫ cos(x)^2 dx = x/2 + sin(2x)/4 + C'),
      ],
    },
    {
      title: 'Logs',
      items: [
        id('∫ ln x dx = x ln x − x + C', '∫ ln(x) dx = x ln(x) - x + C'),
        id('∫ log_{b}x dx = (x ln x − x)/ln b + C', '∫ log(x, b) dx = (x ln(x) - x)/ln(b) + C'),
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

// each form with its constants, by cover-up; the constants copy as the bar's own assignments, so a pasted set works
const PARTIAL_FRACTIONS: IdentitySection = {
  title: 'Partial fractions',
  wide: true,
  groups: [
    {
      title: 'Distinct linear factors',
      items: [
        id('(px + q)/((x − a)(x − b)) = A/(x − a) + B/(x − b)', '(p x + q)/((x - a)(x - b)) = A/(x - a) + B/(x - b)'),
        id('A = (pa + q)/(a − b)', 'A = (p a + q)/(a - b)'),
        id('B = (pb + q)/(b − a)', 'B = (p b + q)/(b - a)'),
        id('1/(x^{2} − a^{2}) = (1/(x − a) − 1/(x + a))/2a', '1/(x^2 - a^2) = (1/(x - a) - 1/(x + a))/(2a)'),
      ],
    },
    {
      title: 'Repeated linear factor',
      items: [
        id('(px + q)/(x − a)^{2} = A/(x − a) + B/(x − a)^{2}', '(p x + q)/(x - a)^2 = A/(x - a) + B/(x - a)^2'),
        id('A = p', 'A = p'),
        id('B = pa + q', 'B = p a + q'),
      ],
    },
    {
      title: 'Irreducible quadratic factor',
      items: [
        id('(px + q)/((x − a)(x^{2} + b^{2})) = A/(x − a) + (Bx + C)/(x^{2} + b^{2})', '(p x + q)/((x - a)(x^2 + b^2)) = A/(x - a) + (B x + C)/(x^2 + b^2)'),
        id('A = (pa + q)/(a^{2} + b^{2})', 'A = (p a + q)/(a^2 + b^2)'),
        id('B = −A', 'B = -A'),
        id('C = p − aA', 'C = p - a A'),
      ],
    },
  ],
}

const SUMMATION: IdentitySection = {
  title: 'Summation',
  groups: [
    {
      title: 'Powers',
      items: [
        id('Σ_{k=1}^{n} 1 = n', 'Σ 1, k = 1..n = n'),
        id('Σ_{k=1}^{n} k = n(n + 1)/2', 'Σ k, k = 1..n = n(n + 1)/2'),
        id('Σ_{k=1}^{n} k^{2} = n(n + 1)(2n + 1)/6', 'Σ k^2, k = 1..n = n(n + 1)(2n + 1)/6'),
        id('Σ_{k=1}^{n} k^{3} = (n(n + 1)/2)^{2}', 'Σ k^3, k = 1..n = (n(n + 1)/2)^2'),
        id('Σ_{k=1}^{n} (2k − 1) = n^{2}', 'Σ (2k - 1), k = 1..n = n^2', 'odd numbers'),
      ],
    },
    {
      title: 'Arithmetic and geometric',
      items: [
        id('Σ_{k=1}^{n} (a + (k − 1)d) = n(2a + (n − 1)d)/2', 'Σ (a + (k - 1)d), k = 1..n = n(2a + (n - 1)d)/2'),
        id('Σ_{k=1}^{n} ar^{k−1} = a(1 − r^{n})/(1 − r)', 'Σ a r^(k - 1), k = 1..n = a(1 - r^n)/(1 - r)', 'r ≠ 1'),
        id('Σ_{k=0}^{∞} ar^{k} = a/(1 − r)', 'Σ a r^k, k = 0..∞ = a/(1 - r)', '|r| < 1'),
        id('Σ_{k=1}^{∞} kr^{k} = r/(1 − r)^{2}', 'Σ k r^k, k = 1..∞ = r/(1 - r)^2', '|r| < 1'),
      ],
    },
    {
      title: 'Series',
      items: [
        id('Σ_{k=0}^{∞} x^{k}/k! = e^{x}', 'Σ x^k/k!, k = 0..∞ = e^x'),
        id('Σ_{k=0}^{n} C(n, k) = 2^{n}', 'Σ nCr(n, k), k = 0..n = 2^n'),
      ],
    },
  ],
}

const spaced = (digits: string) => digits.replace(/(\d{3})(?=\d)/g, '$1 ')
// 10^n written out, in groups of three: 1 000 000, 0.000 001
const written = (n: number) => (n < 0 ? `0.${spaced(`${'0'.repeat(-n - 1)}1`)}` : n % 3 ? `1${'0'.repeat(n)}` : `1${' 000'.repeat(n / 3)}`)
// past 10^15 the digits no longer fit a row, so the name stands in
const prefix = (name: string, symbol: string, n: number, word = ''): Identity => ({
  lead: [name, symbol],
  show: `10^{${n < 0 ? '−' : ''}${Math.abs(n)}}`,
  copy: `10^${n}`,
  note: word || written(n),
})
// one unit in another, as the calculator itself converts it; `show` defaults to the copy with its powers raised
const cv = (copy: string, show?: string, note?: string): Identity =>
  id(show ?? copy.replace(/\^(\d+)/g, '^{$1}').replace(/\bpi\b/g, 'π'), copy, note)

const PREFIXES: IdentitySection = {
  title: 'Metric prefixes',
  groups: [
    {
      title: 'Multiples',
      items: [
        prefix('quetta', 'Q', 30, 'nonillion'),
        prefix('ronna', 'R', 27, 'octillion'),
        prefix('yotta', 'Y', 24, 'septillion'),
        prefix('zetta', 'Z', 21, 'sextillion'),
        prefix('exa', 'E', 18, 'quintillion'),
        prefix('peta', 'P', 15),
        prefix('tera', 'T', 12),
        prefix('giga', 'G', 9),
        prefix('mega', 'M', 6),
        prefix('kilo', 'k', 3),
        prefix('hecto', 'h', 2),
        prefix('deka', 'da', 1),
      ],
    },
    {
      title: 'Fractions',
      items: [
        prefix('deci', 'd', -1),
        prefix('centi', 'c', -2),
        prefix('milli', 'm', -3),
        prefix('micro', 'μ', -6),
        prefix('nano', 'n', -9),
        prefix('pico', 'p', -12),
        prefix('femto', 'f', -15),
        prefix('atto', 'a', -18, 'quintillionth'),
        prefix('zepto', 'z', -21, 'sextillionth'),
        prefix('yocto', 'y', -24, 'septillionth'),
        prefix('ronto', 'r', -27, 'octillionth'),
        prefix('quecto', 'q', -30, 'nonillionth'),
      ],
    },
  ],
}

const SIZE: IdentitySection = {
  title: 'Length, area, volume and mass',
  groups: [
    {
      title: 'Length',
      items: [
        cv('1 in = 2.54 cm', undefined, 'exact'),
        cv('1 ft = 0.3048 m', undefined, 'exact'),
        cv('1 yd = 0.9144 m', undefined, 'exact'),
        cv('1 mi = 1.609344 km', undefined, 'exact'),
        cv('1 mi = 5280 ft'),
        cv('1 nmi = 1852 m'),
        cv('1 m = 3.28084 ft'),
        cv('1 km = 0.621371 mi'),
        cv('1 mil = 0.0254 mm'),
        cv('1 angstrom = 1e-10 m', '1 Å = 10^{−10} m'),
        cv('1 au = 1.495979e8 km', '1 au = 1.495979 × 10^{8} km'),
        cv('1 ly = 9.46073e12 km', '1 ly = 9.46073 × 10^{12} km'),
      ],
    },
    {
      title: 'Volume',
      items: [
        cv('1 L = 1000 cm^3'),
        cv('1 m^3 = 1000 L'),
        cv('1 gal = 3.78541 L'),
        cv('1 gal = 231 in^3'),
        cv('1 qt = 0.946353 L'),
        cv('1 cup = 236.588 mL'),
        cv('1 floz = 29.5735 mL', '1 fl oz = 29.5735 mL'),
        cv('1 tbsp = 14.7868 mL'),
        cv('1 tsp = 4.92892 mL'),
        cv('1 ft^3 = 28.3168 L'),
        cv('1 in^3 = 16.3871 cm^3'),
      ],
    },
    {
      title: 'Area',
      items: [
        cv('1 m^2 = 10000 cm^2', '1 m^{2} = 10^{4} cm^{2}'),
        cv('1 ft^2 = 144 in^2'),
        cv('1 ft^2 = 0.092903 m^2'),
        cv('1 acre = 43560 ft^2'),
        cv('1 acre = 4046.86 m^2'),
        cv('1 ha = 10000 m^2', '1 ha = 10^{4} m^{2}'),
        cv('1 mi^2 = 2.58999 km^2'),
      ],
    },
    {
      title: 'Mass',
      items: [
        cv('1 lb = 0.453592 kg'),
        cv('1 kg = 2.20462 lb'),
        cv('1 lb = 16 oz'),
        cv('1 oz = 28.3495 g'),
        cv('1 slug = 14.5939 kg'),
        cv('1 tonne = 1000 kg'),
        cv('1 ton = 2000 lb', undefined, 'US short'),
        cv('1 grain = 64.7989 mg'),
        cv('1 u = 1.66054e-27 kg', '1 u = 1.66054 × 10^{−27} kg'),
      ],
    },
  ],
}

const MECHANICS: IdentitySection = {
  title: 'Force, pressure, energy and motion',
  groups: [
    {
      title: 'Force',
      items: [
        cv('1 lbf = 4.44822 N'),
        cv('1 N = 0.224809 lbf'),
        cv('1 kgf = 9.80665 N', undefined, 'exact'),
        cv('1 kip = 1000 lbf'),
        cv('1 dyn = 1e-5 N', '1 dyn = 10^{−5} N'),
      ],
    },
    {
      title: 'Pressure',
      items: [
        cv('1 atm = 101.325 kPa', undefined, 'exact'),
        cv('1 atm = 760 torr'),
        cv('1 atm = 14.6959 psi'),
        cv('1 bar = 100 kPa'),
        cv('1 psi = 6.89476 kPa'),
        cv('1 ksi = 6.89476 MPa'),
        cv('1 torr = 133.322 Pa'),
        cv('1 inHg = 3.38639 kPa'),
      ],
    },
    {
      title: 'Energy',
      items: [
        cv('1 cal = 4.184 J', undefined, 'exact'),
        cv('1 kcal = 4.184 kJ'),
        cv('1 BTU = 1055.06 J'),
        cv('1 kWh = 3.6 MJ'),
        cv('1 ft lbf = 1.35582 J'),
        cv('1 L atm = 101.325 J'),
        cv('1 eV = 1.60218e-19 J', '1 eV = 1.60218 × 10^{−19} J'),
        cv('1 erg = 1e-7 J', '1 erg = 10^{−7} J'),
      ],
    },
    {
      title: 'Power',
      items: [cv('1 hp = 745.7 W'), cv('1 kW = 1.34102 hp'), cv('1 BTU/hr = 0.293071 W')],
    },
    {
      title: 'Torque',
      items: [cv('1 lbf ft = 1.35582 N m'), cv('1 N m = 0.737562 lbf ft'), cv('1 lbf in = 0.112985 N m')],
    },
    {
      title: 'Speed',
      items: [
        cv('1 mph = 0.44704 m/s', undefined, 'exact'),
        cv('1 mph = 1.609344 km/h'),
        cv('1 m/s = 2.23694 mph'),
        cv('1 m/s = 3.6 km/h'),
        cv('1 ft/s = 0.3048 m/s'),
        cv('1 knot = 0.514444 m/s'),
      ],
    },
    {
      title: 'Density',
      items: [cv('1 g/cm^3 = 1000 kg/m^3'), cv('1 lb/ft^3 = 16.0185 kg/m^3')],
    },
  ],
}

const OTHER: IdentitySection = {
  title: 'Temperature, angle, time and data',
  groups: [
    {
      title: 'Temperature',
      items: [
        id('°F = °C × 9/5 + 32', 'F = C*9/5 + 32'),
        id('°C = (°F − 32) × 5/9', 'C = (F - 32)*5/9'),
        id('K = °C + 273.15', 'K = C + 273.15'),
        id('°R = °F + 459.67', 'R = F + 459.67'),
        id('°R = K × 9/5', 'R = K*9/5'),
        id('Δ1 °C = Δ1 K = Δ1.8 °F', 'dC = dK = 1.8 dF', 'a difference'),
      ],
    },
    {
      title: 'Angle',
      items: [
        cv('180 deg = pi rad', '180° = π rad'),
        cv('1 rad = 57.2958 deg', '1 rad = 57.2958°'),
        cv('1 rev = 2pi rad', '1 rev = 2π rad'),
        cv('1 deg = 60 arcmin', '1° = 60 arcmin'),
        cv('1 arcmin = 60 arcsec'),
        cv('1 rpm = 0.10472 rad/s'),
        cv('1 Hz = 60 rpm'),
      ],
    },
    {
      title: 'Time',
      items: [
        cv('1 hr = 3600 s'),
        cv('1 day = 86400 s'),
        cv('1 week = 7 days'),
        cv('1 year = 365.25 days', undefined, 'Julian'),
        cv('1 year = 3.15576e7 s', '1 year = 3.15576 × 10^{7} s'),
      ],
    },
    {
      title: 'Data',
      items: [cv('1 byte = 8 bits'), cv('1 kB = 1000 bytes'), cv('1 GB = 1000 MB'), cv('1 KiB = 1024 bytes'), cv('1 GiB = 1024 MiB')],
    },
  ],
}

const SUPERSCRIPT = '⁰¹²³⁴⁵⁶⁷⁸⁹'
// digits in threes either side of the point, as codata prints them: 6.674 30(15), 299 792 458
const grouped = (digits: string) => {
  const [whole, frac] = digits.split('.')
  const left = whole!.length > 4 ? whole!.replace(/\B(?=(\d{3})+$)/g, ' ') : whole!
  return frac == null ? left : `${left}.${frac.replace(/(\d{3})(?=\d)/g, '$1 ')}`
}
// a published value, 6.67430(15)e-11, as the sheet shows it and as the bar reads it
const codata = (value: string) => {
  const [, sign, digits, unc, exp] = value.match(/^([−-]?)([\d.]+)(\(\d+\))?(?:e([−-]?\d+))?$/)!
  const power = exp ? ` × 10^{${exp.replace('-', '−')}}` : ''
  return { show: `${sign ? '−' : ''}${grouped(digits!)}${unc ?? ''}${power}`, plain: `${sign ? '-' : ''}${digits}${exp ? `e${exp}` : ''}` }
}
// the units come written with unicode powers, m·s⁻¹; raised runs draw them like the rest of the sheets
const unitShow = (unit: string) =>
  unit.replace(/([⁻]?[⁰¹²³⁴⁵⁶⁷⁸⁹]+)/g, (p) => `^{${p.replace('⁻', '−').replace(/[⁰¹²³⁴⁵⁶⁷⁸⁹]/g, (d) => String(SUPERSCRIPT.indexOf(d)))}}`)
const relShow = (rel: string) => {
  const m = rel.match(/^([\d.]+)e-(\d+)$/)
  return m ? `${m[1]} × 10⁻${m[2]!.replace(/^0/, '').replace(/\d/g, (d) => SUPERSCRIPT[Number(d)]!)}` : rel
}

// one codata 2022 row: its symbol, name, published value, unit, relative uncertainty ('' when exact) and the unit the
// bar reads it in. '' copies the bare number, for a unit the bar lacks (mol, sr, Wb) or misreads (S as seconds, T as
// tonnes). the bar's own constants (speed of light, mₑ) are older codata values, so the digits shown are what's copied
const constant = (symbol: string, name: string, value: string, unit: string, rel: string, bar: string): Identity => {
  const v = codata(value)
  return {
    lead: [symbol],
    show: unit ? `${v.show} ${unitShow(unit)}` : v.show,
    copy: bar ? `${v.plain} ${bar}` : v.plain,
    note: `${name} · ${rel ? relShow(rel) : 'exact'}`,
  }
}

const CONSTANTS: IdentitySection = {
  title: 'CODATA 2022',
  wide: true,
  groups: [
    {
      title: 'Universal',
      items: [
        constant('c', 'speed of light in vacuum', '299792458', 'm·s⁻¹', '', 'm/s'),
        constant('h', 'Planck constant', '6.62607015e-34', 'J·Hz⁻¹', '', 'J s'),
        constant('ħ', 'reduced Planck constant, h/2π', '1.054571817e-34', 'J·s', '', 'J s'),
        constant('G', 'Newtonian constant of gravitation', '6.67430(15)e-11', 'm³·kg⁻¹·s⁻²', '2.2e-5', 'm^3/(kg s^2)'),
        constant('Λ', 'cosmological constant', '1.089(29)e-52', 'm⁻²', '0.027', 'm^-2'),
      ],
    },
    {
      title: 'Electromagnetic',
      items: [
        constant('e', 'elementary charge', '1.602176634e-19', 'C', '', 'C'),
        constant('μ_{0}', 'vacuum magnetic permeability', '1.25663706127(20)e-6', 'N·A⁻²', '1.6e-10', 'N/A^2'),
        constant('ε_{0}', 'vacuum electric permittivity', '8.8541878188(14)e-12', 'F·m⁻¹', '1.6e-10', 'F/m'),
        constant('Z_{0}', 'characteristic impedance of vacuum', '376.730313412(59)', 'Ω', '1.6e-10', 'ohm'),
        constant('Φ_{0}', 'magnetic flux quantum, h/2e', '2.067833848e-15', 'Wb', '', ''),
        constant('G_{0}', 'conductance quantum, 2e²/h', '7.748091729e-5', 'S', '', ''),
        constant('G_{0}^{−1}', 'inverse conductance quantum', '12906.40372', 'Ω', '', 'ohm'),
        constant('K_{J}', 'Josephson constant, 2e/h', '483597.8484e9', 'Hz·V⁻¹', '', 'Hz/V'),
        constant('R_{K}', 'von Klitzing constant, h/e²', '25812.80745', 'Ω', '', 'ohm'),
        constant('μ_{B}', 'Bohr magneton', '9.2740100657(29)e-24', 'J·T⁻¹', '3.1e-10', ''),
        constant('μ_{N}', 'nuclear magneton', '5.0507837393(16)e-27', 'J·T⁻¹', '3.1e-10', ''),
      ],
    },
    {
      title: 'Atomic and nuclear',
      items: [
        constant('α', 'fine-structure constant', '0.0072973525643(11)', '', '1.6e-10', ''),
        constant('α^{−1}', 'inverse fine-structure constant', '137.035999177(21)', '', '1.6e-10', ''),
        constant('a_{0}', 'Bohr radius', '5.29177210544(82)e-11', 'm', '1.6e-10', 'm'),
        constant('r_{e}', 'classical electron radius', '2.8179403205(13)e-15', 'm', '4.7e-10', 'm'),
        constant('σ_{e}', 'Thomson cross section', '6.6524587051(62)e-29', 'm²', '9.3e-10', 'm^2'),
        constant('h/2m_{e}', 'quantum of circulation', '3.6369475467(11)e-4', 'm²·s⁻¹', '3.1e-10', 'm^2/s'),
        constant('R_{∞}', 'Rydberg constant', '10973731.568157(12)', 'm⁻¹', '1.1e-12', 'm^-1'),
        constant('Ry', 'Rydberg unit of energy, R∞hc', '2.1798723611030(24)e-18', 'J', '1.1e-12', 'J'),
        constant('E_{h}', 'Hartree energy', '4.3597447222060(48)e-18', 'J', '1.1e-12', 'J'),
        constant('G_{F}/(ħc)^{3}', 'Fermi coupling constant', '1.1663787(6)e-5', 'GeV⁻²', '5.1e-7', 'GeV^-2'),
        constant('sin^{2}θ_{W}', 'weak mixing angle, 1 − (m_W/m_Z)²', '0.22305(23)', '', '1.0e-3', ''),
        constant('g_{e}', 'electron g-factor', '−2.00231930436092(36)', '', '1.8e-13', ''),
        constant('g_{μ}', 'muon g-factor', '−2.00233184123(82)', '', '4.1e-10', ''),
        constant('g_{p}', 'proton g-factor', '5.5856946893(16)', '', '2.9e-10', ''),
        constant('Δν_{Cs}', 'caesium-133 hyperfine frequency', '9192631770', 'Hz', '', 'Hz'),
      ],
    },
    {
      title: 'Particle masses',
      items: [
        constant('m_{e}', 'electron mass', '9.1093837139(28)e-31', 'kg', '3.1e-10', 'kg'),
        constant('m_{p}', 'proton mass', '1.67262192595(52)e-27', 'kg', '3.1e-10', 'kg'),
        constant('m_{n}', 'neutron mass', '1.67492750056(85)e-27', 'kg', '5.1e-10', 'kg'),
        constant('m_{μ}', 'muon mass', '1.883531627(42)e-28', 'kg', '2.2e-8', 'kg'),
        constant('m_{τ}', 'tau mass', '3.16754(21)e-27', 'kg', '6.8e-5', 'kg'),
        constant('m_{u}', 'atomic mass constant, m(¹²C)/12', '1.66053906892(52)e-27', 'kg', '3.1e-10', 'kg'),
        constant('m_{p}/m_{e}', 'proton-to-electron mass ratio', '1836.152673426(32)', '', '1.7e-11', ''),
        constant('m_{W}/m_{Z}', 'W-to-Z mass ratio', '0.88145(13)', '', '1.5e-4', ''),
      ],
    },
    {
      title: 'Physico-chemical',
      items: [
        constant('k_{B}', 'Boltzmann constant', '1.380649e-23', 'J·K⁻¹', '', 'J/K'),
        constant('N_{A}', 'Avogadro constant', '6.02214076e23', 'mol⁻¹', '', ''),
        constant('R', 'molar gas constant, N_A k_B', '8.31446261815324', 'J·mol⁻¹·K⁻¹', '', ''),
        constant('F', 'Faraday constant, N_A e', '96485.3321233100184', 'C·mol⁻¹', '', ''),
        constant('N_{A}h', 'molar Planck constant', '3.9903127128934314e-10', 'J·s·mol⁻¹', '', ''),
        constant('M_{u}', 'molar mass constant', '1.00000000105(31)e-3', 'kg·mol⁻¹', '3.1e-10', ''),
        constant('M(^{12}C)', 'molar mass of carbon-12', '12.0000000126(37)e-3', 'kg·mol⁻¹', '3.1e-10', ''),
        constant('V_{m}(Si)', 'molar volume of silicon', '1.205883199(60)e-5', 'm³·mol⁻¹', '4.9e-8', ''),
      ],
    },
    {
      title: 'Radiation',
      items: [
        constant('σ', 'Stefan–Boltzmann constant', '5.670374419e-8', 'W·m⁻²·K⁻⁴', '', 'W/(m^2 K^4)'),
        constant('c_{1}', 'first radiation constant, 2πhc²', '3.741771852e-16', 'W·m²', '', 'W m^2'),
        constant('c_{1L}', 'first radiation constant for spectral radiance', '1.191042972e-16', 'W·m²·sr⁻¹', '', ''),
        constant('c_{2}', 'second radiation constant, hc/k_B', '1.438776877e-2', 'm·K', '', 'm K'),
        constant('b', 'Wien wavelength displacement constant', '2.897771955e-3', 'm·K', '', 'm K'),
        constant('b′', 'Wien frequency displacement constant', '5.878925757e10', 'Hz·K⁻¹', '', 'Hz/K'),
        constant('b_{entropy}', 'Wien entropy displacement constant', '3.002916077e-3', 'm·K', '', 'm K'),
      ],
    },
  ],
}

export const IDENTITY_SHEET: IdentitySheet = {
  id: 'identities',
  title: 'Identities',
  hint: 'identity sheet',
  sections: [TRIG, LOGS, ALGEBRA, PARTIAL_FRACTIONS, SUMMATION],
}

export const CALCULUS_SHEET: IdentitySheet = {
  id: 'calculus',
  title: 'Derivatives and Integrals',
  hint: 'derivatives and integrals',
  sections: [DERIVATIVES, INTEGRALS],
}

export const CONVERSION_SHEET: IdentitySheet = {
  id: 'conversions',
  title: 'Unit Conversions',
  hint: 'unit conversions',
  sections: [PREFIXES, SIZE, MECHANICS, OTHER],
}

export const CONSTANTS_SHEET: IdentitySheet = {
  id: 'constants',
  title: 'Physical Constants',
  hint: 'physical constants',
  sections: [CONSTANTS],
}

const SHEETS = [IDENTITY_SHEET, CALCULUS_SHEET, CONVERSION_SHEET, CONSTANTS_SHEET]

const IDENTITY_WORDS = /^(?:(?:trig(?:onometric|onometry)?|log(?:arithm(?:ic)?)?|exponent|algebra(?:ic)?|math)\s+)?identit(?:y|ies)$/i
const CONVERSION_WORDS =
  /^(?:(?:(?:unit|metric)\s+)?conversions?(?:\s+(?:table|sheet|factors))?|(?:(?:metric|si)\s+)?prefix(?:es)?(?:\s+table)?|metric(?:\s+(?:table|system))?|units?\s+(?:table|sheet))$/i
const CALCULUS_WORDS = /^(?:(?:common\s+)?(?:derivatives?|integrals?)|calculus(?:\s+(?:identities|sheet|rules))?|derivative\s+rules)$/i
const CONSTANTS_WORDS = /^(?:(?:physical|physics|fundamental)\s+)?constants(?:\s+(?:table|sheet))?$|^codata$/i

// `identities <part>` (or `identity`, `idt`) opens the sheet holding that part, at it; the first name is the one
// completion offers
const PARTS: { names: RegExp; first: string; sheet: IdentitySheet; section: IdentitySection }[] = [
  { names: /^trig(?:onometry|onometric)?$/, first: 'trig', sheet: IDENTITY_SHEET, section: TRIG },
  { names: /^(?:logs?|logarithms?|exponents?)$/, first: 'logs', sheet: IDENTITY_SHEET, section: LOGS },
  { names: /^algebra$/, first: 'algebra', sheet: IDENTITY_SHEET, section: ALGEBRA },
  { names: /^(?:integration|integrals?|antiderivatives?)$/, first: 'integration', sheet: CALCULUS_SHEET, section: INTEGRALS },
  { names: /^(?:partial(?: fractions?)?|pf)$/, first: 'partial fractions', sheet: IDENTITY_SHEET, section: PARTIAL_FRACTIONS },
  { names: /^(?:summations?|sums?|series|sigma)$/, first: 'summation', sheet: IDENTITY_SHEET, section: SUMMATION },
  { names: /^(?:derivatives?|differentiation)$/, first: 'derivatives', sheet: CALCULUS_SHEET, section: DERIVATIVES },
  { names: /^(?:(?:physical )?constants?|codata)$/, first: 'constants', sheet: CONSTANTS_SHEET, section: CONSTANTS },
  { names: /^(?:conversions?|units?|prefixes)$/, first: 'conversions', sheet: CONVERSION_SHEET, section: PREFIXES },
]
const PART_COMMAND = /^(?:identities|identity|idt)\s+(.+)$/i

/** The subfields `identities` takes, in completion's order. */
export const IDENTITY_PARTS = PARTS.map((p) => p.first)

// the windows sheet window's page, which is told only the id
export function identitySheetById(id: string | null): IdentitySheet | null {
  return SHEETS.find((s) => s.id === id) ?? null
}

/** Where a bar line opens on enter: a sheet, the index of the section to show first, and what the bar says. */
export type IdentityTarget = { sheet: IdentitySheet; section: number; hint: string }

const at = (sheet: IdentitySheet, section = 0): IdentityTarget => ({
  sheet,
  section,
  hint: section ? `${sheet.hint}: ${sheet.sections[section]!.title.toLowerCase()}` : sheet.hint,
})

export function identityTargetFor(text: string): IdentityTarget | null {
  const t = text.trim().replace(/\s+/g, ' ')
  const part = PART_COMMAND.exec(t)?.[1]?.toLowerCase()
  const named = part ? PARTS.find((p) => p.names.test(part)) : /^trig$/i.test(t) ? PARTS[0] : undefined
  if (named) return at(named.sheet, named.sheet.sections.indexOf(named.section))
  if (/^partial fractions$/i.test(t)) return at(IDENTITY_SHEET, IDENTITY_SHEET.sections.indexOf(PARTIAL_FRACTIONS))
  if (IDENTITY_WORDS.test(t) || /^idt$/i.test(t)) return at(IDENTITY_SHEET)
  if (CALCULUS_WORDS.test(t)) return at(CALCULUS_SHEET)
  if (CONVERSION_WORDS.test(t)) return at(CONVERSION_SHEET)
  if (CONSTANTS_WORDS.test(t)) return at(CONSTANTS_SHEET)
  return null
}

/** The sheet a bar line opens on enter, or null. */
export function identitySheetFor(text: string): IdentitySheet | null {
  return identityTargetFor(text)?.sheet ?? null
}

export function isIdentityCommand(text: string): boolean {
  return identityTargetFor(text) != null
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
export function identityLabel(show: string, lead: string[] = []): string {
  const plain = (text: string) =>
    identityRuns(text)
      .map((r) => r.text)
      .join('')
  return [...lead, show].map(plain).join(' ')
}

// the mac app draws the sheet in its own window from this, so the data lives in one place; false means no native window.
// the windows shell opens its own window on sheet.html, which finds the sheet by its id and draws it itself.
// `section` is the index of the one to scroll to
export function openNativeIdentitySheet({ sheet, section }: IdentityTarget): boolean {
  const native = nativeHandler()
  if (!native) return false
  if (isWindowsHost()) {
    native.postMessage({ type: 'identities', id: sheet.id, title: sheet.title, section })
    return true
  }
  const sections = sheet.sections.map((s) => ({
    title: s.title,
    wide: Boolean(s.wide),
    groups: s.groups.map((g) => ({
      title: g.title,
      items: g.items.map((item) => ({
        runs: identityRuns(item.show),
        copy: item.copy,
        note: item.note ?? '',
        lead: (item.lead ?? []).map(identityRuns),
      })),
    })),
  }))
  native.postMessage({ type: 'identities', id: sheet.id, title: sheet.title, sections, section })
  return true
}
