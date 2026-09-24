import { num, textVal } from './format'
import { literalMeas, typedDigits } from './measure'
import type { Meas, Value } from './types'

export const IMPROPER_UNIT_CONVERSION = 'improper unit conversion'

export function isImproperUnitConversion(text: string): boolean {
  return text.trim().toLowerCase() === IMPROPER_UNIT_CONVERSION
}

function unitError(): Value {
  return textVal(IMPROPER_UNIT_CONVERSION)
}

export type Dim =
  | 'length'
  | 'mass'
  | 'temperature'
  | 'volume'
  | 'area'
  | 'speed'
  | 'time'
  | 'digital'
  | 'datarate'
  | 'energy'
  | 'power'
  | 'pressure'
  | 'force'
  | 'angle'
  | 'frequency'
  | 'acceleration'
  | 'charge'
  | 'current'
  | 'voltage'
  | 'resistance'
  | 'capacitance'
  | 'inductance'
  | 'dimensionless'

/** Preferred output unit per dimension. A missing key keeps the built-in SI ↔ US counterpart. */
export type DefaultUnits = Partial<Record<Dim, string>>

type Unit = {
  id: string
  dim: Dim
  symbol: string
  toBase: number
  names: string[]
  /** The counterpart shown when there is no "to ..." target. */
  defaultTo?: string
  /** Accepts SI prefixes (kilometer, ms, mega joules). */
  prefixable?: boolean
  /** Symbols whose b or B must match as typed: `Mb` is a megabit, `MB` a megabyte. */
  cased?: string[]
  /** Names that only match exactly as written: `Cal` is the food calorie, `cal` the small one. */
  exactNames?: string[]
}

const LB = 0.45359237
const FT = 0.3048
const MI = 1609.344
const US_GAL = 3.785411784
const US_FLOZ = 0.0295735295625
const ACRE = 4046.8564224
const IN3 = 0.016387064
const PSI = 6894.757293168361
const LBF = 4.4482216152605
const G0 = 9.80665
const C = 299792458
const E_CHARGE = 1.602176634e-19
const AMU = 1.6605390666e-27
const AU = 149597870700
const YEAR = 365.25 * 86400
const SLUG = (G0 * LB) / FT
const SLINCH = SLUG * 12
const LIGHT_YEAR = C * YEAR
const PARSEC = (AU * 648000) / Math.PI
const THERM = 1e5 * 1055.05585262
const US_BUSHEL = 35.23907016688
const OIL_BBL = 42 * US_GAL
const CORD = 128 * 28.316846592
const DARCY = 9.869232667160128e-13

type Prefix = { name: string; names: string[]; symbol: string; factor: number }

const PREFIXES: Prefix[] = [
  { name: 'yocto', names: ['yocto'], symbol: 'y', factor: 1e-24 },
  { name: 'zepto', names: ['zepto'], symbol: 'z', factor: 1e-21 },
  { name: 'atto', names: ['atto'], symbol: 'a', factor: 1e-18 },
  { name: 'femto', names: ['femto'], symbol: 'f', factor: 1e-15 },
  { name: 'pico', names: ['pico'], symbol: 'p', factor: 1e-12 },
  { name: 'nano', names: ['nano'], symbol: 'n', factor: 1e-9 },
  { name: 'micro', names: ['micro'], symbol: 'μ', factor: 1e-6 },
  { name: 'milli', names: ['milli', 'm'], symbol: 'm', factor: 1e-3 },
  { name: 'centi', names: ['centi', 'c'], symbol: 'c', factor: 1e-2 },
  { name: 'deci', names: ['deci'], symbol: 'd', factor: 1e-1 },
  { name: 'deka', names: ['deka', 'deca'], symbol: 'da', factor: 1e1 },
  { name: 'hecto', names: ['hecto'], symbol: 'h', factor: 1e2 },
  { name: 'kilo', names: ['kilo', 'k'], symbol: 'k', factor: 1e3 },
  { name: 'mega', names: ['mega'], symbol: 'M', factor: 1e6 },
  { name: 'giga', names: ['giga'], symbol: 'G', factor: 1e9 },
  { name: 'tera', names: ['tera'], symbol: 'T', factor: 1e12 },
  { name: 'peta', names: ['peta'], symbol: 'P', factor: 1e15 },
  { name: 'exa', names: ['exa'], symbol: 'E', factor: 1e18 },
  { name: 'zetta', names: ['zetta'], symbol: 'Z', factor: 1e21 },
  { name: 'yotta', names: ['yotta'], symbol: 'Y', factor: 1e24 },
]

/** `ms`, `kV`, `MN`. A squared or cubed base keeps the spelled label, since `km²` would mean 10⁶ m². */
function prefixedLabel(prefix: Prefix, base: Unit): string {
  if (!/[²³]/.test(base.symbol)) return prefix.symbol + base.symbol
  const word = wordNames(base)[0]
  return prefix.name + (word ?? base.symbol)
}

function prefixedUnit(prefix: Prefix, base: Unit, id = `${prefix.name}_${base.id}`): Unit {
  return { id, dim: base.dim, symbol: prefixedLabel(prefix, base), toBase: base.toBase * prefix.factor, defaultTo: base.id, names: [] }
}

const DATA_PREFIXES = [
  ['', '', 1],
  ['k', 'kilo', 1e3],
  ['M', 'mega', 1e6],
  ['G', 'giga', 1e9],
  ['T', 'tera', 1e12],
  ['P', 'peta', 1e15],
  ['E', 'exa', 1e18],
  ['Z', 'zetta', 1e21],
  ['Y', 'yotta', 1e24],
] as const

const BINARY_PREFIXES = [
  ['Ki', 'kibi'],
  ['Mi', 'mebi'],
  ['Gi', 'gibi'],
  ['Ti', 'tebi'],
  ['Pi', 'pebi'],
  ['Ei', 'exbi'],
] as const

// bytes are the base; the byte ids (kb, mb, gb, tb) predate the bit units and stay as they were
function dataUnits(): Unit[] {
  const out: Unit[] = []
  for (const [p, word, f] of DATA_PREFIXES) {
    const id = p.toLowerCase()
    out.push(
      { id: id ? `${id}b` : 'byte', dim: 'digital', symbol: `${p}B`, toBase: f, names: [`${word}byte`, `${word}bytes`], cased: [`${p}B`] },
      // a lone b stays a letter, since `a + b = 10` is an equation, not ares plus bits
      { id: `${id}bit`, dim: 'digital', symbol: p ? `${p}b` : 'bit', toBase: f / 8, names: [`${word}bit`, `${word}bits`], cased: p ? [`${p}b`] : [] },
      {
        id: `${id}bps`,
        dim: 'datarate',
        symbol: `${p}bps`,
        toBase: f / 8,
        names: [`${p}bps`, `${p}bit/s`, `${word}bits per second`],
        cased: [`${p}b/s`],
      },
      { id: `${id}byteps`, dim: 'datarate', symbol: `${p}B/s`, toBase: f, names: [`${word}bytes per second`], cased: [`${p}B/s`] },
    )
  }
  BINARY_PREFIXES.forEach(([p, word], i) => {
    const id = p.toLowerCase()
    out.push({ id: `${id}b`, dim: 'digital', symbol: `${p}B`, toBase: 1024 ** (i + 1), names: [`${id}b`, `${word}byte`, `${word}bytes`] })
  })
  return out
}

const UNIT_LIST: Unit[] = [
  { id: 'in', dim: 'length', symbol: 'in', toBase: 0.0254, defaultTo: 'mm', prefixable: true, names: ['in', 'inch', 'inches', '"'] },
  { id: 'ft', dim: 'length', symbol: 'ft', toBase: FT, defaultTo: 'm', names: ['ft', 'foot', 'feet', "'"] },
  { id: 'yd', dim: 'length', symbol: 'yd', toBase: 0.9144, defaultTo: 'm', names: ['yd', 'yds', 'yard', 'yards'] },
  { id: 'mi', dim: 'length', symbol: 'mi', toBase: MI, defaultTo: 'km', names: ['mi', 'mile', 'miles'] },
  { id: 'nmi', dim: 'length', symbol: 'nmi', toBase: 1852, defaultTo: 'km', names: ['nmi', 'nmile', 'nauticalmile', 'nauticalmiles', 'nautical mile', 'nautical miles'] },
  { id: 'mil', dim: 'length', symbol: 'mil', toBase: 2.54e-5, defaultTo: 'um', names: ['mil', 'mils', 'thou'] },
  { id: 'fathom', dim: 'length', symbol: 'fathom', toBase: 6 * FT, defaultTo: 'm', names: ['fathom', 'fathoms'] },
  { id: 'rod', dim: 'length', symbol: 'rod', toBase: 16.5 * FT, defaultTo: 'm', names: ['rod', 'rods', 'perch', 'pole'] },
  { id: 'chain', dim: 'length', symbol: 'chain', toBase: 66 * FT, defaultTo: 'm', names: ['chain', 'chains'] },
  { id: 'furlong', dim: 'length', symbol: 'furlong', toBase: 660 * FT, defaultTo: 'm', names: ['furlong', 'furlongs'] },
  { id: 'league', dim: 'length', symbol: 'league', toBase: 3 * MI, defaultTo: 'km', names: ['league', 'leagues'] },
  { id: 'fermi', dim: 'length', symbol: 'fm', toBase: 1e-15, defaultTo: 'm', names: ['fermi', 'fermis', 'femtometer', 'femtometers', 'femtometre', 'femtometres'] },
  { id: 'angstrom', dim: 'length', symbol: 'Å', toBase: 1e-10, defaultTo: 'nm', names: ['angstrom', 'angstroms', 'ångström', 'ångstrom'] },
  { id: 'au', dim: 'length', symbol: 'au', toBase: AU, defaultTo: 'km', names: ['au', 'astronomicalunit', 'astronomicalunits', 'astronomical unit', 'astronomical units'] },
  { id: 'ly', dim: 'length', symbol: 'ly', toBase: LIGHT_YEAR, defaultTo: 'km', names: ['ly', 'lightyear', 'lightyears', 'light year', 'light years'] },
  { id: 'pc', dim: 'length', symbol: 'pc', toBase: PARSEC, defaultTo: 'ly', prefixable: true, names: ['pc', 'parsec', 'parsecs'] },
  { id: 'mm', dim: 'length', symbol: 'mm', toBase: 0.001, defaultTo: 'in', names: ['mm', 'millimeter', 'millimeters', 'millimetre', 'millimetres'] },
  { id: 'cm', dim: 'length', symbol: 'cm', toBase: 0.01, defaultTo: 'in', names: ['cm', 'centimeter', 'centimeters', 'centimetre', 'centimetres'] },
  { id: 'dm', dim: 'length', symbol: 'dm', toBase: 0.1, defaultTo: 'in', names: ['dm', 'decimeter', 'decimeters', 'decimetre', 'decimetres'] },
  { id: 'm', dim: 'length', symbol: 'm', toBase: 1, defaultTo: 'ft', prefixable: true, names: ['m', 'meter', 'meters', 'metre', 'metres'] },
  { id: 'km', dim: 'length', symbol: 'km', toBase: 1000, defaultTo: 'mi', names: ['km', 'kilometer', 'kilometers', 'kilometre', 'kilometres'] },
  { id: 'um', dim: 'length', symbol: 'μm', toBase: 1e-6, defaultTo: 'mil', names: ['um', 'μm', 'micron', 'microns', 'micrometer', 'micrometers', 'micrometre', 'micrometres'] },
  { id: 'nm', dim: 'length', symbol: 'nm', toBase: 1e-9, defaultTo: 'in', names: ['nm', 'nanometer', 'nanometers', 'nanometre', 'nanometres'] },

  { id: 'oz', dim: 'mass', symbol: 'oz', toBase: 28.349523125 / 1000, defaultTo: 'g', names: ['oz', 'ozm', 'ounce', 'ounces', 'ouncemass', 'ounce mass'] },
  { id: 'lb', dim: 'mass', symbol: 'lbs', toBase: LB, defaultTo: 'kg', names: ['lb', 'lbs', 'lbm', 'pound', 'pounds', 'poundmass', 'pound mass'] },
  { id: 'st', dim: 'mass', symbol: 'st', toBase: 6.35029318, defaultTo: 'kg', names: ['st', 'stone', 'stones'] },
  { id: 'ton', dim: 'mass', symbol: 'ton', toBase: 2000 * LB, defaultTo: 'tonne', names: ['ton', 'tons', 'shortton', 'shorttons', 'short ton', 'short tons', 'us short ton', 'us short tons'] },
  { id: 'longton', dim: 'mass', symbol: 'long ton', toBase: 2240 * LB, defaultTo: 'tonne', names: ['longton', 'longtons', 'long ton', 'long tons', 'imperial ton', 'imperial tons', 'uk ton', 'uk tons'] },
  { id: 'grain', dim: 'mass', symbol: 'gr', toBase: 64.79891e-6, defaultTo: 'mg', names: ['gr', 'grain', 'grains'] },
  { id: 'slug', dim: 'mass', symbol: 'slug', toBase: SLUG, defaultTo: 'kg', names: ['slug', 'slugs'] },
  { id: 'slinch', dim: 'mass', symbol: 'slinch', toBase: SLINCH, defaultTo: 'kg', names: ['slinch', 'snail', 'snails'] },
  { id: 'amu', dim: 'mass', symbol: 'u', toBase: AMU, defaultTo: 'kg', names: ['u', 'amu', 'atomicmassunit', 'atomicmassunits', 'atomic mass unit', 'atomic mass units'] },
  { id: 'emass', dim: 'mass', symbol: 'mₑ', toBase: 9.1093837015e-31, defaultTo: 'kg', names: ['electronrestmass', 'electron rest mass'] },
  { id: 'pmass', dim: 'mass', symbol: 'mₚ', toBase: 1.007276466621 * AMU, defaultTo: 'kg', names: ['protonrestmass', 'proton rest mass'] },
  { id: 'nmass', dim: 'mass', symbol: 'mₙ', toBase: 1.00866491588 * AMU, defaultTo: 'kg', names: ['neutronrestmass', 'neutron rest mass'] },
  { id: 'mg', dim: 'mass', symbol: 'mg', toBase: 1e-6, defaultTo: 'grain', names: ['mg', 'milligram', 'milligrams'] },
  { id: 'g', dim: 'mass', symbol: 'g', toBase: 0.001, defaultTo: 'oz', prefixable: true, names: ['g', 'gram', 'grams', 'gramme', 'grammes'] },
  { id: 'kg', dim: 'mass', symbol: 'kg', toBase: 1, defaultTo: 'lb', names: ['kg', 'kilogram', 'kilograms'] },
  { id: 'tonne', dim: 'mass', symbol: 't', toBase: 1000, defaultTo: 'ton', prefixable: true, names: ['t', 'tonne', 'tonnes', 'metricton', 'metrictons', 'metric ton', 'metric tons'] },

  // °C and °F are offset scales: conversions only, never arithmetic
  { id: 'c', dim: 'temperature', symbol: '°C', toBase: 0, defaultTo: 'f', names: ['c', 'celsius', 'centigrade', 'degc', 'deg c', 'degree celsius', 'degrees celsius', '°c'] },
  { id: 'f', dim: 'temperature', symbol: '°F', toBase: 0, defaultTo: 'c', names: ['f', 'fahrenheit', 'degf', 'deg f', 'degree fahrenheit', 'degrees fahrenheit', '°f'] },
  { id: 'k', dim: 'temperature', symbol: 'K', toBase: 1, defaultTo: 'r', names: ['k', 'kelvin', 'kelvins'] },
  { id: 'r', dim: 'temperature', symbol: '°R', toBase: 5 / 9, defaultTo: 'k', names: ['r', 'rankine', 'degr', 'deg r', 'degree rankine', 'degrees rankine', '°r'] },

  { id: 'drop', dim: 'volume', symbol: 'drop', toBase: 5e-5, defaultTo: 'ml', names: ['drop', 'drops'] },
  { id: 'tsp', dim: 'volume', symbol: 'tsp', toBase: US_FLOZ / 6, defaultTo: 'ml', names: ['tsp', 'teaspoon', 'teaspoons'] },
  { id: 'tbsp', dim: 'volume', symbol: 'tbsp', toBase: US_FLOZ / 2, defaultTo: 'ml', names: ['tbs', 'tbsp', 'tablespoon', 'tablespoons'] },
  { id: 'floz', dim: 'volume', symbol: 'fl oz', toBase: US_FLOZ, defaultTo: 'ml', names: ['floz', 'flozs', 'fl oz', 'fluidounce', 'fluidounces', 'fluid ounce', 'fluid ounces', 'us fluid ounce', 'us fluid ounces'] },
  { id: 'cup', dim: 'volume', symbol: 'cup', toBase: 0.2365882365, defaultTo: 'ml', names: ['cup', 'cups', 'us cup', 'us cups'] },
  { id: 'pt', dim: 'volume', symbol: 'pt', toBase: 0.473176473, defaultTo: 'l', names: ['pt', 'pint', 'pints', 'us pint', 'us pints'] },
  { id: 'qt', dim: 'volume', symbol: 'qt', toBase: 0.946352946, defaultTo: 'l', names: ['qt', 'quart', 'quarts', 'us quart', 'us quarts'] },
  { id: 'usgal', dim: 'volume', symbol: 'gal', toBase: US_GAL, defaultTo: 'l', names: ['gal', 'gallon', 'gallons', 'us gallon', 'us gallons'] },
  { id: 'impgal', dim: 'volume', symbol: 'imp gal', toBase: 4.54609, defaultTo: 'l', names: ['imp gal', 'imperial gallon', 'imperial gallons', 'uk gallon', 'uk gallons'] },
  { id: 'peck', dim: 'volume', symbol: 'peck', toBase: US_BUSHEL / 4, defaultTo: 'l', names: ['peck', 'pecks'] },
  { id: 'bushel', dim: 'volume', symbol: 'bu', toBase: US_BUSHEL, defaultTo: 'l', names: ['bu', 'bushel', 'bushels'] },
  { id: 'barrel', dim: 'volume', symbol: 'bbl', toBase: OIL_BBL, defaultTo: 'l', names: ['bbl', 'barrel', 'barrels', 'oil barrel', 'oil barrels'] },
  { id: 'cord', dim: 'volume', symbol: 'cord', toBase: CORD, defaultTo: 'm3', names: ['cord', 'cords'] },
  { id: 'in3', dim: 'volume', symbol: 'in³', toBase: IN3, defaultTo: 'cm3', names: ['in3', 'in^3', 'cu in', 'cuin', 'cubic inch', 'cubic inches'] },
  { id: 'ft3', dim: 'volume', symbol: 'ft³', toBase: 28.316846592, defaultTo: 'm3', names: ['ft3', 'ft^3', 'cu ft', 'cuft', 'cubic foot', 'cubic feet'] },
  { id: 'yd3', dim: 'volume', symbol: 'yd³', toBase: 764.554857984, defaultTo: 'm3', names: ['yd3', 'yd^3', 'cu yd', 'cubic yard', 'cubic yards'] },
  { id: 'ml', dim: 'volume', symbol: 'mL', toBase: 0.001, defaultTo: 'floz', names: ['ml', 'milliliter', 'milliliters', 'millilitre', 'millilitres', 'cc', 'ccs'] },
  { id: 'cm3', dim: 'volume', symbol: 'cm³', toBase: 0.001, defaultTo: 'in3', names: ['cm3', 'cm^3', 'cubic centimeter', 'cubic centimeters', 'cubic centimetre', 'cubic centimetres'] },
  { id: 'l', dim: 'volume', symbol: 'L', toBase: 1, defaultTo: 'usgal', prefixable: true, names: ['l', 'liter', 'liters', 'litre', 'litres'] },
  { id: 'stere', dim: 'volume', symbol: 'stere', toBase: 1000, defaultTo: 'ft3', names: ['stere', 'steres'] },
  { id: 'm3', dim: 'volume', symbol: 'm³', toBase: 1000, defaultTo: 'ft3', names: ['m3', 'm^3', 'cubic meter', 'cubic meters', 'cubic metre', 'cubic metres'] },

  { id: 'barn', dim: 'area', symbol: 'barn', toBase: 1e-28, defaultTo: 'm2', prefixable: true, names: ['barn', 'barns'] },
  { id: 'darcy', dim: 'area', symbol: 'D', toBase: DARCY, defaultTo: 'm2', names: ['darcy', 'darcys', 'darcies'] },
  { id: 'in2', dim: 'area', symbol: 'in²', toBase: 0.00064516, defaultTo: 'cm2', names: ['in2', 'in^2', 'sqin', 'sq in', 'square inch', 'square inches'] },
  { id: 'sqft', dim: 'area', symbol: 'ft²', toBase: 0.09290304, defaultTo: 'm2', names: ['sqft', 'sq ft', 'ft2', 'ft^2', 'square foot', 'square feet'] },
  { id: 'yd2', dim: 'area', symbol: 'yd²', toBase: 0.83612736, defaultTo: 'm2', names: ['yd2', 'yd^2', 'sq yd', 'square yard', 'square yards'] },
  { id: 'acre', dim: 'area', symbol: 'acres', toBase: ACRE, defaultTo: 'ha', names: ['acre', 'acres'] },
  { id: 'sqmi', dim: 'area', symbol: 'mi²', toBase: MI * MI, defaultTo: 'km2', names: ['sqmi', 'sq mi', 'mi2', 'mi^2', 'square mile', 'square miles'] },
  { id: 'cm2', dim: 'area', symbol: 'cm²', toBase: 0.0001, defaultTo: 'in2', names: ['cm2', 'cm^2', 'sq cm', 'square centimeter', 'square centimeters', 'square centimetre', 'square centimetres'] },
  { id: 'm2', dim: 'area', symbol: 'm²', toBase: 1, defaultTo: 'sqft', prefixable: true, names: ['m2', 'm^2', 'sqm', 'sq m', 'square meter', 'square meters', 'square metre', 'square metres'] },
  { id: 'km2', dim: 'area', symbol: 'km²', toBase: 1e6, defaultTo: 'sqmi', names: ['km2', 'km^2', 'sq km', 'square kilometer', 'square kilometers', 'square kilometre', 'square kilometres'] },
  { id: 'ha', dim: 'area', symbol: 'ha', toBase: 10000, defaultTo: 'acre', names: ['ha', 'hectare', 'hectares'] },

  { id: 'mph', dim: 'speed', symbol: 'mph', toBase: MI / 3600, defaultTo: 'kmh', names: ['mph', 'mi/h', 'mile/h', 'mile per hour', 'miles per hour'] },
  { id: 'fps', dim: 'speed', symbol: 'ft/s', toBase: FT, defaultTo: 'mps', names: ['fps', 'ft/s', 'foot per second', 'feet per second'] },
  { id: 'knot', dim: 'speed', symbol: 'kn', toBase: 1852 / 3600, defaultTo: 'kmh', names: ['kt', 'knot', 'knots'] },
  { id: 'kmh', dim: 'speed', symbol: 'km/h', toBase: 1000 / 3600, defaultTo: 'mph', names: ['km/h', 'km/hr', 'kph', 'kmh', 'kilometer per hour', 'kilometers per hour', 'kilometre per hour', 'kilometres per hour'] },
  { id: 'mps', dim: 'speed', symbol: 'm/s', toBase: 1, defaultTo: 'fps', names: ['m/s', 'meter per second', 'meters per second', 'metre per second', 'metres per second'] },
  { id: 'light', dim: 'speed', symbol: 'c', toBase: C, defaultTo: 'mps', names: ['lightspeed', 'speedoflight', 'speed of light'] },

  // time and digital have no defaultTo: they convert only with an explicit "to"
  { id: 'hr', dim: 'time', symbol: 'hr', toBase: 3600, names: ['h', 'hr', 'hrs', 'hour', 'hours'] },
  { id: 's', dim: 'time', symbol: 's', toBase: 1, prefixable: true, names: ['s', 'sec', 'secs', 'second', 'seconds'] },
  { id: 'day', dim: 'time', symbol: 'd', toBase: 86400, names: ['d', 'day', 'days'] },
  { id: 'min', dim: 'time', symbol: 'min', toBase: 60, names: ['min', 'mins', 'minute', 'minutes'] },
  { id: 'wk', dim: 'time', symbol: 'wk', toBase: 604800, names: ['wk', 'wks', 'week', 'weeks'] },
  { id: 'yr', dim: 'time', symbol: 'yr', toBase: YEAR, names: ['yr', 'yrs', 'year', 'years'] },
  { id: 'month', dim: 'time', symbol: 'mo', toBase: YEAR / 12, names: ['mo', 'mos', 'month', 'months'] },
  { id: 'decade', dim: 'time', symbol: 'decades', toBase: 10 * YEAR, names: ['decade', 'decades'] },
  { id: 'century', dim: 'time', symbol: 'centuries', toBase: 100 * YEAR, names: ['century', 'centuries'] },
  { id: 'millennium', dim: 'time', symbol: 'kyr', toBase: 1000 * YEAR, names: ['millenium', 'millennium', 'millenniums', 'millennia'] },

  ...dataUnits(),

  { id: 'ev', dim: 'energy', symbol: 'eV', toBase: E_CHARGE, defaultTo: 'j', prefixable: true, names: ['ev', 'evs', 'electronvolt', 'electronvolts', 'electron volt', 'electron volts'] },
  { id: 'erg', dim: 'energy', symbol: 'erg', toBase: 1e-7, defaultTo: 'j', names: ['erg', 'ergs'] },
  { id: 'cal', dim: 'energy', symbol: 'cal', toBase: 4.184, defaultTo: 'j', names: ['cal', 'calorie', 'calories', 'thermodynamic calorie', 'thermodynamic calories'] },
  { id: 'kcal', dim: 'energy', symbol: 'kcal', toBase: 4184, defaultTo: 'kj', names: ['kcal', 'kilocalorie', 'kilocalories', 'food calorie', 'food calories'], exactNames: ['Cal'] },
  { id: 'btu', dim: 'energy', symbol: 'BTU', toBase: 1055.05585262, defaultTo: 'kj', names: ['btu', 'btus', 'british thermal unit', 'british thermal units'] },
  { id: 'therm', dim: 'energy', symbol: 'thm', toBase: THERM, defaultTo: 'btu', names: ['thm', 'therm', 'therms'] },
  { id: 'j', dim: 'energy', symbol: 'J', toBase: 1, defaultTo: 'cal', prefixable: true, names: ['j', 'joule', 'joules'] },
  { id: 'kj', dim: 'energy', symbol: 'kJ', toBase: 1000, defaultTo: 'btu', names: ['kj', 'kilojoule', 'kilojoules'] },
  { id: 'wh', dim: 'energy', symbol: 'Wh', toBase: 3600, defaultTo: 'kj', names: ['wh', 'watt hour', 'watt hours'] },
  { id: 'kwh', dim: 'energy', symbol: 'kWh', toBase: 3.6e6, defaultTo: 'btu', names: ['kwh', 'kw h', 'kilowatt hour', 'kilowatt hours'] },

  { id: 'hp', dim: 'power', symbol: 'hp', toBase: 745.699872, defaultTo: 'kw', names: ['hp', 'horsepower', 'mechanical horsepower'] },
  { id: 'w', dim: 'power', symbol: 'W', toBase: 1, defaultTo: 'hp', prefixable: true, names: ['w', 'watt', 'watts'] },
  { id: 'kw', dim: 'power', symbol: 'kW', toBase: 1000, defaultTo: 'hp', names: ['kw', 'kilowatt', 'kilowatts'] },

  { id: 'psi', dim: 'pressure', symbol: 'psi', toBase: PSI, defaultTo: 'kpa', names: ['psi', 'pound per square inch', 'pounds per square inch'] },
  { id: 'ksi', dim: 'pressure', symbol: 'ksi', toBase: PSI * 1000, defaultTo: 'mpa', names: ['ksi'] },
  { id: 'inhg', dim: 'pressure', symbol: 'inHg', toBase: 3386.389, defaultTo: 'kpa', names: ['inhg', 'in hg', 'inch of mercury', 'inches of mercury'] },
  { id: 'pa', dim: 'pressure', symbol: 'Pa', toBase: 1, defaultTo: 'psi', prefixable: true, names: ['pa', 'pascal', 'pascals'] },
  { id: 'kpa', dim: 'pressure', symbol: 'kPa', toBase: 1000, defaultTo: 'psi', names: ['kpa', 'kilopascal', 'kilopascals'] },
  { id: 'mpa', dim: 'pressure', symbol: 'MPa', toBase: 1e6, defaultTo: 'ksi', names: ['mpa', 'megapascal', 'megapascals'] },
  { id: 'bar', dim: 'pressure', symbol: 'bar', toBase: 1e5, defaultTo: 'psi', names: ['bar', 'bars'] },
  { id: 'atm', dim: 'pressure', symbol: 'atm', toBase: 101325, defaultTo: 'psi', names: ['atm', 'atmosphere', 'atmospheres'] },
  { id: 'torr', dim: 'pressure', symbol: 'torr', toBase: 101325 / 760, defaultTo: 'psi', names: ['torr', 'torrs', 'mmhg', 'mm hg', 'millimeter of mercury', 'millimeters of mercury'] },

  { id: 'dyn', dim: 'force', symbol: 'dyn', toBase: 1e-5, defaultTo: 'n', names: ['dyn', 'dyne', 'dynes'] },
  { id: 'ozf', dim: 'force', symbol: 'ozf', toBase: LBF / 16, defaultTo: 'n', names: ['ozf', 'ounceforce', 'ounce-force', 'ounce force'] },
  { id: 'lbf', dim: 'force', symbol: 'lbf', toBase: LBF, defaultTo: 'n', names: ['lbf', 'poundforce', 'pound-force', 'pounds-force', 'pound force'] },
  { id: 'kip', dim: 'force', symbol: 'kip', toBase: 1000 * LBF, defaultTo: 'n', names: ['kip', 'kips'] },
  { id: 'n', dim: 'force', symbol: 'N', toBase: 1, defaultTo: 'lbf', prefixable: true, names: ['n', 'newton', 'newtons'] },
  { id: 'kilonewton', dim: 'force', symbol: 'kN', toBase: 1000, defaultTo: 'lbf', names: ['kn', 'kilonewton', 'kilonewtons'] },
  { id: 'kgf', dim: 'force', symbol: 'kgf', toBase: G0, defaultTo: 'n', names: ['kgf', 'kgforce', 'kilogramforce', 'kilogram-force', 'kilogram force'] },

  { id: 'arcsec', dim: 'angle', symbol: '″', toBase: Math.PI / (180 * 3600), defaultTo: 'deg', names: ['arcsec', 'arcsecs', 'arcsecond', 'arcseconds', 'arc sec', 'arc secs', 'arc second', 'arc seconds'] },
  { id: 'arcmin', dim: 'angle', symbol: '′', toBase: Math.PI / (180 * 60), defaultTo: 'deg', names: ['arcmin', 'arcmins', 'arcminute', 'arcminutes', 'arc min', 'arc mins', 'arc minute', 'arc minutes'] },
  { id: 'deg', dim: 'angle', symbol: 'deg', toBase: Math.PI / 180, defaultTo: 'rad', names: ['deg', 'degs', 'degree', 'degrees'] },
  { id: 'rad', dim: 'angle', symbol: 'rad', toBase: 1, defaultTo: 'deg', prefixable: true, names: ['rad', 'rads', 'radian', 'radians'] },
  { id: 'rev', dim: 'angle', symbol: 'rev', toBase: 2 * Math.PI, defaultTo: 'deg', names: ['rev', 'revs', 'revolution', 'revolutions'] },

  { id: 'hz', dim: 'frequency', symbol: 'Hz', toBase: 1, defaultTo: 'rpm', prefixable: true, names: ['hz', 'hertz'] },
  { id: 'rpm', dim: 'frequency', symbol: 'rpm', toBase: 1 / 60, defaultTo: 'hz', names: ['rpm', 'rpms', 'revolution per minute', 'revolutions per minute'] },

  { id: 'mps2', dim: 'acceleration', symbol: 'm/s²', toBase: 1, defaultTo: 'gee', names: ['m/s2', 'm/s^2', 'm/s²', 'meter per second squared', 'meters per second squared', 'metre per second squared', 'metres per second squared'] },
  { id: 'gee', dim: 'acceleration', symbol: 'g', toBase: G0, defaultTo: 'mps2', names: ['gs', 'gee', 'gravity'] },

  { id: 'electron', dim: 'charge', symbol: 'e', toBase: E_CHARGE, defaultTo: 'coulomb', names: ['electron', 'electrons'] },
  { id: 'photon', dim: 'dimensionless', symbol: 'photon', toBase: 1, names: ['photon', 'photons'] },
  { id: 'coulomb', dim: 'charge', symbol: 'C', toBase: 1, defaultTo: 'electron', prefixable: true, names: ['coulomb', 'coulombs'] },

  { id: 'amp', dim: 'current', symbol: 'A', toBase: 1, prefixable: true, names: ['a', 'amp', 'amps', 'ampere', 'amperes'] },
  { id: 'volt', dim: 'voltage', symbol: 'V', toBase: 1, prefixable: true, names: ['v', 'volt', 'volts'] },
  { id: 'ohm', dim: 'resistance', symbol: 'Ω', toBase: 1, prefixable: true, names: ['ohm', 'ohms', 'Ω'] },
  { id: 'farad', dim: 'capacitance', symbol: 'F', toBase: 1, prefixable: true, names: ['farad', 'farads'] },
  { id: 'henry', dim: 'inductance', symbol: 'H', toBase: 1, prefixable: true, names: ['henry', 'henrys', 'henries'] },

  // "1" is left out on purpose; it would steal trailing digits
  { id: 'none', dim: 'dimensionless', symbol: 'dimensionless', toBase: 1, names: ['dimensionless', 'none', 'nounit', 'nounits'] },
  { id: 'unit', dim: 'dimensionless', symbol: 'units', toBase: 1, names: ['unit', 'units'] },
  ...PREFIXES.map((p) => ({
    id: `prefix_${p.name}`,
    dim: 'dimensionless' as const,
    symbol: p.name,
    toBase: p.factor,
    names: p.names.filter((n) => n.length > 1),
  })).filter((p) => p.names.length),
]

function sameScale(a: number, b: number): boolean {
  const scale = Math.max(Math.abs(a), Math.abs(b), 1e-300)
  return Math.abs(a - b) / scale < 1e-12
}

function wordNames(unit: Unit): string[] {
  return unit.names.filter((n) => /^[A-Za-z]{3,}$/.test(n))
}

function takenAliases(units: Unit[]): Set<string> {
  const taken = new Set<string>()
  for (const unit of units) {
    for (const name of unit.cased ?? []) taken.add(name.toLowerCase())
    for (const name of unit.names) {
      taken.add(name.toLowerCase())
      const norm = normalizeName(name)
      if (norm) taken.add(norm)
    }
  }
  return taken
}

function applySiPrefixes(units: Unit[]): void {
  const taken = takenAliases(units)
  const roots = units.filter((u) => u.prefixable)
  const families = new Map<Dim, Unit[]>()
  for (const u of units) {
    const family = families.get(u.dim)
    if (family) family.push(u)
    else families.set(u.dim, [u])
  }
  for (const base of roots) {
    const family = families.get(base.dim)!
    const words = wordNames(base)
    for (const prefix of PREFIXES) {
      const toBase = prefix.factor * base.toBase
      if (!Number.isFinite(toBase) || toBase === 0) continue
      const candidates: string[] = []
      for (const word of words) {
        for (const pname of prefix.names) {
          if (pname.length === 1) continue
          candidates.push(pname + word)
        }
      }
      const fresh = [...new Set(candidates.map((a) => a.toLowerCase()))].filter((a) => a && !taken.has(a))
      if (!fresh.length) continue
      const existing = family.find((u) => sameScale(u.toBase, toBase))
      if (existing) {
        existing.names.push(...fresh)
        for (const a of fresh) taken.add(a)
        continue
      }
      const unit: Unit = { ...prefixedUnit(prefix, base), names: fresh }
      units.push(unit)
      family.push(unit)
      for (const a of fresh) taken.add(a)
    }
  }
}

function normalizeName(s: string): string {
  return s
    .trim()
    .toLowerCase()
    .replace(/°\s*c\b/g, 'c')
    .replace(/°\s*f\b/g, 'f')
    .replace(/°\s*r\b/g, 'r')
    .replace(/°/g, 'deg')
    .replace(/µ|μ/g, 'u')
    .replace(/²/g, '2')
    .replace(/³/g, '3')
    .replace(/\^/g, '')
    .replace(/\s*\/\s*/g, '/')
    .replace(/[_.,]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

applySiPrefixes(UNIT_LIST)

{
  const megaohm = UNIT_LIST.find((u) => u.dim === 'resistance' && sameScale(u.toBase, 1e6))
  megaohm?.names.push('megohm', 'megohms')
  const microfarad = UNIT_LIST.find((u) => u.dim === 'capacitance' && sameScale(u.toBase, 1e-6))
  microfarad?.names.push('uf')
}

const BY_ID = new Map<string, Unit>()
for (const unit of UNIT_LIST) {
  BY_ID.set(unit.id, unit)
}

function toKelvin(n: number, id: string): number {
  if (id === 'c') return n + 273.15
  if (id === 'f') return ((n - 32) * 5) / 9 + 273.15
  if (id === 'r') return (n * 5) / 9
  return n
}

function fromKelvin(k: number, id: string): number {
  if (id === 'c') return k - 273.15
  if (id === 'f') return ((k - 273.15) * 9) / 5 + 32
  if (id === 'r') return (k * 9) / 5
  return k
}

function convertAmount(amount: number, from: Unit, to: Unit): Value | null {
  if (from.dim !== to.dim) return unitError()
  let n: number
  if (from.dim === 'temperature') n = fromKelvin(toKelvin(amount, from.id), to.id)
  else n = (amount * from.toBase) / to.toBase
  if (!Number.isFinite(n)) return unitError()
  return { ...num(n), unit: to.symbol, unitId: to.id }
}

const ALIAS_INDEX: { alias: string; unit: Unit; exact?: string; whole?: boolean }[] = []
{
  const seen = new Set<string>()
  for (const unit of UNIT_LIST) {
    for (const exact of unit.cased ?? []) ALIAS_INDEX.push({ alias: exact.toLowerCase(), unit, exact })
    for (const exact of unit.exactNames ?? []) ALIAS_INDEX.push({ alias: exact.toLowerCase(), unit, exact, whole: true })
    for (const name of unit.names) {
      for (const alias of [name.toLowerCase(), normalizeName(name)]) {
        if (!alias) continue
        const key = `${alias}\0${unit.id}`
        if (seen.has(key)) continue
        seen.add(key)
        ALIAS_INDEX.push({ alias, unit })
      }
    }
  }
  // longest first, and a case-sensitive name before a plain one of the same length
  ALIAS_INDEX.sort((a, b) => b.alias.length - a.alias.length || Number(Boolean(b.exact)) - Number(Boolean(a.exact)))
}

const ALIAS_NAMES = new Set(ALIAS_INDEX.map((a) => a.alias))

/** A unit's own name or symbol, not a prefix glued onto one (`kilom` is not a name). */
export function isUnitName(word: string): boolean {
  return ALIAS_NAMES.has(word.toLowerCase())
}

const PREFIX_INDEX = PREFIXES.flatMap((p) => p.names.map((name) => ({ name: name.toLowerCase(), prefix: p }))).sort(
  (a, b) => b.name.length - a.name.length,
)

// volume's table base is the litre, not m³
const SI_SCALE: Partial<Record<Dim, number>> = { volume: 0.001 }

function siOf(unit: Unit): number {
  return unit.toBase * (SI_SCALE[unit.dim] ?? 1)
}

const PREFIX_BY_NAME = new Map(PREFIXES.map((p) => [p.name, p]))

/** Case-sensitive one-letter prefixes (`MN`, `mN`, `us` for µs after preprocess). */
const CASE_PREFIXES: { symbol: string; prefix: Prefix }[] = (
  [
    ['Y', 'yotta'],
    ['Z', 'zetta'],
    ['E', 'exa'],
    ['P', 'peta'],
    ['T', 'tera'],
    ['G', 'giga'],
    ['M', 'mega'],
    ['k', 'kilo'],
    ['h', 'hecto'],
    ['d', 'deci'],
    ['c', 'centi'],
    ['m', 'milli'],
    ['u', 'micro'],
    ['n', 'nano'],
    ['p', 'pico'],
    ['f', 'femto'],
    ['a', 'atto'],
    ['z', 'zepto'],
    ['y', 'yocto'],
  ] as const
).map(([symbol, name]) => ({ symbol, prefix: PREFIX_BY_NAME.get(name)! }))

function preprocess(s: string): string {
  return s
    .replace(/(\d+)'(\d+(?:\.\d+)?)"/g, "$1 ft $2 in")
    .replace(/π/g, 'pi')
    .replace(/τ/g, 'tau')
    .replace(/−/g, '-')
    .replace(/°\s*C\b/gi, ' degc')
    .replace(/°\s*F\b/gi, ' degf')
    .replace(/°\s*R\b/gi, ' degr')
    .replace(/°/g, ' deg')
    .replace(/[µμ](?=[A-Za-z])/g, 'u')
    .replace(/²/g, '^2')
    .replace(/³/g, '^3')
    .replace(/\s*\/\s*/g, '/')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^convert\s+/i, '')
}

function isLetter(c: string | undefined): boolean {
  return !!c && /[A-Za-z]/.test(c)
}

/** The text before `token` at the end of `t`, or null when it isn't a separate token there. */
function beforeTail(t: string, token: string): string | null {
  if (t.length <= token.length) return null
  const rest = t.slice(0, t.length - token.length)
  if (t.slice(rest.length).toLowerCase() !== token) return null
  if (isLetter(rest[rest.length - 1]) && isLetter(token[0])) return null
  return rest.trim() ? rest : null
}

/** `mPa` is a millipascal and `Mg` a megagram, though the table's `MPa` and `mg` match them ignoring case. */
function caseClash(typed: string, unit: Unit): boolean {
  const sym = unit.symbol
  // only the prefix letter's case differs; `MG` or `MPH` in caps keep their table reading
  if (typed[0] === sym[0] || typed.slice(1) !== sym.slice(1) || typed[0]!.toLowerCase() !== sym[0]!.toLowerCase()) return false
  const prefix = CASE_PREFIXES.find((p) => p.symbol === typed[0])
  const base = prefix && matchBareUnitAtStart(typed.slice(1))
  return Boolean(base && !base.rest && scaleUnit(base.unit, prefix.prefix))
}

function caseFits(typed: string, exact: string | undefined, whole = false): boolean {
  if (!exact) return true
  if (whole) return typed === exact
  for (let i = 0; i < exact.length; i++) {
    if ((exact[i] === 'b' || exact[i] === 'B') && typed[i] !== exact[i]) return false
  }
  return true
}

function matchUnitAtEnd(s: string): { unit: Unit; rest: string } | null {
  const t = s.trimEnd()
  for (const { alias, unit, exact, whole } of ALIAS_INDEX) {
    const rest = beforeTail(t, alias)
    if (rest == null) continue
    const typed = t.slice(rest.length)
    if (!caseFits(typed, exact, whole) || caseClash(typed, unit)) continue
    // `1 H` is a henry, hours are `h`/`hr`; `72 F` and `100 C` stay temperatures here
    return { unit: t.slice(rest.length) === 'H' ? BY_ID.get('henry')! : unit, rest: rest.trimEnd() }
  }
  return null
}

function scaleUnit(base: Unit, prefix: Prefix): Unit | null {
  if (base.dim === 'temperature' || base.dim === 'digital' || base.dim === 'dimensionless') return null
  // of the time units only seconds take prefixes (`ms`, `µs`)
  if (base.dim === 'time' && base.id !== 's') return null
  const toBase = base.toBase * prefix.factor
  if (!Number.isFinite(toBase) || toBase === 0) return null
  return UNIT_LIST.find((u) => u.dim === base.dim && sameScale(u.toBase, toBase)) ?? prefixedUnit(prefix, base)
}

function matchCasePrefixAtEnd(rest: string, unit: Unit): { unit: Unit; rest: string } | null {
  for (const p of CASE_PREFIXES) {
    if (rest.length <= p.symbol.length || !rest.endsWith(p.symbol)) continue
    const before = rest.slice(0, rest.length - p.symbol.length)
    const prev = before[before.length - 1]
    if (isLetter(prev)) continue
    if (!before.trim()) continue
    const scaled = scaleUnit(unit, p.prefix)
    if (!scaled) continue
    return { unit: scaled, rest: before.trimEnd() }
  }
  return null
}

function matchPrefixedUnit(s: string): { unit: Unit; rest: string } | null {
  const end = matchUnitAtEnd(s)
  if (!end) return null
  const t = end.rest.trimEnd()
  const cased = matchCasePrefixAtEnd(t, end.unit)
  if (cased) return cased
  for (const { name, prefix } of PREFIX_INDEX) {
    if (name.length === 1) continue
    const rest = beforeTail(t, name)
    if (rest == null) continue
    const scaled = scaleUnit(end.unit, prefix)
    if (!scaled) continue
    return { unit: scaled, rest: rest.trimEnd() }
  }
  return end
}

function parseAmount(expr: string): number | null {
  const t = expr.trim()
  if (!t) return null
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(t)) return null
  const n = Number(t)
  return Number.isFinite(n) ? n : null
}

function startsWithToken(s: string, token: string): boolean {
  if (s.length < token.length || s.slice(0, token.length).toLowerCase() !== token) return false
  const next = s[token.length]
  const last = token[token.length - 1]
  if (next && /[A-Za-z0-9]/.test(next) && last && /[A-Za-z0-9]/.test(last)) return false
  // `ft/s^2` is ft / s^2, not (ft/s)^2
  if (token.includes('/') && next === '^') return false
  return true
}

/**
 * In arithmetic the capital SI symbols win: `F`, `C`, `H` are farad, coulomb, henry (`1 F * 1 V` = 1 C).
 * Temperatures are written `°F`/`°C` or appear in a plain conversion (`72 F`, `100 C in F`).
 */
const SI_CAPITALS: Record<string, string> = { F: 'farad', C: 'coulomb', H: 'henry' }

function matchBareUnitAtStart(s: string): { unit: Unit; rest: string } | null {
  const t = s.trimStart()
  const si = SI_CAPITALS[t[0] ?? '']
  if (si && !/[A-Za-z0-9]/.test(t[1] ?? '')) return { unit: BY_ID.get(si)!, rest: t.slice(1) }
  for (const { alias, unit, exact, whole } of ALIAS_INDEX) {
    if (!startsWithToken(t, alias)) continue
    const typed = t.slice(0, alias.length)
    if (!caseFits(typed, exact, whole) || caseClash(typed, unit)) continue
    return { unit, rest: t.slice(alias.length) }
  }
  return null
}

function prefixedUnitAtStart(prefix: Prefix, s: string): { unit: Unit; rest: string } | null {
  const u = matchBareUnitAtStart(s)
  const scaled = u && scaleUnit(u.unit, prefix)
  return scaled ? { unit: scaled, rest: u!.rest } : null
}

function matchSpelledPrefixAtStart(t: string): { unit: Unit; rest: string } | null {
  for (const { name, prefix } of PREFIX_INDEX) {
    if (name.length === 1) continue
    if (t.length < name.length || t.slice(0, name.length).toLowerCase() !== name) continue
    const hit = prefixedUnitAtStart(prefix, t.slice(name.length))
    if (hit) return hit
  }
  return null
}

function matchUnitAtStart(s: string): { unit: Unit; rest: string } | null {
  const t = s.trimStart()
  const prefixed = matchSpelledPrefixAtStart(t)
  const bare = matchBareUnitAtStart(t)
  // the longer match wins; a tie goes to the bare unit
  if (prefixed && (!bare || prefixed.rest.length < bare.rest.length)) return prefixed
  if (bare) return bare
  for (const p of CASE_PREFIXES) {
    if (t[0] !== p.symbol) continue
    const hit = prefixedUnitAtStart(p.prefix, t.slice(p.symbol.length))
    if (hit) return hit
  }
  return null
}

type DimVec = readonly number[]

const DIM_VEC: Record<Dim, DimVec> = {
  mass: [1, 0, 0, 0, 0, 0, 0],
  length: [0, 1, 0, 0, 0, 0, 0],
  time: [0, 0, 1, 0, 0, 0, 0],
  current: [0, 0, 0, 1, 0, 0, 0],
  temperature: [0, 0, 0, 0, 1, 0, 0],
  angle: [0, 0, 0, 0, 0, 1, 0],
  speed: [0, 1, -1, 0, 0, 0, 0],
  acceleration: [0, 1, -2, 0, 0, 0, 0],
  force: [1, 1, -2, 0, 0, 0, 0],
  energy: [1, 2, -2, 0, 0, 0, 0],
  power: [1, 2, -3, 0, 0, 0, 0],
  pressure: [1, -1, -2, 0, 0, 0, 0],
  area: [0, 2, 0, 0, 0, 0, 0],
  volume: [0, 3, 0, 0, 0, 0, 0],
  frequency: [0, 0, -1, 0, 0, 0, 0],
  charge: [0, 0, 1, 1, 0, 0, 0],
  voltage: [1, 2, -3, -1, 0, 0, 0],
  resistance: [1, 2, -3, -2, 0, 0, 0],
  capacitance: [-1, -2, 4, 2, 0, 0, 0],
  inductance: [1, 2, -2, -2, 0, 0, 0],
  dimensionless: [0, 0, 0, 0, 0, 0, 0],
  digital: [0, 0, 0, 0, 0, 0, 1],
  datarate: [0, 0, -1, 0, 0, 0, 1],
}

/**
 * Sig figs (Infinity = exact) and relative ± uncertainty; absent means exact.
 * NaN once an addition mixes measured parts, since that would need decimal places.
 * `digits` marks a ± still exactly as typed: naming its unit keeps it, any arithmetic drops it.
 */
type Prec = { sig: number; rel: number; digits?: string }
const LOST: Prec = { sig: Number.NaN, rel: Number.NaN }

/** `bare` is a unit on its own (`m`, `s^2`), with no number. */
type Qty = { si: number; dim: number[]; prefer?: Unit; prec?: Prec; bare?: boolean }

/** Products, quotients and powers: fewest sig figs, relative uncertainties add. */
function precMul(a: Qty, b: Qty): Prec | undefined {
  if (!a.prec && !b.prec) return undefined
  const x = a.prec ?? { sig: Infinity, rel: 0 }
  const y = b.prec ?? { sig: Infinity, rel: 0 }
  const digits = b.bare ? x.digits : a.bare ? y.digits : undefined
  return { sig: Math.min(x.sig, y.sig), rel: x.rel + y.rel, ...(digits && { digits }) }
}

function vec(d: Dim): number[] {
  return [...DIM_VEC[d]]
}

function addVec(a: number[], b: number[]): number[] {
  return a.map((x, i) => x + (b[i] ?? 0))
}

function subVec(a: number[], b: number[]): number[] {
  return a.map((x, i) => x - (b[i] ?? 0))
}

function scaleVec(a: number[], k: number): number[] {
  return a.map((x) => x * k)
}

function vecEq(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i])
}

function isZeroVec(a: readonly number[]): boolean {
  return a.every((x) => x === 0)
}

function isFreq(d: readonly number[]): boolean {
  return vecEq(d, DIM_VEC.frequency)
}

function isTime(d: readonly number[]): boolean {
  return vecEq(d, DIM_VEC.time)
}

function isAngVel(d: readonly number[]): boolean {
  return vecEq(d, subVec(vec('angle'), vec('time')))
}

function isEnergy(d: readonly number[]): boolean {
  return vecEq(d, DIM_VEC.energy)
}

function isPower(d: readonly number[]): boolean {
  return vecEq(d, DIM_VEC.power)
}

function unitQty(unit: Unit): Qty {
  return { si: siOf(unit), dim: vec(unit.dim), prefer: unit, bare: true }
}

/** An rpm rate counts revolutions (2π rad each); a bare `50/hr` or Hz is just a rate. */
function isRevs(q: Qty): boolean {
  return isFreq(q.dim) && q.prefer?.id === 'rpm'
}

function mulQty(a: Qty, b: Qty): Qty {
  let si = a.si * b.si
  const dim = addVec(a.dim, b.dim)
  const prec = precMul(a, b)
  if ((isEnergy(a.dim) && isRevs(b)) || (isRevs(a) && isEnergy(b.dim))) si *= 2 * Math.PI
  if ((isRevs(a) && isTime(b.dim)) || (isTime(a.dim) && isRevs(b))) {
    return { si: si * 2 * Math.PI, dim: vec('angle'), prefer: BY_ID.get('rad'), prec }
  }
  const prefer = isZeroVec(a.dim) ? b.prefer : isZeroVec(b.dim) ? a.prefer : undefined
  return { si, dim, prefer, prec }
}

function divQty(a: Qty, b: Qty): Qty | null {
  if (b.si === 0) return null
  let si = a.si / b.si
  const dim = subVec(a.dim, b.dim)
  if (isPower(a.dim) && isRevs(b) && isEnergy(dim)) si /= 2 * Math.PI
  const prefer = isZeroVec(b.dim) ? a.prefer : isZeroVec(a.dim) ? b.prefer : undefined
  return { si, dim, prefer, prec: precMul(a, b) }
}

function addQty(a: Qty, b: Qty, sign: 1 | -1): Qty | null {
  if (!vecEq(a.dim, b.dim)) return null
  const prec = a.prec || b.prec ? LOST : undefined
  return { si: a.si + sign * b.si, dim: a.dim, prefer: a.prefer ?? b.prefer, prec }
}

/** Exponents are exact; an uncertain one is not modelled. */
function powQty(a: Qty, exp: Qty): Qty | null {
  const e = exp.si
  if (!Number.isFinite(e) || !isZeroVec(exp.dim)) return null
  if (a.si < 0 && !Number.isInteger(e)) return null
  const prec = exp.prec?.rel ? LOST : a.prec && { sig: a.prec.sig, rel: a.prec.rel * Math.abs(e) }
  const bare = a.bare && !exp.prec
  return { si: a.si ** e, dim: scaleVec(a.dim, e), prefer: Math.abs(e) === 1 ? a.prefer : undefined, prec, ...(bare && { bare }) }
}

/** Sig figs and ± of a unit answer `n`, in the answer's own unit. */
function precMeas(n: number, p: Prec | undefined): Meas | undefined {
  if (!p || Number.isNaN(p.sig) || !Number.isFinite(n)) return undefined
  const out: Meas = {}
  if (Number.isFinite(p.sig)) {
    out.sig = p.sig
    out.dp = n === 0 ? p.sig - 1 : p.sig - 1 - Math.floor(Math.log10(Math.abs(n)))
  }
  if (p.rel > 0) {
    out.unc = Math.abs(n) * p.rel
    if (p.digits) out.uncDigits = p.digits
  }
  return out.sig == null && out.unc == null ? undefined : out
}

function convertQty(q: Qty, target: Qty): number | null {
  if (vecEq(q.dim, target.dim)) return target.si === 0 ? null : q.si / target.si
  if (isFreq(q.dim) && isAngVel(target.dim)) return q.si * (2 * Math.PI) / target.si
  if (isAngVel(q.dim) && isFreq(target.dim)) return q.si / (2 * Math.PI) / target.si
  const qNoAngle = q.dim.map((x, i) => (i === 5 ? 0 : x))
  const tNoAngle = target.dim.map((x, i) => (i === 5 ? 0 : x))
  if (vecEq(qNoAngle, tNoAngle)) return target.si === 0 ? null : q.si / target.si
  return null
}

const SI_UNIT_IDS = new Set([
  'kg',
  'm',
  'm2',
  'm3',
  's',
  'amp',
  'rad',
  'n',
  'j',
  'w',
  'pa',
  'coulomb',
  'volt',
  'ohm',
  'farad',
  'henry',
  'hz',
  'mps',
  'mps2',
])

function namedUnitFor(dim: number[]): Unit | undefined {
  const matches = UNIT_LIST.filter((u) => vecEq(vec(u.dim), dim))
  return (
    matches.find((u) => SI_UNIT_IDS.has(u.id) && sameScale(siOf(u), 1)) ??
    matches.find((u) => sameScale(siOf(u), 1)) ??
    matches.find((u) => sameScale(u.toBase, 1)) ??
    matches[0]
  )
}

function formatCompound(dim: number[]): string {
  const names = ['kg', 'm', 's', 'A', 'K', 'rad', 'B']
  const num: string[] = []
  const den: string[] = []
  dim.forEach((e, i) => {
    if (!e) return
    const s = Math.abs(e) === 1 ? names[i]! : `${names[i]}^${Math.abs(e)}`
    if (e > 0) num.push(s)
    else den.push(s)
  })
  if (!num.length && !den.length) return ''
  if (!den.length) return num.join(' ')
  const left = num.length ? num.join(' ') : '1'
  return `${left} / ${den.join(' ')}`
}

/** `1/Ω` rather than a slash-containing compound like `1/m/s`. */
function reciprocalLabel(symbol: string): string | null {
  const t = symbol.trim()
  if (!t || /[\s/^]/.test(t) || t.startsWith('1')) return null
  return `1/${t}`
}

function asReciprocal(q: Qty, unit: Unit): Value | null {
  if (!vecEq(scaleVec(vec(unit.dim), -1), q.dim)) return null
  const label = reciprocalLabel(unit.symbol)
  if (!label) return null
  const n = q.si * siOf(unit)
  if (!Number.isFinite(n)) return null
  return { ...num(n), unit: label }
}

function qtyToValue(q: Qty, target?: Qty, targetLabel?: string): Value | null {
  const v = qtyValue(q, target, targetLabel)
  const meas = v?.kind === 'number' ? precMeas(v.n, q.prec) : undefined
  return meas ? { ...v!, meas } : v
}

function qtyValue(q: Qty, target?: Qty, targetLabel?: string): Value | null {
  if (target) {
    if (!Number.isFinite(q.si)) return unitError()
    const n = convertQty(q, target)
    if (n == null || !Number.isFinite(n)) return unitError()
    const unit = targetLabel || target.prefer?.symbol || formatCompound(target.dim)
    const { prefer } = target
    const unitId = prefer && vecEq(vec(prefer.dim), target.dim) && sameScale(target.si, siOf(prefer)) ? prefer.id : undefined
    return unit ? { ...num(n), unit, unitId } : num(n)
  }
  if (!Number.isFinite(q.si)) return null
  if (isZeroVec(q.dim)) return num(q.si)
  if (q.prefer && vecEq(vec(q.prefer.dim), q.dim)) {
    const n = q.si / siOf(q.prefer)
    if (!Number.isFinite(n)) return null
    return { ...num(n), unit: q.prefer.symbol, unitId: q.prefer.id }
  }
  const named = namedUnitFor(q.dim)
  if (named) {
    const n = q.si / siOf(named)
    if (!Number.isFinite(n)) return null
    return { ...num(n), unit: named.symbol, unitId: named.id }
  }
  if (q.prefer) {
    const rec = asReciprocal(q, q.prefer)
    if (rec) return rec
  }
  const inv = namedUnitFor(scaleVec(q.dim, -1))
  if (inv) {
    const rec = asReciprocal(q, inv)
    if (rec) return rec
  }
  const unit = formatCompound(q.dim)
  return unit ? { ...num(q.si), unit } : num(q.si)
}

class UnitParser {
  s: string
  i = 0
  usedUnit = false
  incompatible = false

  constructor(s: string) {
    this.s = s
  }

  skip(): void {
    while (this.i < this.s.length && this.s[this.i] === ' ') this.i++
  }

  peek(): string {
    this.skip()
    return this.s[this.i] ?? ''
  }

  eat(ch: string): boolean {
    this.skip()
    if (this.s[this.i] !== ch) return false
    this.i++
    return true
  }

  parse(): Qty | null {
    const q = this.parseAdd()
    this.skip()
    if (this.i !== this.s.length || !q || !this.usedUnit) return null
    return q
  }

  parseAdd(): Qty | null {
    let left = this.parseMul()
    if (!left) return null
    for (;;) {
      this.skip()
      const sign = this.s[this.i] === '+' ? 1 : this.s[this.i] === '-' ? -1 : 0
      if (!sign) break
      this.i++
      const right = this.parseMul()
      if (!right) return null
      const next = addQty(left, right, sign)
      if (!next) {
        if (!isZeroVec(left.dim) && !isZeroVec(right.dim)) this.incompatible = true
        return null
      }
      left = next
    }
    return left
  }

  parseMul(): Qty | null {
    const first = this.parsePow()
    if (!first) return null
    let left: Qty = first
    for (;;) {
      this.skip()
      const ch = this.s[this.i]
      if (ch === '*' || ch === '/') {
        this.i++
        const right = this.parsePow()
        if (!right) return null
        if (ch === '*') {
          left = mulQty(left, right)
          continue
        }
        const quot = divQty(left, right)
        if (!quot) return null
        left = quot
        continue
      }
      // `5 (2 m)` multiplies, but after a unit `(` is a function call (`sec(0)`)
      if (isLetter(ch) || (ch === '(' && isZeroVec(left.dim))) {
        const right = this.parsePow()
        if (!right) return null
        left = mulQty(left, right)
        continue
      }
      // `5 ft 10 in`: a juxtaposed quantity of the same dimension adds
      if (/[\d.]/.test(ch ?? '') && !isZeroVec(left.dim)) {
        const right = this.parsePow()
        if (!right || !vecEq(left.dim, right.dim)) return null
        left = addQty(left, right, 1)!
        continue
      }
      break
    }
    return left
  }

  parsePow(): Qty | null {
    const base = this.parsePrimary()
    if (!base) return null
    this.skip()
    if (this.s[this.i] !== '^') return base
    this.i++
    const exp = this.parsePrimary()
    return exp && powQty(base, exp)
  }

  parsePrimary(): Qty | null {
    this.skip()
    if (this.eat('(')) {
      const inner = this.parseAdd()
      if (!inner || !this.eat(')')) return null
      return inner
    }
    if (this.s.slice(this.i, this.i + 2).toLowerCase() === 'pi' && !/[A-Za-z0-9]/.test(this.s[this.i + 2] ?? '')) {
      this.i += 2
      const unit = this.parseUnitRef()
      if (unit) return mulQty({ si: Math.PI, dim: vec('dimensionless') }, unit)
      return { si: Math.PI, dim: vec('dimensionless') }
    }
    const amount = this.parseAmount()
    if (amount) {
      const unit = this.parseUnitRef()
      if (unit) {
        const raised = this.parseUnitPower(unit)
        return raised && this.parsePerUnits(mulQty(amount, raised))
      }
      return amount
    }
    return this.parseUnitRef()
  }

  parseUnitPower(unit: Qty): Qty | null {
    this.skip()
    if (this.s[this.i] !== '^') return unit
    this.i++
    const exp = this.parsePrimary()
    return exp && powQty(unit, exp)
  }

  /** `4 MB/s` is one rate, so `32 GB / 4 MB/s` divides by the rate, not by 4 MB and then by s. */
  parsePerUnits(q: Qty): Qty | null {
    let out: Qty | null = q
    while (out && this.peek() === '/') {
      const at = this.i
      this.i++
      const unit = this.parseUnitRef()
      if (!unit) {
        this.i = at
        break
      }
      const den = this.parseUnitPower(unit)
      out = den && divQty(out, den)
    }
    return out
  }

  /**
   * A number as written: `2.50` carries 3 sig figs, `5.0 ± 0.1` / `10 ± 5%` an uncertainty
   * (± binds tighter than any operator), and a whole-number fraction right before a unit is one amount
   * (`3/8 in`; `1/6.8 ohm` and `1/1 s` stay reciprocals).
   */
  parseAmount(): Qty | null {
    const value = this.parseNumber()
    if (!value) return null
    let q = value
    if (this.eat('±')) {
      const start = this.i
      const u = this.parseNumber()
      if (!u || value.si === 0) return null
      const pct = this.eat('%')
      const unc = Math.abs(pct ? (value.si * u.si) / 100 : u.si)
      const digits = pct ? undefined : typedDigits(this.s.slice(start, this.i).replace(/[\s+-]/g, ''))
      q = { ...value, prec: { sig: value.prec?.sig ?? Infinity, rel: unc / Math.abs(value.si), digits } }
    }
    const at = this.i
    if (this.eat('/')) {
      const den = this.parseNumber()
      this.skip()
      const vulgar = Number.isInteger(q.si) && !q.prec && den && Number.isInteger(den.si) && den.si > 1 && !den.prec
      if (vulgar && matchUnitAtStart(this.s.slice(this.i))) return divQty(q, den)
      this.i = at
    }
    return q
  }

  parseNumber(): Qty | null {
    this.skip()
    const m = this.s.slice(this.i).match(/^([+-]?)((?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)/i)
    if (!m) return null
    const si = Number(m[0])
    if (!Number.isFinite(si)) return null
    this.i += m[0].length
    const { sig } = literalMeas(m[2]!)
    return { si, dim: vec('dimensionless'), prec: Number.isFinite(sig) ? { sig, rel: 0 } : undefined }
  }

  parseUnitRef(): Qty | null {
    this.skip()
    const hit = matchUnitAtStart(this.s.slice(this.i))
    // °C and °F are offset scales: `72 F in C` converts, but they can't be multiplied or added
    if (!hit || hit.unit.id === 'c' || hit.unit.id === 'f') return null
    this.i += this.s.slice(this.i).length - hit.rest.length
    this.usedUnit = true
    return unitQty(hit.unit)
  }
}

function splitConvert(src: string): { left: string; right?: string } {
  const m = src.match(/^(.*)\s+(to|into)\s+(.+)$/i)
  if (m) return { left: (m[1] ?? '').trim(), right: (m[3] ?? '').trim() }
  return { left: src }
}

function splitInConvert(src: string): { left: string; right: string } | null {
  const m = src.match(/^(.*)\s+in\s+(.+)$/i)
  if (!m) return null
  const left = (m[1] ?? '').trim()
  const right = (m[2] ?? '').trim()
  if (!left || !right) return null
  return { left, right }
}

function targetFor(from: Unit, defaults?: DefaultUnits): Unit | undefined {
  const chosen = defaults?.[from.dim]
  if (chosen) {
    const to = BY_ID.get(chosen)
    if (to && to.dim === from.dim) return to
  }
  return from.defaultTo ? BY_ID.get(from.defaultTo) : undefined
}

function applyDefaultUnit(q: Qty, defaults?: DefaultUnits): Value | null {
  if (defaults) {
    const named = namedUnitFor(q.dim)
    if (named) {
      const id = defaults[named.dim]
      const to = id ? BY_ID.get(id) : undefined
      if (to && vecEq(vec(to.dim), q.dim)) return qtyToValue(q, unitQty(to), to.symbol)
    }
  }
  return qtyToValue(q)
}

function evalUnitSides(left: string, right: string | undefined, defaults?: DefaultUnits): Value | null {
  const leftParser = new UnitParser(left)
  const leftQ = leftParser.parse()
  if (!right) {
    if (!leftQ) return leftParser.incompatible ? unitError() : null
    return applyDefaultUnit(leftQ, defaults)
  }
  const rightQ = new UnitParser(right).parse()
  if (!leftQ) {
    if (leftParser.incompatible || (rightQ && leftParser.usedUnit)) return unitError()
    return null
  }
  if (!rightQ) return null
  return qtyToValue(leftQ, rightQ, right.replace(/\s+/g, ' '))
}

function tryUnitExpression(src: string, defaults?: DefaultUnits): Value | null {
  const { left, right } = splitConvert(src)
  if (right) return evalUnitSides(left, right, defaults)

  // `in` is also inches, so it only converts when both sides parse and the dimensions fit (`3V/39ohm in mA`)
  const viaIn = splitInConvert(src)
  if (viaIn) {
    const leftParser = new UnitParser(viaIn.left)
    const leftQ = leftParser.parse()
    const rightQ = new UnitParser(viaIn.right).parse()
    if (leftQ && rightQ) {
      if (convertQty(leftQ, rightQ) != null) {
        return qtyToValue(leftQ, rightQ, viaIn.right.replace(/\s+/g, ' '))
      }
      return unitError()
    }
  }

  return evalUnitSides(src, undefined, defaults)
}

function trySimpleConvert(src: string, defaults?: DefaultUnits): Value | null {
  const end = matchPrefixedUnit(src)
  if (!end) return null

  const conv = end.rest.match(/^(.*)\s+(to|into|in)$/i)
  let from: Unit
  let to: Unit
  let expr: string
  if (conv) {
    const source = matchPrefixedUnit((conv[1] ?? '').trim())
    // a mismatch (`2 F in uF`) may still convert once the unit parser reads `F` as a farad
    if (!source || source.unit.dim !== end.unit.dim) return null
    from = source.unit
    to = end.unit
    expr = source.rest
  } else {
    from = end.unit
    const target = targetFor(from, defaults)
    if (!target) {
      const amount = parseAmount(end.rest)
      if (amount != null && from.dim === 'dimensionless') return num(amount * from.toBase)
      return null
    }
    to = target
    expr = end.rest
  }

  const amount = parseAmount(expr)
  if (amount == null) return null
  const out = convertAmount(amount, from, to)
  // conversion factors are exact, so `12.0 kg in lb` keeps 3 sig figs; offset scales don't
  const { sig } = literalMeas(expr.trim().replace(/^[+-]/, ''))
  const meas = from.dim !== 'temperature' && out?.kind === 'number' ? precMeas(out.n, Number.isFinite(sig) ? { sig, rel: 0 } : undefined) : undefined
  return meas ? { ...out!, meas } : out
}

export function tryConvert(text: string, defaults?: DefaultUnits): Value | null {
  const src = preprocess(text)
  // a lone quote mark is no quantity, and `5 kg in` is a conversion still being typed, not kg times inches
  if (!src || /^['"]+$/.test(src) || /(?<!\b(?:to|into|in))(?<=[A-Za-z])\s+in$/i.test(src)) return null
  const units = sanitizeDefaultUnits(defaults)
  return trySimpleConvert(src, units) ?? tryUnitExpression(src, units)
}

/** Units that step through SI prefixes with ⌥↑/⌥↓ (kg and t sit on the gram ladder). */
const PREFIX_ROOTS = ['m', 'g', 's', 'l', 'j', 'w', 'pa', 'hz', 'n', 'volt', 'amp', 'ohm', 'farad', 'henry', 'coulomb', 'ev']

function prefixExponent(p: Prefix): number {
  return Math.round(Math.log10(p.factor))
}

/** Table units by id, plus the `kilo_n`-style ids of prefixed units synthesized on the fly. */
function unitById(id: string | undefined): Unit | undefined {
  if (!id) return undefined
  const known = BY_ID.get(id)
  if (known) return known
  const cut = id.indexOf('_')
  const prefix = PREFIX_BY_NAME.get(id.slice(0, cut))
  const base = BY_ID.get(id.slice(cut + 1))
  return cut > 0 && prefix && base ? prefixedUnit(prefix, base, id) : undefined
}

/** Where a unit sits on a prefix ladder: `kN` → N at 10³, `cm` → m at 10⁻². */
function ladderPlace(unit: Unit): { root: Unit; exp: number } | null {
  for (const id of PREFIX_ROOTS) {
    const root = BY_ID.get(id)!
    if (root.dim !== unit.dim) continue
    const exp = Math.log10(unit.toBase / root.toBase)
    const k = Math.round(exp)
    if (Math.abs(exp - k) < 1e-9) return { root, exp: k }
  }
  return null
}

/** The unit at 10^exp on a ladder, preferring the table's own (kN, MPa, t). */
function ladderUnit(root: Unit, exp: number): Unit | null {
  if (exp === 0) return root
  const prefix = PREFIXES.find((p) => prefixExponent(p) === exp)
  if (!prefix) return null
  if (root.id === 'g' && exp === 6) return BY_ID.get('tonne')!
  const unit = prefixedUnit(prefix, root)
  return UNIT_LIST.find((u) => u.dim === unit.dim && u.symbol === unit.symbol && sameScale(u.toBase, unit.toBase)) ?? unit
}

function moveOnLadder(value: Value, from: { root: Unit; exp: number }, exp: number): Value | null {
  const to = ladderUnit(from.root, exp)
  if (!to) return null
  const shift = from.exp - exp
  const scale = 10 ** Math.abs(shift)
  const n = shift >= 0 ? value.n * scale : value.n / scale
  return { ...num(n), unit: to.symbol, unitId: to.id }
}

/**
 * One engineering step (×10³) up or down the SI prefixes: `98 N` → `0.098 kN`. Null for plain numbers,
 * units without prefixes (°C, in, hr) and the ends of the ladder (y…Y).
 */
export function stepPrefix(value: Value, dir: 1 | -1): Value | null {
  if (value.kind !== 'number' || !Number.isFinite(value.n)) return null
  const unit = unitById(value.unitId)
  const place = unit && ladderPlace(unit)
  if (!place) return null
  const exp = dir > 0 ? Math.ceil((place.exp + 1) / 3) * 3 : Math.floor((place.exp - 1) / 3) * 3
  if (Math.abs(exp) > 24) return null
  return moveOnLadder(value, place, exp)
}

/** Re-express a value in a unit from the same prefix ladder (keeps a ⌥↑ choice while typing). */
export function inLadderUnit(value: Value, unitId: string): Value | null {
  if (value.kind !== 'number' || !Number.isFinite(value.n)) return null
  const fromUnit = unitById(value.unitId)
  const toUnit = unitById(unitId)
  const from = fromUnit && ladderPlace(fromUnit)
  const to = toUnit && ladderPlace(toUnit)
  if (!from || !to || from.root !== to.root) return null
  return moveOnLadder(value, from, to.exp)
}

export type UnitChoice = { id: string; label: string }

export type UnitSettingItem = {
  dim: Dim
  label: string
  units: UnitChoice[]
}

export type UnitSettingGroup = {
  id: string
  title: string
  items: UnitSettingItem[]
}

/** Curated defaults shown in Settings. Automatic (empty) keeps SI ↔ US counterparts. */
export const UNIT_SETTING_GROUPS: UnitSettingGroup[] = [
  {
    id: 'length',
    title: 'Length',
    items: [
      {
        dim: 'length',
        label: 'Length',
        units: [
          { id: 'in', label: 'Inch' },
          { id: 'ft', label: 'Foot' },
          { id: 'yd', label: 'Yard' },
          { id: 'mi', label: 'Mile' },
          { id: 'mm', label: 'Millimeter' },
          { id: 'cm', label: 'Centimeter' },
          { id: 'm', label: 'Meter' },
          { id: 'km', label: 'Kilometer' },
        ],
      },
      {
        dim: 'area',
        label: 'Area',
        units: [
          { id: 'in2', label: 'Square inch' },
          { id: 'sqft', label: 'Square foot' },
          { id: 'yd2', label: 'Square yard' },
          { id: 'cm2', label: 'Square centimeter' },
          { id: 'm2', label: 'Square meter' },
          { id: 'km2', label: 'Square kilometer' },
          { id: 'acre', label: 'Acre' },
          { id: 'ha', label: 'Hectare' },
        ],
      },
      {
        dim: 'volume',
        label: 'Volume',
        units: [
          { id: 'in3', label: 'Cubic inch' },
          { id: 'ft3', label: 'Cubic foot' },
          { id: 'usgal', label: 'US gallon' },
          { id: 'floz', label: 'US fluid ounce' },
          { id: 'ml', label: 'Milliliter' },
          { id: 'l', label: 'Liter' },
          { id: 'm3', label: 'Cubic meter' },
        ],
      },
      {
        dim: 'speed',
        label: 'Speed',
        units: [
          { id: 'mph', label: 'Mile per hour' },
          { id: 'fps', label: 'Foot per second' },
          { id: 'kmh', label: 'Kilometer per hour' },
          { id: 'mps', label: 'Meter per second' },
          { id: 'knot', label: 'Knot' },
        ],
      },
      {
        dim: 'acceleration',
        label: 'Linear acceleration',
        units: [
          { id: 'mps2', label: 'Meter per second squared' },
          { id: 'gee', label: 'g (standard gravity)' },
        ],
      },
    ],
  },
  {
    id: 'angle',
    title: 'Angle',
    items: [
      {
        dim: 'angle',
        label: 'Angle',
        units: [
          { id: 'deg', label: 'Degree' },
          { id: 'rad', label: 'Radian' },
          { id: 'rev', label: 'Revolution' },
        ],
      },
    ],
  },
  {
    id: 'mechanical',
    title: 'Mechanical',
    items: [
      {
        dim: 'mass',
        label: 'Mass',
        units: [
          { id: 'oz', label: 'Ounce' },
          { id: 'lb', label: 'Pound' },
          { id: 'g', label: 'Gram' },
          { id: 'kg', label: 'Kilogram' },
          { id: 'ton', label: 'US ton' },
          { id: 'tonne', label: 'Metric ton' },
        ],
      },
      {
        dim: 'force',
        label: 'Force',
        units: [
          { id: 'ozf', label: 'Ounce-force' },
          { id: 'lbf', label: 'Pound-force' },
          { id: 'n', label: 'Newton' },
          { id: 'kilonewton', label: 'Kilonewton' },
          { id: 'kgf', label: 'Kilogram-force' },
        ],
      },
      {
        dim: 'pressure',
        label: 'Pressure',
        units: [
          { id: 'psi', label: 'Pound per square inch' },
          { id: 'ksi', label: 'Kilopound per square inch' },
          { id: 'kpa', label: 'Kilopascal' },
          { id: 'mpa', label: 'Megapascal' },
          { id: 'bar', label: 'Bar' },
          { id: 'atm', label: 'Atmosphere' },
        ],
      },
      {
        dim: 'energy',
        label: 'Energy',
        units: [
          { id: 'j', label: 'Joule' },
          { id: 'kj', label: 'Kilojoule' },
          { id: 'cal', label: 'Calorie' },
          { id: 'kcal', label: 'Kilocalorie' },
          { id: 'btu', label: 'British thermal unit' },
          { id: 'kwh', label: 'Kilowatt-hour' },
        ],
      },
      {
        dim: 'power',
        label: 'Power',
        units: [
          { id: 'w', label: 'Watt' },
          { id: 'kw', label: 'Kilowatt' },
          { id: 'hp', label: 'Horsepower' },
        ],
      },
    ],
  },
  {
    id: 'temperature',
    title: 'Temperature',
    items: [
      {
        dim: 'temperature',
        label: 'Temperature',
        units: [
          { id: 'c', label: 'Celsius' },
          { id: 'f', label: 'Fahrenheit' },
          { id: 'k', label: 'Kelvin' },
          { id: 'r', label: 'Rankine' },
        ],
      },
    ],
  },
  {
    id: 'time',
    title: 'Time',
    items: [
      {
        dim: 'time',
        label: 'Time',
        units: [
          { id: 's', label: 'Second' },
          { id: 'min', label: 'Minute' },
          { id: 'hr', label: 'Hour' },
          { id: 'day', label: 'Day' },
        ],
      },
      {
        dim: 'frequency',
        label: 'Frequency',
        units: [
          { id: 'hz', label: 'Hertz' },
          { id: 'rpm', label: 'Revolution per minute' },
        ],
      },
    ],
  },
]

const SETTING_DIMS = new Set<Dim>(UNIT_SETTING_GROUPS.flatMap((g) => g.items.map((item) => item.dim)))

export function sanitizeDefaultUnits(raw: unknown): DefaultUnits {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return {}
  const out: DefaultUnits = {}
  for (const [dim, id] of Object.entries(raw as Record<string, unknown>)) {
    if (!SETTING_DIMS.has(dim as Dim)) continue
    if (typeof id !== 'string' || !id) continue
    const unit = BY_ID.get(id)
    if (!unit || unit.dim !== dim) continue
    out[dim as Dim] = id
  }
  return out
}

export function defaultUnitsEqual(a: DefaultUnits, b: DefaultUnits): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)])
  for (const key of keys) {
    if (a[key as Dim] !== b[key as Dim]) return false
  }
  return true
}

/** `5 cm` (or `5.0 ± 0.1 cm`) as text that parses back to the same quantity, or null when the unit label doesn't. */
export function quantityText(v: Value): string | null {
  if (v.kind !== 'number' || !v.unit || !v.unitId || !Number.isFinite(v.n)) return null
  const n = Number(v.n.toPrecision(12))
  const text = `${n}${v.meas?.unc ? ` ± ${v.meas.unc}` : ''} ${v.unit}`
  const back = tryConvert(`${text} to ${v.unit}`)
  return back?.kind === 'number' && back.unitId === v.unitId && sameScale(back.n, n) ? text : null
}

/** A few other units worth tabbing to, picked from the Settings lists by how readable the number is. */
export function unitAlternatives(value: Value, limit = 3): Value[] {
  if (value.kind !== 'number' || !Number.isFinite(value.n)) return []
  const from = unitById(value.unitId)
  if (!from || (value.n === 0 && from.dim !== 'temperature')) return []
  const choices = UNIT_SETTING_GROUPS.flatMap((g) => g.items).find((item) => item.dim === from.dim)?.units ?? []
  const out: Value[] = []
  for (const choice of choices) {
    const to = BY_ID.get(choice.id)
    if (!to || to.id === from.id || (to.symbol === from.symbol && sameScale(to.toBase, from.toBase))) continue
    const v = convertAmount(value.n, from, to)
    if (v?.kind === 'number' && Number.isFinite(v.n)) out.push(v)
  }
  // 1 to 999 reads best; tiny numbers read worse than big ones
  const readability = (n: number) => {
    const mag = Math.log10(Math.abs(n))
    return mag < 0 ? -mag * 2 : Math.max(0, mag - 3)
  }
  const picked = new Set([...out].sort((a, b) => readability(a.n) - readability(b.n)).slice(0, limit))
  return out.filter((v) => picked.has(v))
}
