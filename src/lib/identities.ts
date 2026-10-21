import { nativeHandler } from './bridge'
import { isWindowsHost } from './platform'

// `show` is how a line reads, with ^{…} raised and _{…} lowered; `copy` is what a click puts on the clipboard,
// written the way the bar takes it
// `lead` is a row's leading table cells, drawn in fixed columns ahead of `show`: a prefix's name and symbol
export type Identity = { show: string; copy: string; note?: string; lead?: string[] }
export type IdentityGroup = { title: string; items: Identity[] }
export type IdentitySection = { title: string; groups: IdentityGroup[] }
export type IdentitySheet = { id: 'identities' | 'calculus' | 'conversions'; title: string; hint: string; sections: IdentitySection[] }

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

export const CONVERSION_SHEET: IdentitySheet = {
  id: 'conversions',
  title: 'Unit Conversions',
  hint: 'unit conversions',
  sections: [PREFIXES, SIZE, MECHANICS, OTHER],
}

const IDENTITY_WORDS = /^(?:(?:trig(?:onometric|onometry)?|log(?:arithm(?:ic)?)?|exponent|algebra(?:ic)?|math)\s+)?identit(?:y|ies)$/i
const CONVERSION_WORDS =
  /^(?:(?:(?:unit|metric)\s+)?conversions?(?:\s+(?:table|sheet|factors))?|(?:(?:metric|si)\s+)?prefix(?:es)?(?:\s+table)?|metric(?:\s+(?:table|system))?|units?\s+(?:table|sheet))$/i
const CALCULUS_WORDS = /^(?:(?:common\s+)?(?:derivatives?|integrals?)|calculus(?:\s+(?:identities|sheet|rules))?|derivative\s+rules)$/i

/** The sheet a bar line opens on enter, or null. */
// the windows sheet window's page, which is told only the id
export function identitySheetById(id: string | null): IdentitySheet | null {
  return [IDENTITY_SHEET, CALCULUS_SHEET, CONVERSION_SHEET].find((s) => s.id === id) ?? null
}

export function identitySheetFor(text: string): IdentitySheet | null {
  const t = text.trim().replace(/\s+/g, ' ')
  if (IDENTITY_WORDS.test(t)) return IDENTITY_SHEET
  if (CALCULUS_WORDS.test(t)) return CALCULUS_SHEET
  if (CONVERSION_WORDS.test(t)) return CONVERSION_SHEET
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
export function identityLabel(show: string, lead: string[] = []): string {
  const text = identityRuns(show)
    .map((r) => r.text)
    .join('')
  return [...lead, text].join(' ')
}

// the mac app draws the sheet in its own window from this, so the data lives in one place; false means no native window.
// the windows shell opens its own window on sheet.html, which finds the sheet by its id and draws it itself
export function openNativeIdentitySheet(sheet: IdentitySheet): boolean {
  const native = nativeHandler()
  if (!native) return false
  if (isWindowsHost()) {
    native.postMessage({ type: 'identities', id: sheet.id, title: sheet.title })
    return true
  }
  const sections = sheet.sections.map((s) => ({
    title: s.title,
    groups: s.groups.map((g) => ({
      title: g.title,
      items: g.items.map((item) => ({ runs: identityRuns(item.show), copy: item.copy, note: item.note ?? '', lead: item.lead ?? [] })),
    })),
  }))
  native.postMessage({ type: 'identities', id: sheet.id, title: sheet.title, sections })
  return true
}
