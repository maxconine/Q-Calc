import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'
import { sanitizeDefaultUnits, UNIT_SETTING_GROUPS } from './units'

type Case = {
  name: string
  input: string
  expected: number
  eps?: number
  unit?: string | RegExp
}

function run(c: Case): void {
  const r = evaluateLine(c.input)
  const label = `${c.input} → ${r.display}`
  const actual = r.value?.n
  expect(actual, label).toEqual(expect.any(Number))
  const eps = c.eps ?? 1e-8
  expect(Math.abs(actual! - c.expected), `${label} (expected ${c.expected})`).toBeLessThan(eps)
  if (c.unit !== undefined) {
    if (typeof c.unit === 'string') expect(r.display, label).toMatch(new RegExp(`${c.unit}$`))
    else expect(r.display, label).toMatch(c.unit)
  }
}

function suite(title: string, cases: Case[]) {
  describe(title, () => {
    it.each(cases)('$name', (c) => run(c))
  })
}

suite('Length & Distance', [
  { name: 'Inches to Centimeters', input: '10 inches to centimeters', expected: 10 * 2.54, unit: 'cm' },
  { name: 'Centimeters to Inches', input: '50 centimeters to inches', expected: 50 / 2.54, unit: 'in' },
  { name: 'Feet to Meters', input: '6 feet to meters', expected: 6 * 0.3048, unit: 'm' },
  { name: 'Meters to Feet', input: '10 meters to feet', expected: 10 / 0.3048, unit: 'ft' },
  { name: 'Yards to Meters', input: '100 yards to meters', expected: 100 * 0.9144, unit: 'm' },
  { name: 'Miles to Kilometers', input: '5 miles to kilometers', expected: 5 * 1.609344, unit: 'km' },
  { name: 'Kilometers to Miles', input: '10 kilometers to miles', expected: 10 / 1.609344, unit: 'mi' },
  { name: 'Nautical Miles to Kilometers', input: '10 nautical miles to kilometers', expected: 10 * 1.852, unit: 'km' },
  { name: 'Millimeters to Inches', input: '25 millimeters to inches', expected: 25 / 25.4, unit: 'in' },
  { name: 'Kilometers to Meters', input: '3.5 kilometers to meters', expected: 3.5 * 1000, unit: 'm' },
])

suite('Mass & Weight', [
  { name: 'Pounds to Kilograms', input: '150 pounds to kilograms', expected: 150 * 0.45359237, unit: 'kg' },
  { name: 'Kilograms to Pounds', input: '70 kilograms to pounds', expected: 70 / 0.45359237, unit: 'lbs' },
  { name: 'Ounces to Grams', input: '16 ounces to grams', expected: 16 * 28.349523125, unit: 'g' },
  { name: 'Grams to Ounces', input: '500 grams to ounces', expected: 500 / 28.349523125, unit: 'oz' },
  { name: 'Metric Tons to Kilograms', input: '2.5 metric tons to kilograms', expected: 2.5 * 1000, unit: 'kg' },
  { name: 'Stones to Kilograms', input: '10 stones to kilograms', expected: 10 * 6.35029318, unit: 'kg' },
  { name: 'Milligrams to Grams', input: '2500 milligrams to grams', expected: 2500 / 1000, unit: 'g' },
  { name: 'US Short Tons to Pounds', input: '3 US short tons to pounds', expected: 3 * 2000, unit: 'lbs' },
])

suite('Temperature', [
  { name: 'Celsius to Fahrenheit', input: '25 celsius to fahrenheit', expected: (25 * 9) / 5 + 32, unit: '°F' },
  { name: 'Fahrenheit to Celsius', input: '98.6 fahrenheit to celsius', expected: ((98.6 - 32) * 5) / 9, unit: '°C' },
  { name: 'Celsius to Kelvin', input: '100 celsius to kelvin', expected: 100 + 273.15, unit: 'K' },
  { name: 'Kelvin to Celsius', input: '300 kelvin to celsius', expected: 300 - 273.15, unit: '°C' },
  { name: 'Fahrenheit to Kelvin', input: '32 fahrenheit to kelvin', expected: ((32 - 32) * 5) / 9 + 273.15, unit: 'K' },
  { name: 'Kelvin to Fahrenheit', input: '0 kelvin to fahrenheit', expected: ((0 - 273.15) * 9) / 5 + 32, unit: '°F' },
])

suite('Volume & Capacity', [
  { name: 'US Gallons to Liters', input: '10 US gallons to liters', expected: 10 * 3.785411784, unit: 'L' },
  { name: 'Liters to US Gallons', input: '20 liters to US gallons', expected: 20 / 3.785411784, unit: 'gal' },
  { name: 'US Fluid Ounces to Milliliters', input: '8 US fluid ounces to milliliters', expected: 8 * 29.5735295625, unit: 'mL' },
  { name: 'Milliliters to US Fluid Ounces', input: '500 milliliters to US fluid ounces', expected: 500 / 29.5735295625, unit: 'fl oz' },
  { name: 'US Cups to Milliliters', input: '2 US cups to milliliters', expected: 2 * 236.5882365, unit: 'mL' },
  { name: 'Imperial Gallons to Liters', input: '5 imperial gallons to liters', expected: 5 * 4.54609, unit: 'L' },
  { name: 'Cubic Meters to Liters', input: '1.5 cubic meters to liters', expected: 1.5 * 1000, unit: 'L' },
  { name: 'US Quarts to Liters', input: '4 US quarts to liters', expected: 4 * 0.946352946, unit: 'L' },
])

suite('Area', [
  { name: 'Square Feet to Square Meters', input: '100 square feet to square meters', expected: 100 * 0.09290304, unit: 'm²' },
  { name: 'Square Meters to Square Feet', input: '50 square meters to square feet', expected: 50 / 0.09290304, unit: 'ft²' },
  { name: 'Acres to Square Meters', input: '1 acre to square meters', expected: 1 * 4046.8564224, unit: 'm²' },
  { name: 'Hectares to Acres', input: '5 hectares to acres', expected: 5 * (10000 / 4046.8564224), unit: 'acres' },
  { name: 'Square Miles to Square Kilometers', input: '10 square miles to square kilometers', expected: 10 * 1.609344 ** 2, unit: 'km²' },
])

suite('Speed & Velocity', [
  { name: 'Miles per Hour to Kilometers per Hour', input: '60 miles per hour to kilometers per hour', expected: 60 * 1.609344, unit: 'km/h' },
  { name: 'Kilometers per Hour to Meters per Second', input: '100 kilometers per hour to meters per second', expected: 100 / 3.6, unit: 'm/s' },
  { name: 'Meters per Second to Miles per Hour', input: '10 meters per second to miles per hour', expected: 10 * (3600 / 1609.344), unit: 'mph' },
  { name: 'Knots to Kilometers per Hour', input: '20 knots to kilometers per hour', expected: 20 * 1.852, unit: 'km/h' },
])

suite('Time', [
  { name: 'Hours to Seconds', input: '2.5 hours to seconds', expected: 2.5 * 3600, unit: 's' },
  { name: 'Days to Minutes', input: '7 days to minutes', expected: 7 * 1440, unit: 'min' },
  { name: 'Weeks to Hours', input: '3 weeks to hours', expected: 3 * 168, unit: 'hr' },
  { name: 'Years (Julian) to Seconds', input: '1 year to seconds', expected: 365.25 * 86400, unit: 's' },
])

suite('Digital Data Storage', [
  { name: 'Megabytes to Kilobytes (Binary)', input: '500 megabytes to kilobytes', expected: 500 * 1024, unit: 'KB' },
  { name: 'Gigabytes to Megabytes (Binary)', input: '16 gigabytes to megabytes', expected: 16 * 1024, unit: 'MB' },
  { name: 'Terabytes to Gigabytes (Binary)', input: '2 terabytes to gigabytes', expected: 2 * 1024, unit: 'GB' },
])

suite('Energy & Power', [
  { name: 'Thermodynamic Calories to Joules', input: '250 calories to joules', expected: 250 * 4.184, unit: 'J' },
  { name: 'Mechanical Horsepower to Watts', input: '100 horsepower to watts', expected: 100 * 745.699872, unit: 'W' },
])

describe('Unit conversion phrases', () => {
  it('accepts abbreviations and "in" as the converter', () => {
    const r = evaluateLine('10 in to cm')
    expect(Math.abs(r.value!.n - 25.4)).toBeLessThan(1e-8)
    expect(r.display).toBe('25.4 cm')
    const alt = evaluateLine('10 inches in cm')
    expect(Math.abs(alt.value!.n - 25.4)).toBeLessThan(1e-8)
  })

  it('does not convert mismatched dimensions', () => {
    const r = evaluateLine('10 meters to kilograms')
    expect(r.value?.kind).toBe('text')
    expect(r.display).toBe('improper unit conversion')
  })

  it('does not throw on improper unit conversions', () => {
    const inputs = [
      '10 meters to kilograms',
      '10 m to kg',
      '2 kg to meters',
      '2 kg to m',
      '2 kg + 3 m',
      '5 hours to meters',
      '2 in to kg',
      '100 watts to meters',
      '1 m to m^2',
      '1 joule to watt',
      '10^1000 m to nm',
    ]
    for (const input of inputs) {
      expect(() => evaluateLine(input), input).not.toThrow()
      const r = evaluateLine(input)
      expect(r.value?.kind, input).toBe('text')
      expect(r.display, input).toBe('improper unit conversion')
    }
  })
})

suite('Automatic SI ↔ US conversion', [
  { name: 'Inches default to millimeters', input: '2 in', expected: 50.8, unit: 'mm' },
  { name: 'Inches without a space', input: '2in', expected: 50.8, unit: 'mm' },
  { name: 'Inches to meters when asked', input: '2 in to m', expected: 0.0508, unit: 'm' },
  { name: 'Inches word default to millimeters', input: '2 inches', expected: 50.8, unit: 'mm' },
  { name: 'Millimeters default to inches', input: '25.4 mm', expected: 1, unit: 'in' },
  { name: 'Feet default to meters', input: '2 ft', expected: 2 * 0.3048, unit: 'm' },
  { name: 'Meters default to feet', input: '2 m', expected: 2 / 0.3048, unit: 'ft' },
  { name: 'Miles default to kilometers', input: '2 mi', expected: 2 * 1.609344, unit: 'km' },
  { name: 'Kilometers default to miles', input: '2 km', expected: 2 / 1.609344, unit: 'mi' },
  { name: 'Pounds default to kilograms', input: '2 lb', expected: 2 * 0.45359237, unit: 'kg' },
  { name: 'Kilograms default to pounds', input: '2 kg', expected: 2 / 0.45359237, unit: 'lbs' },
  { name: 'Ounces default to grams', input: '2 oz', expected: 2 * 28.349523125, unit: 'g' },
  { name: 'Grams default to ounces', input: '2 g', expected: 2 / 28.349523125, unit: 'oz' },
  { name: 'Celsius default to Fahrenheit', input: '0 c', expected: 32, unit: '°F' },
  { name: 'Fahrenheit default to Celsius', input: '32 f', expected: 0, unit: '°C' },
  { name: 'Gallons default to liters', input: '2 gal', expected: 2 * 3.785411784, unit: 'L' },
  { name: 'Liters default to gallons', input: '2 L', expected: 2 / 3.785411784, unit: 'gal' },
  { name: 'Fluid ounces default to milliliters', input: '2 fl oz', expected: 2 * 29.5735295625, unit: 'mL' },
  { name: 'Square feet default to square meters', input: '2 sq ft', expected: 2 * 0.09290304, unit: 'm²' },
  { name: 'Square meters default to square feet', input: '2 m^2', expected: 2 / 0.09290304, unit: 'ft²' },
  { name: 'Miles per hour default to km/h', input: '2 mph', expected: 2 * 1.609344, unit: 'km/h' },
  { name: 'km/h default to mph', input: '2 km/h', expected: 2 / 1.609344, unit: 'mph' },
  { name: 'PSI default to kilopascals', input: '2 psi', expected: (2 * 6894.757293168361) / 1000, unit: 'kPa' },
  { name: 'Newtons default to pound-force', input: '2 N', expected: 2 / 4.4482216152605, unit: 'lbf' },
  { name: 'Horsepower default to kilowatts', input: '2 hp', expected: (2 * 745.699872) / 1000, unit: 'kW' },
  { name: 'Degrees default to radians', input: '180 deg', expected: Math.PI, unit: 'rad' },
])

describe('Workspace default units', () => {
  it('keeps a quantity in the chosen default unit', () => {
    const r = evaluateLine('2 in', { defaultUnits: { length: 'in' } })
    expect(r.display).toBe('2 in')
    expect(r.value?.n).toBe(2)
  })

  it('converts other units into the chosen default', () => {
    const r = evaluateLine('25.4 mm', { defaultUnits: { length: 'in' } })
    expect(r.display).toMatch(/in$/)
    expect(r.value?.n).toBeCloseTo(1, 8)
  })

  it('still honors an explicit to-target', () => {
    const r = evaluateLine('2 in to mm', { defaultUnits: { length: 'in' } })
    expect(r.display).toBe('50.8 mm')
  })

  it('converts compound results into the default unit', () => {
    const r = evaluateLine('1 m + 50 cm', { defaultUnits: { length: 'in' } })
    expect(r.display).toMatch(/in$/)
    expect(r.value?.n).toBeCloseTo(1.5 / 0.0254, 6)
  })

  it('converts area products into the area default', () => {
    const r = evaluateLine('2 ft * 2 ft', { defaultUnits: { area: 'm2' } })
    expect(r.display).toMatch(/m²$/)
    expect(r.value?.n).toBeCloseTo(4 * 0.09290304, 8)
  })

  it('converts time when a default is set', () => {
    const r = evaluateLine('2 hr', { defaultUnits: { time: 's' } })
    expect(r.display).toMatch(/s$/)
    expect(r.value?.n).toBe(7200)
  })

  it('accepts only known unit ids for a dimension', () => {
    expect(sanitizeDefaultUnits({ length: 'in', mass: 'nope', bogus: 'kg' })).toEqual({ length: 'in' })
  })

  it('uses catalog ids that exist', () => {
    for (const group of UNIT_SETTING_GROUPS) {
      for (const item of group.items) {
        for (const unit of item.units) {
          expect(sanitizeDefaultUnits({ [item.dim]: unit.id })[item.dim], `${item.dim}:${unit.id}`).toBe(unit.id)
        }
      }
    }
  })
})

describe('Requested unit conversion examples', () => {
  it('converts 2 in to millimeters by default', () => {
    const r = evaluateLine('2 in')
    expect(r.display).toBe('50.8 mm')
    expect(r.value?.n).toBeCloseTo(50.8, 8)
  })

  it('converts 2 in to m when a target is given', () => {
    const r = evaluateLine('2 in to m')
    expect(r.display).toBe('0.0508 m')
    expect(r.value?.n).toBeCloseTo(0.0508, 8)
  })
})

const LB = 0.45359237
const FT = 0.3048
const G0 = 9.80665
const C = 299792458
const E = 1.602176634e-19
const AMU = 1.6605390666e-27
const AU = 149597870700
const YEAR = 365.25 * 86400
const SLUG = (G0 * LB) / FT
const US_GAL = 3.785411784

suite('Requested catalog aliases', [
  { name: 'electron rest mass to kg', input: '1 electronRestMass to kg', expected: 9.1093837015e-31, unit: 'kg', eps: 1e-40 },
  { name: 'proton rest mass to kg', input: '1 protonRestMass to kg', expected: 1.007276466621 * AMU, unit: 'kg', eps: 1e-36 },
  { name: 'neutron rest mass to kg', input: '1 neutronRestMass to kg', expected: 1.00866491588 * AMU, unit: 'kg', eps: 1e-36 },
  { name: 'AMU to kg', input: '1 AMU to kg', expected: AMU, unit: 'kg', eps: 1e-36 },
  { name: 'AtomicMassUnit to g', input: '1 AtomicMassUnit to g', expected: AMU * 1000, unit: 'g', eps: 1e-33 },
  { name: 'Ozm to g', input: '1 Ozm to g', expected: 28.349523125, unit: 'g' },
  { name: 'OunceMass to g', input: '1 OunceMass to g', expected: 28.349523125, unit: 'g' },
  { name: 'Lbm to kg', input: '1 Lbm to kg', expected: LB, unit: 'kg' },
  { name: 'PoundMass to kg', input: '1 PoundMass to kg', expected: LB, unit: 'kg' },
  { name: 'Slug to kg', input: '1 Slug to kg', expected: SLUG, unit: 'kg' },
  { name: 'Snail to slugs', input: '1 Snail to slugs', expected: 12, unit: 'slug' },
  { name: 'Slinch to kg', input: '1 Slinch to kg', expected: 12 * SLUG, unit: 'kg' },
  { name: 'ShortTon to lbs', input: '1 ShortTon to lbs', expected: 2000, unit: 'lbs' },
  { name: 'LongTon to lbs', input: '1 LongTon to lbs', expected: 2240, unit: 'lbs' },
  { name: 'Fermi to m', input: '1 Fermi to m', expected: 1e-15, unit: 'm', eps: 1e-24 },
  { name: 'angstrom to m', input: '1 angstrom to m', expected: 1e-10, unit: 'm', eps: 1e-20 },
  { name: 'astronomicalUnit to m', input: '1 astronomicalUnit to m', expected: AU, unit: 'm', eps: 1e-3 },
  { name: 'parsec to au', input: '1 parsec to au', expected: 648000 / Math.PI, unit: 'au', eps: 1e-6 },
  { name: 'lightYear to m', input: '1 lightYear to m', expected: C * 365.25 * 86400, unit: 'm', eps: 1 },
  { name: 'Yds to m', input: '2 Yds to m', expected: 1.8288, unit: 'm' },
  { name: 'Fathom to ft', input: '1 Fathom to ft', expected: 6, unit: 'ft' },
  { name: 'Rod to ft', input: '1 Rod to ft', expected: 16.5, unit: 'ft' },
  { name: 'Chain to ft', input: '1 Chain to ft', expected: 66, unit: 'ft' },
  { name: 'Furlong to ft', input: '1 Furlong to ft', expected: 660, unit: 'ft' },
  { name: 'League to mi', input: '1 League to mi', expected: 3, unit: 'mi' },
  { name: 'NauticalMile to m', input: '1 NauticalMile to m', expected: 1852, unit: 'm' },
  { name: 'decade to years', input: '1 decade to years', expected: 10, unit: 'yr' },
  { name: 'century to years', input: '1 century to years', expected: 100, unit: 'yr' },
  { name: 'millenium to years', input: '1 millenium to years', expected: 1000, unit: 'yr' },
  { name: 'arcSecond to deg', input: '3600 arcSecond to deg', expected: 1, unit: 'deg' },
  { name: 'arcMinute to deg', input: '60 arcMinute to deg', expected: 1, unit: 'deg' },
  { name: 'revolution to deg', input: '1 revolution to deg', expected: 360, unit: 'deg' },
  { name: 'RPM to Hz', input: '60 RPM to Hz', expected: 1, unit: 'Hz' },
  { name: 'Hertz to RPM', input: '1 Hertz to RPM', expected: 60, unit: 'rpm' },
  { name: 'km/hr to m/s', input: '36 km/hr to m/s', expected: 10, unit: 'm/s' },
  { name: 'lightSpeed to m/s', input: '1 lightSpeed to m/s', expected: C, unit: 'm/s', eps: 1e-6 },
  { name: 'speedOfLight to m/s', input: '1 speedOfLight to m/s', expected: C, unit: 'm/s', eps: 1e-6 },
  { name: 'gravity to m/s^2', input: '1 gravity to m/s^2', expected: G0, unit: 'm/s²' },
  { name: 'gs to m/s2', input: '2 gs to m/s2', expected: 2 * G0, unit: 'm/s²' },
  { name: 'dyne to N', input: '1e5 dyne to N', expected: 1, unit: 'N' },
  { name: 'kgForce to N', input: '1 kgForce to N', expected: G0, unit: 'N' },
  { name: 'OunceForce to lbf', input: '16 OunceForce to lbf', expected: 1, unit: 'lbf' },
  { name: 'Ozf to N', input: '1 Ozf to N', expected: (G0 * LB) / 16, unit: 'N' },
  { name: 'PoundForce to N', input: '1 PoundForce to N', expected: G0 * LB, unit: 'N' },
  { name: 'kip to lbf', input: '1 kip to lbf', expected: 1000, unit: 'lbf' },
  { name: 'Torrs to Pa', input: '1 Torrs to Pa', expected: 101325 / 760, unit: 'Pa' },
  { name: 'electronVolt to J', input: '1 electronVolt to J', expected: E, unit: 'J', eps: 1e-28 },
  { name: 'eV to J', input: '1 eV to J', expected: E, unit: 'J', eps: 1e-28 },
  { name: 'erg to J', input: '1e7 erg to J', expected: 1, unit: 'J' },
  { name: 'Therm to BTU', input: '1 Therm to BTU', expected: 1e5, unit: 'BTU' },
  { name: 'HorsePower to W', input: '1 HorsePower to W', expected: 745.699872, unit: 'W' },
  { name: 'barn to m^2', input: '1 barn to m^2', expected: 1e-28, unit: 'm²', eps: 1e-36 },
  { name: 'darcy to m^2', input: '1 darcy to m^2', expected: 9.869232667160128e-13, unit: 'm²', eps: 1e-20 },
  { name: 'ccs to mL', input: '1 ccs to L', expected: 0.001, unit: 'L' },
  { name: 'stere to L', input: '1 stere to L', expected: 1000, unit: 'L' },
  { name: 'Drop to mL', input: '1 Drop to mL', expected: 0.05, unit: 'mL' },
  { name: 'Tbs to tsp', input: '1 Tbs to tsp', expected: 3, unit: 'tsp' },
  { name: 'Peck to bushels', input: '4 Peck to bushels', expected: 1, unit: 'bu' },
  { name: 'Barrel to gal', input: '1 Barrel to gal', expected: 42, unit: 'gal' },
  { name: 'Cord to ft3', input: '1 Cord to ft3', expected: 128, unit: 'ft³' },
  { name: 'electron to Coulomb', input: '1 electron to Coulomb', expected: E, unit: 'C', eps: 1e-28 },
  { name: 'amp to milliamp', input: '1 amp to milliamp', expected: 1000, unit: /milliamp/ },
  { name: 'Volt to kilovolt', input: '1000 Volt to kilovolt', expected: 1, unit: /kilovolt/ },
  { name: 'Ohm to kiloohm', input: '1000 Ohm to kiloohm', expected: 1, unit: /kiloohm/ },
  { name: 'Farad to millifarad', input: '1 Farad to millifarad', expected: 1000, unit: /millifarad/ },
  { name: 'Henry to millihenry', input: '1 Henry to millihenry', expected: 1000, unit: /millihenry/ },
  { name: 'Dimensionless to units', input: '5 Dimensionless to units', expected: 5, unit: 'units' },
  { name: 'NoUnit to None', input: '5 NoUnit to None', expected: 5, unit: 'dimensionless' },
])

suite('SI prefixes', [
  { name: 'one-word millisecond', input: '2 milliseconds to seconds', expected: 0.002, unit: 's' },
  { name: 'ms abbreviation', input: '2 ms to s', expected: 0.002, unit: 's' },
  { name: 'prefixed word milli seconds', input: '2 milli seconds to s', expected: 0.002, unit: 's' },
  { name: 'kilo meters as two words', input: '5 kilo meters to m', expected: 5000, unit: 'm' },
  { name: 'short prefix k meters', input: '5 k meters to m', expected: 5000, unit: 'm' },
  { name: 'megameter', input: '1 megameter to m', expected: 1e6, unit: 'm' },
  { name: 'mega joules', input: '3 mega joules to J', expected: 3e6, unit: 'J' },
  { name: 'millinewton', input: '1 millinewton to N', expected: 0.001, unit: 'N' },
  { name: 'kilovolt', input: '2 kilovolt to Volt', expected: 2000, unit: 'V' },
  { name: 'microgram', input: '1 microgram to g', expected: 1e-6, unit: 'g', eps: 1e-15 },
  { name: 'yoctosecond', input: '1 yoctosecond to s', expected: 1e-24, unit: 's', eps: 1e-32 },
  { name: 'yottameter', input: '1 yottameter to m', expected: 1e24, unit: 'm', eps: 1e12 },
  { name: 'femtometer is a fermi', input: '1 femtometer to fermi', expected: 1, unit: 'fm' },
  { name: 'milli inch is a mil', input: '1 milli inch to mil', expected: 1, unit: 'mil' },
  { name: 'centi prefix', input: '1 centi meter to cm', expected: 1, unit: 'cm' },
  { name: 'deka prefix', input: '1 deka meter to m', expected: 10, unit: 'm' },
  { name: 'deca spelling', input: '1 deca liter to L', expected: 10, unit: 'L' },
])

const REQUESTED_ALIASES = [
  'electronRestMass',
  'protonRestMass',
  'neutronRestMass',
  'AtomicMassUnit',
  'AMU',
  'gram',
  'grams',
  'g',
  'metricTon',
  'metricTons',
  'tonne',
  'Grain',
  'Grains',
  'Ozm',
  'OunceMass',
  'Lbm',
  'PoundMass',
  'Slug',
  'Slugs',
  'Snail',
  'Snails',
  'Slinch',
  'ShortTon',
  'ShortTons',
  'LongTon',
  'LongTons',
  'Fermi',
  'Fermis',
  'angstrom',
  'angstroms',
  'micron',
  'microns',
  'meter',
  'meters',
  'm',
  'astronomicalUnit',
  'astronomicalUnits',
  'parsec',
  'parsecs',
  'lightYear',
  'lightYears',
  'Mil',
  'Mils',
  'Inch',
  'Inches',
  'In',
  'Foot',
  'Feet',
  'Ft',
  'Yard',
  'Yards',
  'Yd',
  'Yds',
  'Fathom',
  'Fathoms',
  'Rod',
  'Rods',
  'Chain',
  'Chains',
  'Furlong',
  'Furlongs',
  'League',
  'Leagues',
  'Mile',
  'Miles',
  'Mi',
  'NauticalMile',
  'NauticalMiles',
  'second',
  'seconds',
  'sec',
  'secs',
  's',
  'minute',
  'minutes',
  'min',
  'mins',
  'hour',
  'hours',
  'hr',
  'hrs',
  'day',
  'days',
  'week',
  'weeks',
  'year',
  'years',
  'decade',
  'decades',
  'century',
  'centuries',
  'millenium',
  'arcSecond',
  'arcSeconds',
  'arcSec',
  'arcSecs',
  'arcMinute',
  'arcMinutes',
  'arcMin',
  'arcMins',
  'degree',
  'degrees',
  'deg',
  'degs',
  'radian',
  'radians',
  'rad',
  'rads',
  'revolution',
  'revolutions',
  'rev',
  'revs',
  'RPM',
  'Hertz',
  'Hz',
  'MPH',
  'kph',
  'km/hr',
  'Knot',
  'Knots',
  'lightSpeed',
  'speedOfLight',
  'gravity',
  'gs',
  'dyne',
  'dynes',
  'Newton',
  'Newtons',
  'N',
  'kgForce',
  'kgf',
  'OunceForce',
  'Ozf',
  'PoundForce',
  'Lbf',
  'kip',
  'kips',
  'Pascal',
  'Pascals',
  'Pa',
  'bar',
  'bars',
  'atmosphere',
  'atmospheres',
  'Torr',
  'Torrs',
  'PSI',
  'electronVolt',
  'electronVolts',
  'eV',
  'eVs',
  'erg',
  'ergs',
  'Joule',
  'Joules',
  'J',
  'calorie',
  'calories',
  'BTU',
  'BTUs',
  'Therm',
  'Therms',
  'Watt',
  'Watts',
  'HorsePower',
  'HP',
  'barn',
  'barns',
  'darcy',
  'darcys',
  'Acre',
  'Acres',
  'hectare',
  'hectares',
  'cc',
  'ccs',
  'liter',
  'liters',
  'l',
  'stere',
  'steres',
  'Drop',
  'Drops',
  'TeaSpoon',
  'TeaSpoons',
  'Tsp',
  'TableSpoon',
  'TableSpoons',
  'Tbs',
  'FluidOunce',
  'FluidOunces',
  'Cup',
  'Cups',
  'Pint',
  'Pints',
  'Quart',
  'Quarts',
  'Gallon',
  'Gallons',
  'Peck',
  'Pecks',
  'Bushel',
  'Bushels',
  'Barrel',
  'Barrels',
  'Cord',
  'Cords',
  'electron',
  'electrons',
  'Coulomb',
  'Coulombs',
  'amp',
  'amps',
  'Volt',
  'Volts',
  'Ohm',
  'Ohms',
  'Farad',
  'Farads',
  'Henry',
  'Henrys',
  'Dimensionless',
  'None',
  'NoUnit',
  'NoUnits',
  'Unit',
  'Units',
]

describe('Every requested unit alias', () => {
  it.each(REQUESTED_ALIASES)('accepts %s', (alias) => {
    const r = evaluateLine(`1 ${alias} to ${alias}`)
    expect(r.value?.n, alias).toBeCloseTo(1, 10)
  })
})

const PREFIX_NAMES = [
  'yocto',
  'zepto',
  'atto',
  'femto',
  'pico',
  'nano',
  'micro',
  'milli',
  'centi',
  'deci',
  'deka',
  'hecto',
  'kilo',
  'mega',
  'giga',
  'tera',
  'peta',
  'exa',
  'zetta',
  'yotta',
]

describe('Every requested SI prefix', () => {
  it.each(PREFIX_NAMES)('accepts %s meter', (prefix) => {
    const r = evaluateLine(`1 ${prefix} meter to m`)
    expect(r.value?.n, prefix).toEqual(expect.any(Number))
    expect(Number.isFinite(r.value!.n), prefix).toBe(true)
  })

  it('accepts short prefixes m, c, and k', () => {
    expect(evaluateLine('1 m meter to m').value?.n).toBeCloseTo(0.001, 12)
    expect(evaluateLine('1 c meter to m').value?.n).toBeCloseTo(0.01, 12)
    expect(evaluateLine('1 k meter to m').value?.n).toBeCloseTo(1000, 12)
  })
})

function relEps(expected: number): number {
  const a = Math.abs(expected)
  if (a === 0) return 1e-10
  return Math.max(1e-8, a * 1e-6)
}

function compat(title: string, cases: Case[]) {
  describe(title, () => {
    it.each(cases)('$name', (c) => run({ ...c, eps: c.eps ?? relEps(c.expected), unit: undefined }))
  })
}

const LBF = 4.4482216152605
const BTU = 1055.05585262
const HP = 745.699872
const US_FLOZ = 0.0295735295625
const US_TSP = US_FLOZ / 6
const OIL_BBL = 42 * US_GAL
const PARSEC = (AU * 648000) / Math.PI

compat('Mass conversions & operations', [
  { name: '1 kilogram to gram', input: '1 kilogram to gram', expected: 1000, unit: 'gram' },
  { name: '1 gram to milligram', input: '1 gram to milligram', expected: 1000, unit: 'milligram' },
  { name: '1 metricTon to kilogram', input: '1 metricTon to kilogram', expected: 1000, unit: 'kilogram' },
  { name: '1 tonne to g', input: '1 tonne to g', expected: 1e6, unit: 'g' },
  { name: '1 Grain to milligram', input: '1 Grain to milligram', expected: 64.79891, unit: 'milligram' },
  { name: '1 Ozm to Grain', input: '1 Ozm to Grain', expected: 437.5, unit: /Grain|gr/i },
  { name: '1 OunceMass to Lbm', input: '1 OunceMass to Lbm', expected: 0.0625, unit: /Lbm|lbs/i },
  { name: '1 Lbm to Ozm', input: '1 Lbm to Ozm', expected: 16, unit: /Ozm|oz/i },
  { name: '1 PoundMass to kilogram', input: '1 PoundMass to kilogram', expected: LB, unit: 'kilogram' },
  { name: '1 Slug to Lbm', input: '1 Slug to Lbm', expected: G0 / FT, unit: /Lbm|lbs/i },
  { name: '1 Slug to kilogram', input: '1 Slug to kilogram', expected: SLUG, unit: 'kilogram' },
  { name: '1 Snail to Slug', input: '1 Snail to Slug', expected: 12, unit: /Slug/i },
  { name: '1 Slinch to Slug', input: '1 Slinch to Slug', expected: 12, unit: /Slug/i },
  { name: '1 ShortTon to Lbm', input: '1 ShortTon to Lbm', expected: 2000, unit: /Lbm|lbs/i },
  { name: '1 LongTon to Lbm', input: '1 LongTon to Lbm', expected: 2240, unit: /Lbm|lbs/i },
  { name: '1 AMU to kg', input: '1 AMU to kg', expected: AMU, unit: 'kg' },
  { name: '1 protonRestMass to AMU', input: '1 protonRestMass to AMU', expected: 1.007276466621, unit: /AMU|u/i },
  { name: '1 neutronRestMass to AMU', input: '1 neutronRestMass to AMU', expected: 1.00866491588, unit: /AMU|u/i },
  { name: '1 electronRestMass to kg', input: '1 electronRestMass to kg', expected: 9.1093837015e-31, unit: 'kg' },
  { name: 'proton / electron rest mass', input: '1 protonRestMass / 1 electronRestMass', expected: (1.007276466621 * AMU) / 9.1093837015e-31 },
  { name: '10 Lbm + 5 Ozm', input: '10 Lbm + 5 Ozm', expected: 10.3125, unit: /Lbm|lbs/i },
  { name: '1 metricTon - 500 kg', input: '1 metricTon - 500 kg', expected: 0.5, unit: /metricTon|t/i },
  { name: '1 ShortTon + 1 LongTon to Lbm', input: '1 ShortTon + 1 LongTon to Lbm', expected: 4240, unit: /Lbm|lbs/i },
  { name: '1000 Grain to Ozm', input: '1000 Grain to Ozm', expected: 1000 / 437.5, unit: /Ozm|oz/i },
  { name: '1 Snail * 1 gravity to Lbf', input: '1 Snail * 1 gravity to Lbf', expected: (12 * SLUG * G0) / LBF, unit: /Lbf/i },
  { name: '1 yoctogram to g', input: '1 yoctogram to g', expected: 1e-24, unit: 'g' },
  { name: '1 yottagram to g', input: '1 yottagram to g', expected: 1e24, unit: 'g' },
  { name: '1 gigagram to metricTon', input: '1 gigagram to metricTon', expected: 1000, unit: /metricTon|t/i },
  { name: '500 AMU * 2', input: '500 AMU * 2', expected: 1000, unit: /AMU|u/i },
  { name: '1 ShortTon / 1 Lbm', input: '1 ShortTon / 1 Lbm', expected: 2000 },
])

compat('Length conversions & operations', [
  { name: '1 Fermi to meter', input: '1 Fermi to meter', expected: 1e-15, unit: 'meter' },
  { name: '1 angstrom to meter', input: '1 angstrom to meter', expected: 1e-10, unit: 'meter' },
  { name: '1 micron to meter', input: '1 micron to meter', expected: 1e-6, unit: 'meter' },
  { name: '1 meter to centi meter', input: '1 meter to centi meter', expected: 100, unit: /centi meter|cm/i },
  { name: '1 kilometer to m', input: '1 kilometer to m', expected: 1000, unit: 'm' },
  { name: '1 astronomicalUnit to meter', input: '1 astronomicalUnit to meter', expected: AU, unit: 'meter' },
  { name: '1 lightYear to meter', input: '1 lightYear to meter', expected: C * YEAR, unit: 'meter' },
  { name: '1 parsec to meter', input: '1 parsec to meter', expected: PARSEC, unit: 'meter' },
  { name: '1 parsec / 1 astronomicalUnit', input: '1 parsec / 1 astronomicalUnit', expected: 648000 / Math.PI },
  { name: '1 Mil to Inch', input: '1 Mil to Inch', expected: 0.001, unit: /Inch|in/i },
  { name: '1 Foot to Inch', input: '1 Foot to Inch', expected: 12, unit: /Inch|in/i },
  { name: '1 Yard to Foot', input: '1 Yard to Foot', expected: 3, unit: /Foot|ft/i },
  { name: '1 Yard to Inch', input: '1 Yard to Inch', expected: 36, unit: /Inch|in/i },
  { name: '1 Fathom to Foot', input: '1 Fathom to Foot', expected: 6, unit: /Foot|ft/i },
  { name: '1 Rod to Foot', input: '1 Rod to Foot', expected: 16.5, unit: /Foot|ft/i },
  { name: '1 Chain to Foot', input: '1 Chain to Foot', expected: 66, unit: /Foot|ft/i },
  { name: '1 Chain to Rod', input: '1 Chain to Rod', expected: 4, unit: /Rod/i },
  { name: '1 Furlong to Chain', input: '1 Furlong to Chain', expected: 10, unit: /Chain/i },
  { name: '1 Furlong to Foot', input: '1 Furlong to Foot', expected: 660, unit: /Foot|ft/i },
  { name: '1 Mile to Furlong', input: '1 Mile to Furlong', expected: 8, unit: /Furlong/i },
  { name: '1 Mile to Foot', input: '1 Mile to Foot', expected: 5280, unit: /Foot|ft/i },
  { name: '1 NauticalMile to meter', input: '1 NauticalMile to meter', expected: 1852, unit: 'meter' },
  { name: '1 League to Mile', input: '1 League to Mile', expected: 3, unit: /Mile|mi/i },
  { name: '1 Inch to centi meter', input: '1 Inch to centi meter', expected: 2.54, unit: /centi meter|cm/i },
  { name: '1 Foot to meter', input: '1 Foot to meter', expected: FT, unit: 'meter' },
  { name: '1 Yard to meter', input: '1 Yard to meter', expected: 0.9144, unit: 'meter' },
  { name: '1 Mile to kilometer', input: '1 Mile to kilometer', expected: 1.609344, unit: 'kilometer' },
  { name: '100 Mil to Inch', input: '100 Mil to Inch', expected: 0.1, unit: /Inch|in/i },
  { name: '1 Fathom + 2 Yard to Foot', input: '1 Fathom + 2 Yard to Foot', expected: 12, unit: /Foot|ft/i },
  { name: '1 Furlong + 20 Rod to Foot', input: '1 Furlong + 20 Rod to Foot', expected: 990, unit: /Foot|ft/i },
  { name: '1 lightYear / 1 speedOfLight to year', input: '1 lightYear / 1 speedOfLight to year', expected: 1, unit: /year|yr/i },
  { name: '1 megaparsec to parsec', input: '1 megaparsec to parsec', expected: 1e6, unit: 'parsec' },
  { name: '1 nanometer to angstrom', input: '1 nanometer to angstrom', expected: 10, unit: /angstrom/i },
  { name: '1 Fermi to femtometer', input: '1 Fermi to femtometer', expected: 1, unit: /femtometer|fm/i },
  { name: '1 League - 1 NauticalMile to m', input: '1 League - 1 NauticalMile to m', expected: 3 * 1609.344 - 1852, unit: 'm' },
])

compat('Time conversions & operations', [
  { name: '1 minute to second', input: '1 minute to second', expected: 60, unit: 'second' },
  { name: '1 hour to minute', input: '1 hour to minute', expected: 60, unit: 'minute' },
  { name: '1 hour to second', input: '1 hour to second', expected: 3600, unit: 'second' },
  { name: '1 day to hour', input: '1 day to hour', expected: 24, unit: 'hour' },
  { name: '1 day to second', input: '1 day to second', expected: 86400, unit: 'second' },
  { name: '1 week to day', input: '1 week to day', expected: 7, unit: 'day' },
  { name: '1 week to hour', input: '1 week to hour', expected: 168, unit: 'hour' },
  { name: '1 year to day', input: '1 year to day', expected: 365.25, unit: 'day' },
  { name: '1 decade to year', input: '1 decade to year', expected: 10, unit: 'year' },
  { name: '1 century to year', input: '1 century to year', expected: 100, unit: 'year' },
  { name: '1 millenium to year', input: '1 millenium to year', expected: 1000, unit: 'year' },
  { name: '1 millisecond to s', input: '1 millisecond to s', expected: 0.001, unit: 's' },
  { name: '1 microsecond to sec', input: '1 microsecond to sec', expected: 1e-6, unit: 'sec' },
  { name: '1 nanosecond to s', input: '1 nanosecond to s', expected: 1e-9, unit: 's' },
  { name: '1 picosecond to s', input: '1 picosecond to s', expected: 1e-12, unit: 's' },
  { name: '1 day - 12 hours to s', input: '1 day - 12 hours to s', expected: 43200, unit: 's' },
  { name: '1 week / 1 day', input: '1 week / 1 day', expected: 7 },
  { name: '1 century / 1 decade', input: '1 century / 1 decade', expected: 10 },
  { name: '1 year / 1 month', input: '1 year / 1 month', expected: 12 },
  { name: '1 millenium / 1 century', input: '1 millenium / 1 century', expected: 10 },
])

compat('Angle, frequency & angular velocity', [
  { name: '1 revolution to degree', input: '1 revolution to degree', expected: 360, unit: 'degree' },
  { name: '1 revolution to radian', input: '1 revolution to radian', expected: 2 * Math.PI, unit: 'radian' },
  { name: '1 degree to arcMinute', input: '1 degree to arcMinute', expected: 60, unit: /arcMinute|′/ },
  { name: '1 arcMinute to arcSecond', input: '1 arcMinute to arcSecond', expected: 60, unit: /arcSecond|″/ },
  { name: '1 degree to arcSecond', input: '1 degree to arcSecond', expected: 3600, unit: /arcSecond|″/ },
  { name: '1 radian to degree', input: '1 radian to degree', expected: 180 / Math.PI, unit: 'degree' },
  { name: '180 degree to radian', input: '180 degree to radian', expected: Math.PI, unit: 'radian' },
  { name: '90 deg to rad', input: '90 deg to rad', expected: Math.PI / 2, unit: 'rad' },
  { name: '1 RPM to revolution / minute', input: '1 RPM to revolution / minute', expected: 1, unit: /revolution \/ minute|rev/i },
  { name: '1 RPM to rad/s', input: '1 RPM to rad / s', expected: (2 * Math.PI) / 60, unit: /rad/ },
  { name: '60 RPM to Hertz', input: '60 RPM to Hertz', expected: 1, unit: /Hertz|Hz/i },
  { name: '1 Hertz to 1 / second', input: '1 Hertz to 1 / 1 second', expected: 1 },
  { name: '1 kHz to Hz', input: '1 kHz to Hz', expected: 1000, unit: 'Hz' },
  { name: '1 MHz to Hertz', input: '1 MHz to Hertz', expected: 1e6, unit: /Hertz|Hz/i },
  { name: '1 GHz to Hz', input: '1 GHz to Hz', expected: 1e9, unit: 'Hz' },
  { name: '3600 arcSec to deg', input: '3600 arcSec to deg', expected: 1, unit: 'deg' },
  { name: '2 * pi * rad to rev', input: '2 * pi * rad to rev', expected: 1, unit: 'rev' },
  { name: '1 rev / 1 sec to RPM', input: '1 rev / 1 sec to RPM', expected: 60, unit: /RPM/i },
  { name: '120 RPM to Hz', input: '120 RPM to Hz', expected: 2, unit: 'Hz' },
  { name: '1000 arcMin to deg', input: '1000 arcMin to deg', expected: 1000 / 60, unit: 'deg' },
])

compat('Velocity & acceleration', [
  { name: '1 MPH to Mile / hour', input: '1 MPH to Mile / hour', expected: 1, unit: /Mile \/ hour|mi/i },
  { name: '1 MPH to meter / second', input: '1 MPH to meter / second', expected: 1609.344 / 3600, unit: /meter \/ second|m \/ s/i },
  { name: '1 kph to kilometer / hour', input: '1 kph to kilometer / hour', expected: 1, unit: /kilometer \/ hour|km/i },
  { name: '1 kph to m/s', input: '1 kph to m/s', expected: 1000 / 3600, unit: 'm/s' },
  { name: '1 Knot to NauticalMile / hour', input: '1 Knot to NauticalMile / hour', expected: 1 },
  { name: '1 Knot to m/s', input: '1 Knot to m/s', expected: 1852 / 3600, unit: 'm/s' },
  { name: '1 speedOfLight to meter / second', input: '1 speedOfLight to meter / second', expected: C },
  { name: '1 lightSpeed to m/s', input: '1 lightSpeed to m/s', expected: C, unit: 'm/s' },
  { name: '1 gravity to meter / second^2', input: '1 gravity to meter / second^2', expected: G0 },
  { name: '1 gravity to Foot / second^2', input: '1 gravity to Foot / second^2', expected: G0 / FT },
  { name: '100 kph to m/s', input: '100 kph to m/s', expected: 100 / 3.6, unit: 'm/s' },
  { name: '60 MPH to Foot/s', input: '60 MPH to Foot / s', expected: 88, unit: /Foot|ft/i },
  { name: '1 Knot to MPH', input: '1 Knot to MPH', expected: 1852 / 1609.344, unit: 'mph' },
  { name: '0.5 lightSpeed to m/s', input: '0.5 lightSpeed to m/s', expected: C / 2, unit: 'm/s' },
  { name: '2 gs to m/s^2', input: '2 gs to m/s^2', expected: 2 * G0, unit: 'm/s²' },
  { name: '10 MPH + 5 MPH', input: '10 MPH + 5 MPH', expected: 15, unit: 'mph' },
  { name: '100 meter / 10 second', input: '100 meter / 10 second', expected: 10, unit: /m \/ s|m\/s/ },
  { name: '60 Mile / 1 hour to MPH', input: '60 Mile / 1 hour to MPH', expected: 60, unit: 'mph' },
  { name: '1 lightSpeed / 1 AU to Hz', input: '1 lightSpeed / 1 AU to Hz', expected: C / AU, unit: 'Hz' },
  { name: '1 gravity * 1 second', input: '1 gravity * 1 second', expected: G0, unit: /m \/ s|m\/s/ },
])

compat('Force & pressure', [
  { name: '1 Newton to kilogram * meter / second^2', input: '1 Newton to kilogram * meter / second^2', expected: 1 },
  { name: '1 Newton to dyne', input: '1 Newton to dyne', expected: 1e5, unit: 'dyne' },
  { name: '1 kgForce to Newton', input: '1 kgForce to Newton', expected: G0, unit: 'Newton' },
  { name: '1 PoundForce to Newton', input: '1 PoundForce to Newton', expected: LBF, unit: 'Newton' },
  { name: '1 PoundForce to Slug * Foot / second^2', input: '1 PoundForce to Slug * Foot / second^2', expected: 1 },
  { name: '1 OunceForce to Lbf', input: '1 OunceForce to Lbf', expected: 0.0625, unit: /Lbf/i },
  { name: '1 kip to PoundForce', input: '1 kip to PoundForce', expected: 1000, unit: /PoundForce|lbf/i },
  { name: '1 Pascal to Newton / meter^2', input: '1 Pascal to Newton / meter^2', expected: 1 },
  { name: '1 bar to Pascal', input: '1 bar to Pascal', expected: 1e5, unit: 'Pascal' },
  { name: '1 atmosphere to Pascal', input: '1 atmosphere to Pascal', expected: 101325, unit: 'Pascal' },
  { name: '1 atmosphere to Torr', input: '1 atmosphere to Torr', expected: 760, unit: /Torr/i },
  { name: '1 Torr to Pascal', input: '1 Torr to Pascal', expected: 101325 / 760, unit: 'Pascal' },
  { name: '1 PSI to PoundForce / Inch^2', input: '1 PSI to PoundForce / Inch^2', expected: 1 },
  { name: '1 PSI to Pascal', input: '1 PSI to Pascal', expected: 6894.757293168361, unit: 'Pascal' },
  { name: '1 atmosphere to PSI', input: '1 atmosphere to PSI', expected: 101325 / 6894.757293168361, unit: /PSI|psi/i },
  { name: '100 kPa to bar', input: '100 kPa to bar', expected: 1, unit: 'bar' },
  { name: '1 megapascal to Pa', input: '1 megapascal to Pa', expected: 1e6, unit: 'Pa' },
  { name: '1 gigapascal to Pa', input: '1 gigapascal to Pa', expected: 1e9, unit: 'Pa' },
  { name: '1 dynes to N', input: '1 dynes to N', expected: 1e-5, unit: 'N' },
  { name: '1 kip to Newton', input: '1 kip to Newton', expected: 1000 * LBF, unit: 'Newton' },
  { name: '1 kgf to kg * gravity', input: '1 kgf to kg * gravity', expected: 1 },
  { name: '1 Ozf to N', input: '1 Ozf to N', expected: LBF / 16, unit: 'N' },
  { name: '10 bar to MPa', input: '10 bar to MPa', expected: 1, unit: 'MPa' },
  { name: '1 atm - 1 bar to Pa', input: '1 atm - 1 bar to Pa', expected: 1325, unit: 'Pa' },
  { name: '50 PSI to bar', input: '50 PSI to bar', expected: (50 * 6894.757293168361) / 1e5, unit: 'bar' },
])

compat('Work, energy & power', [
  { name: '1 Joule to Newton * meter', input: '1 Joule to Newton * meter', expected: 1 },
  { name: '1 Joule to erg', input: '1 Joule to erg', expected: 1e7, unit: 'erg' },
  { name: '1 electronVolt to Joule', input: '1 electronVolt to Joule', expected: E, unit: 'Joule' },
  { name: '1 calorie to Joule', input: '1 calorie to Joule', expected: 4.184, unit: 'Joule' },
  { name: '1 BTU to Joule', input: '1 BTU to Joule', expected: BTU, unit: 'Joule' },
  { name: '1 Therm to BTU', input: '1 Therm to BTU', expected: 1e5, unit: 'BTU' },
  { name: '1 Therm to Joule', input: '1 Therm to Joule', expected: 1e5 * BTU, unit: 'Joule' },
  { name: '1 Watt to Joule / second', input: '1 Watt to Joule / second', expected: 1 },
  { name: '1 HorsePower to Watt', input: '1 HorsePower to Watt', expected: HP, unit: 'Watt' },
  { name: '1 HorsePower to Foot * PoundForce / second', input: '1 HorsePower to Foot * PoundForce / second', expected: 550 },
  { name: '1 kilowatt to Watt', input: '1 kilowatt to Watt', expected: 1000, unit: 'Watt' },
  { name: '1 megawatt to Watt', input: '1 megawatt to Watt', expected: 1e6, unit: 'Watt' },
  { name: '1 gigawatt to W', input: '1 gigawatt to W', expected: 1e9, unit: 'W' },
  { name: '1 kilowatt * 1 hour to Joule', input: '1 kilowatt * 1 hour to Joule', expected: 3.6e6, unit: 'Joule' },
  { name: '1 keV to eV', input: '1 keV to eV', expected: 1000, unit: 'eV' },
  { name: '1 MeV to eV', input: '1 MeV to eV', expected: 1e6, unit: 'eV' },
  { name: '1 GeV to eV', input: '1 GeV to eV', expected: 1e9, unit: 'eV' },
  { name: '1 TeV to eV', input: '1 TeV to eV', expected: 1e12, unit: 'eV' },
  { name: '1000 calorie to Joule', input: '1000 calorie to Joule', expected: 4184, unit: 'Joule' },
  { name: '1 Therm / 1 BTU', input: '1 Therm / 1 BTU', expected: 1e5 },
  { name: '1 HP to Foot * Lbf / sec', input: '1 HP to Foot * Lbf / sec', expected: 550 },
  { name: '1 erg to dyne * centi meter', input: '1 erg to dyne * centi meter', expected: 1 },
  { name: '1 Joule / 1 second', input: '1 Joule / 1 second', expected: 1, unit: 'W' },
  { name: '100 Watt * 10 second to Joule', input: '100 Watt * 10 second to Joule', expected: 1000, unit: 'Joule' },
  { name: '1 BTU to calorie', input: '1 BTU to calorie', expected: BTU / 4.184, unit: 'calorie' },
  { name: '100 HP to Watt', input: '100 HP to Watt', expected: 100 * HP, unit: 'Watt' },
  { name: '1 electronVolt * 1e6 to MeV', input: '1 electronVolt * 1e6 to MeV', expected: 1, unit: 'MeV' },
  { name: '1 gigaElectronVolt to GeV', input: '1 gigaElectronVolt to GeV', expected: 1, unit: 'GeV' },
  { name: '1 microJoule to J', input: '1 microJoule to J', expected: 1e-6, unit: 'J' },
  { name: '1 teraWatt to W', input: '1 teraWatt to W', expected: 1e12, unit: 'W' },
])

compat('Area & volume', [
  { name: '1 barn to meter^2', input: '1 barn to meter^2', expected: 1e-28 },
  { name: '1 darcy to meter^2', input: '1 darcy to meter^2', expected: 9.869232667160128e-13 },
  { name: '1 Acre to Foot^2', input: '1 Acre to Foot^2', expected: 43560 },
  { name: '1 Acre to Chain^2', input: '1 Acre to Chain^2', expected: 10 },
  { name: '1 hectare to meter^2', input: '1 hectare to meter^2', expected: 10000 },
  { name: '1 hectare to Acre', input: '1 hectare to Acre', expected: 10000 / 4046.8564224, unit: /Acre/i },
  { name: '1 cc to centi meter^3', input: '1 cc to centi meter^3', expected: 1 },
  { name: '1 liter to cc', input: '1 liter to cc', expected: 1000, unit: 'cc' },
  { name: '1 liter to meter^3', input: '1 liter to meter^3', expected: 0.001 },
  { name: '1 stere to meter^3', input: '1 stere to meter^3', expected: 1 },
  { name: '1 Drop to milliliter', input: '1 Drop to milliliter', expected: 0.05, unit: 'milliliter' },
  { name: '1 TeaSpoon to milliliter', input: '1 TeaSpoon to milliliter', expected: US_TSP * 1000, unit: 'milliliter' },
  { name: '1 TableSpoon to TeaSpoon', input: '1 TableSpoon to TeaSpoon', expected: 3, unit: /TeaSpoon|tsp/i },
  { name: '1 FluidOunce to TableSpoon', input: '1 FluidOunce to TableSpoon', expected: 2, unit: /TableSpoon|tbsp/i },
  { name: '1 Cup to FluidOunce', input: '1 Cup to FluidOunce', expected: 8, unit: /FluidOunce|fl oz/i },
  { name: '1 Pint to Cup', input: '1 Pint to Cup', expected: 2, unit: /Cup/i },
  { name: '1 Quart to Pint', input: '1 Quart to Pint', expected: 2, unit: /Pint|pt/i },
  { name: '1 Gallon to Quart', input: '1 Gallon to Quart', expected: 4, unit: /Quart|qt/i },
  { name: '1 Gallon to Inch^3', input: '1 Gallon to Inch^3', expected: 231 },
  { name: '1 Peck to Bushel', input: '1 Peck to Bushel', expected: 0.25 },
  { name: '1 Bushel to Peck', input: '1 Bushel to Peck', expected: 4, unit: /Peck/i },
  { name: '1 Barrel to Gallon', input: '1 Barrel to Gallon', expected: 42, unit: /Gallon|gal/i },
  { name: '1 Cord to Foot^3', input: '1 Cord to Foot^3', expected: 128 },
  { name: '1 Acre * 1 Foot to Foot^3', input: '1 Acre * 1 Foot to Foot^3', expected: 43560 },
  { name: '1 Gallon to liter', input: '1 Gallon to liter', expected: US_GAL, unit: 'liter' },
  { name: '1 milliliter to cc', input: '1 milliliter to cc', expected: 1, unit: 'cc' },
  { name: '1 kiloliter to stere', input: '1 kiloliter to stere', expected: 1, unit: 'stere' },
  { name: '1 kiloliter to liter', input: '1 kiloliter to liter', expected: 1000, unit: 'liter' },
  { name: '1 Bushel to Inch^3', input: '1 Bushel to Inch^3', expected: 2150.42 },
  { name: '1 Barrel to liter', input: '1 Barrel to liter', expected: OIL_BBL, unit: 'liter' },
  { name: '16 FluidOunce to Pint', input: '16 FluidOunce to Pint', expected: 1, unit: /Pint|pt/i },
  { name: '128 FluidOunce to Gallon', input: '128 FluidOunce to Gallon', expected: 1, unit: /Gallon|gal/i },
  { name: '1 Cord / 1 Foot^3', input: '1 Cord / 1 Foot^3', expected: 128 },
  { name: '100 hectopascal to Pa', input: '100 hectopascal to Pa', expected: 10000, unit: 'Pa' },
  { name: '1 Megabarn to m^2', input: '1 Megabarn to m^2', expected: 1e-22, unit: 'm²' },
  { name: '1 square meter to centi meter^2', input: '1 square meter to centi meter^2', expected: 10000 },
  { name: '1 square kilometer to hectare', input: '1 square kilometer to hectare', expected: 100, unit: 'hectare' },
  { name: '1000 Drop to ml', input: '1000 Drop to ml', expected: 50, unit: /ml|mL/ },
  { name: '3 Tsp to Tbs', input: '3 Tsp to Tbs', expected: 1, unit: /Tbs|tbsp/i },
  { name: '16 Tbs to Cup', input: '16 Tbs to Cup', expected: 1, unit: /Cup/i },
])

compat('Charge & electrical units', [
  { name: '1 Coulomb to amp * second', input: '1 Coulomb to amp * second', expected: 1 },
  { name: '1 electron to Coulomb', input: '1 electron to Coulomb', expected: E, unit: 'Coulomb' },
  { name: '1 Coulomb / 1 electron', input: '1 Coulomb / 1 electron', expected: 1 / E },
  { name: '1 Volt to Joule / Coulomb', input: '1 Volt to Joule / Coulomb', expected: 1 },
  { name: '1 Ohm to Volt / amp', input: '1 Ohm to Volt / amp', expected: 1 },
  { name: '1 Farad to Coulomb / Volt', input: '1 Farad to Coulomb / Volt', expected: 1 },
  { name: '1 Henry to Volt * second / amp', input: '1 Henry to Volt * second / amp', expected: 1 },
  { name: '1 amp * 1 Ohm', input: '1 amp * 1 Ohm', expected: 1, unit: 'V' },
  { name: '1 Volt * 1 amp', input: '1 Volt * 1 amp', expected: 1, unit: 'W' },
  { name: '1 Farad * 1 Volt', input: '1 Farad * 1 Volt', expected: 1, unit: 'C' },
  { name: '1 Henry * 1 amp / 1 second', input: '1 Henry * 1 amp / 1 second', expected: 1, unit: 'V' },
  { name: '1 milliamp to amp', input: '1 milliamp to amp', expected: 0.001, unit: 'amp' },
  { name: '1 microfarad to Farad', input: '1 microfarad to Farad', expected: 1e-6, unit: 'Farad' },
  { name: '1 picofarad to Farad', input: '1 picofarad to Farad', expected: 1e-12, unit: 'Farad' },
  { name: '1 kilovolt to Volt', input: '1 kilovolt to Volt', expected: 1000, unit: 'Volt' },
  { name: '1 megohm to Ohm', input: '1 megohm to Ohm', expected: 1e6, unit: 'Ohm' },
  { name: '1 millihenry to Henry', input: '1 millihenry to Henry', expected: 0.001, unit: 'Henry' },
  { name: '1 Coulomb / 1 second', input: '1 Coulomb / 1 second', expected: 1, unit: 'A' },
  { name: '1 Volt / 1 Ohm', input: '1 Volt / 1 Ohm', expected: 1, unit: 'A' },
  { name: '1 Watt / 1 Volt', input: '1 Watt / 1 Volt', expected: 1, unit: 'A' },
  { name: '1 Joule / 1 Volt', input: '1 Joule / 1 Volt', expected: 1, unit: 'C' },
  { name: '1 Farad * 1 Ohm', input: '1 Farad * 1 Ohm', expected: 1, unit: 's' },
  { name: '1 Henry / 1 Ohm', input: '1 Henry / 1 Ohm', expected: 1, unit: 's' },
  { name: '1 milliampere * 1 hour to Coulomb', input: '1 milliampere * 1 hour to Coulomb', expected: 3.6, unit: 'Coulomb' },
  { name: '1 microampere to amp', input: '1 microampere to amp', expected: 1e-6, unit: 'amp' },
])

describe('Reciprocal named units', () => {
  it('keeps 1/ohm instead of expanding to SI base units', () => {
    const r = evaluateLine('1/6.8 ohm')
    expect(r.value?.n).toBeCloseTo(1 / 6.8, 12)
    expect(r.display).toMatch(/1\/Ω$/)
    expect(r.display).not.toMatch(/s\^3/)
  })

  it('accepts 1/ohm with a space and the plural', () => {
    const r = evaluateLine('1 / 6.8 ohms')
    expect(r.value?.n).toBeCloseTo(1 / 6.8, 12)
    expect(r.display).toMatch(/1\/Ω$/)
  })

  it('keeps 1/farad and 1/henry', () => {
    expect(evaluateLine('1/2 farad').display).toMatch(/0\.5 1\/F$/)
    expect(evaluateLine('1/2 henry').display).toMatch(/0\.5 1\/H$/)
  })

  it('still names 1/s as hertz', () => {
    const r = evaluateLine('1 / 1 second')
    expect(r.value?.n).toBeCloseTo(1, 12)
    expect(r.display).toMatch(/Hz$/)
  })
})

compat('Dimensionless, unit prefix scaling & aliases', [
  { name: '1 yocto', input: '1 yocto', expected: 1e-24 },
  { name: '1 zepto', input: '1 zepto', expected: 1e-21 },
  { name: '1 atto', input: '1 atto', expected: 1e-18 },
  { name: '1 femto', input: '1 femto', expected: 1e-15 },
  { name: '1 pico', input: '1 pico', expected: 1e-12 },
  { name: '1 nano', input: '1 nano', expected: 1e-9 },
  { name: '1 micro', input: '1 micro', expected: 1e-6 },
  { name: '1 milli', input: '1 milli', expected: 1e-3 },
  { name: '1 centi', input: '1 centi', expected: 1e-2 },
  { name: '1 deci', input: '1 deci', expected: 1e-1 },
  { name: '1 deka', input: '1 deka', expected: 10 },
  { name: '1 hecto', input: '1 hecto', expected: 100 },
  { name: '1 kilo', input: '1 kilo', expected: 1000 },
  { name: '1 mega', input: '1 mega', expected: 1e6 },
  { name: '1 giga', input: '1 giga', expected: 1e9 },
  { name: '1 tera', input: '1 tera', expected: 1e12 },
  { name: '1 peta', input: '1 peta', expected: 1e15 },
  { name: '1 exa', input: '1 exa', expected: 1e18 },
  { name: '1 zetta', input: '1 zetta', expected: 1e21 },
  { name: '1 yotta', input: '1 yotta', expected: 1e24 },
  { name: '1 Dimensionless', input: '1 Dimensionless', expected: 1 },
  { name: '1 None', input: '1 None', expected: 1 },
  { name: '1 NoUnit', input: '1 NoUnit', expected: 1 },
  { name: '1 NoUnits', input: '1 NoUnits', expected: 1 },
  { name: '1 Unit', input: '1 Unit', expected: 1 },
  { name: '1 Units', input: '1 Units', expected: 1 },
  { name: '5 * Dimensionless', input: '5 * Dimensionless', expected: 5 },
  { name: '10 * NoUnit', input: '10 * NoUnit', expected: 10 },
  { name: '1 kilo * 1 milli', input: '1 kilo * 1 milli', expected: 1 },
  { name: '1 mega * 1 micro', input: '1 mega * 1 micro', expected: 1 },
  { name: '1 giga * 1 nano', input: '1 giga * 1 nano', expected: 1 },
  { name: '1 tera * 1 pico', input: '1 tera * 1 pico', expected: 1 },
  { name: '1 peta * 1 femto', input: '1 peta * 1 femto', expected: 1 },
  { name: '1 exa * 1 atto', input: '1 exa * 1 atto', expected: 1 },
  { name: '1 zetta * 1 zepto', input: '1 zetta * 1 zepto', expected: 1 },
  { name: '1 yotta * 1 yocto', input: '1 yotta * 1 yocto', expected: 1 },
  { name: '1 hectometer to meter', input: '1 hectometer to meter', expected: 100, unit: 'meter' },
  { name: '1 dekameter to meter', input: '1 dekameter to meter', expected: 10, unit: 'meter' },
  { name: '1 decimeter to meter', input: '1 decimeter to meter', expected: 0.1, unit: 'meter' },
  { name: '1 centigram to gram', input: '1 centigram to gram', expected: 0.01, unit: 'gram' },
])

compat('Kinematics & mechanics', [
  { name: '10 kg * 5 m/s^2', input: '10 kg * 5 m/s^2', expected: 50, unit: 'N' },
  { name: '2 Slug * 10 Ft/s^2', input: '2 Slug * 10 Ft / s^2 to Lbf', expected: 20 },
  { name: '1 Snail * 1 gravity to Lbf', input: '1 Snail * 1 gravity to Lbf', expected: (12 * SLUG * G0) / LBF, unit: /Lbf/i },
  { name: '100 Lbm * g', input: '100 Lbm * 32.17404856 Foot / second^2 to PoundForce', expected: 100 },
  { name: '5 metricTon * 2 m/s^2', input: '5 metricTon * 2 m/s^2 to kilonewton', expected: 10 },
  { name: '1 kg * 10 m/s', input: '1 kg * 10 m/s', expected: 10, unit: /kg m \/ s/ },
  { name: '0.5 * 2 kg * (10 m/s)^2', input: '0.5 * 2 kg * (10 m/s)^2', expected: 100, unit: 'J' },
  { name: '0.5 * 1 Slug * (60 Foot / second)^2', input: '0.5 * 1 Slug * (60 Foot / second)^2 to Foot * PoundForce', expected: 1800 },
  { name: '10 N * 5 m', input: '10 N * 5 m', expected: 50, unit: 'J' },
  { name: '100 Lbf * 10 Foot', input: '100 Lbf * 10 Foot to Foot * PoundForce', expected: 1000 },
  { name: '500 dynes * 20 centi meter', input: '500 dynes * 20 centi meter to erg', expected: 10000 },
  { name: '50 Joule / 5 second', input: '50 Joule / 5 second', expected: 10, unit: 'W' },
  { name: '550 Foot * PoundForce / 1 second', input: '550 Foot * PoundForce / 1 second to HorsePower', expected: 1 },
  { name: '1 kg * 9.80665 m/s^2', input: '1 kg * 9.80665 m/s^2 to kgForce', expected: 1 },
  { name: '1 ShortTon * 1 gravity to Lbf', input: '1 ShortTon * 1 gravity to Lbf', expected: 2000, unit: /Lbf/i },
])

compat('Pressure, stress & fluid dynamics', [
  { name: '100 Newton / 2 meter^2', input: '100 Newton / 2 meter^2', expected: 50, unit: 'Pa' },
  { name: '1000 PoundForce / 1 Inch^2 to ksi', input: '1000 PoundForce / 1 Inch^2 to ksi', expected: 1, unit: 'ksi' },
  { name: '1 ksi to PSI', input: '1 ksi to PSI', expected: 1000, unit: /PSI|psi/i },
  { name: '1 bar * 1 meter^2', input: '1 bar * 1 meter^2', expected: 1e5, unit: 'N' },
  { name: '1 atmosphere * 1 foot^2 to Lbf', input: '1 atmosphere * 1 foot^2 to Lbf', expected: (101325 * FT * FT) / LBF, unit: /Lbf/i },
  { name: '10 kg / 1 liter', input: '10 kg / 1 liter', expected: 10000, unit: /kg \/ m/ },
  { name: '1 Lbm / 1 Gallon to kg/l', input: '1 Lbm / 1 Gallon to kg / liter', expected: LB / US_GAL },
  { name: '1 Slug / 1 Foot^3 to kg/m^3', input: '1 Slug / 1 Foot^3 to kg / m^3', expected: SLUG / (FT ** 3) },
  { name: '1 gram / 1 cc', input: '1 gram / 1 cc', expected: 1000, unit: /kg \/ m/ },
  { name: '100 Pascal * 5 meter^3', input: '100 Pascal * 5 meter^3', expected: 500, unit: 'J' },
  { name: '1 PSI * 1 Gallon to Joule', input: '1 PSI * 1 Gallon to Joule', expected: 6894.757293168361 * US_GAL * 0.001, unit: 'Joule' },
])

compat('Electromagnetism', [
  { name: '2 amp * 10 Ohm', input: '2 amp * 10 Ohm', expected: 20, unit: 'V' },
  { name: '12 Volt / 4 Ohm', input: '12 Volt / 4 Ohm', expected: 3, unit: 'A' },
  { name: '120 Volt * 10 amp', input: '120 Volt * 10 amp', expected: 1200, unit: 'W' },
  { name: '5 amp * 30 second', input: '5 amp * 30 second', expected: 150, unit: 'C' },
  { name: '0.5 * 10 microfarad * (12 Volt)^2', input: '0.5 * 10 microfarad * (12 Volt)^2', expected: 0.00072, unit: 'J' },
  { name: '0.5 * 2 millihenry * (5 amp)^2', input: '0.5 * 2 millihenry * (5 amp)^2', expected: 0.025, unit: 'J' },
  { name: '1 Coulomb * 10 Volt', input: '1 Coulomb * 10 Volt', expected: 10, unit: 'J' },
  { name: '100 Volt / 2 amp', input: '100 Volt / 2 amp', expected: 50, unit: /Ω|ohm/i },
  { name: '1 Farad * 1 Ohm', input: '1 Farad * 1 Ohm', expected: 1, unit: 's' },
  { name: '1 Henry / 1 Ohm', input: '1 Henry / 1 Ohm', expected: 1, unit: 's' },
])

compat('Quantum, relativistic & atomic physics', [
  { name: 'electron rest energy in J', input: '1 electronRestMass * (1 speedOfLight)^2', expected: 9.1093837015e-31 * C * C, unit: 'J' },
  { name: 'electron rest energy to eV', input: '1 electronRestMass * (1 speedOfLight)^2 to eV', expected: (9.1093837015e-31 * C * C) / E, unit: /eV|MeV/ },
  { name: '1 AMU * c^2 to eV', input: '1 AMU * (1 speedOfLight)^2 to eV', expected: (AMU * C * C) / E, unit: /eV|MeV/ },
  { name: 'proton rest energy to eV', input: '1 protonRestMass * (1 speedOfLight)^2 to eV', expected: (1.007276466621 * AMU * C * C) / E, unit: /eV|MeV/ },
  { name: 'neutron rest energy to eV', input: '1 neutronRestMass * (1 speedOfLight)^2 to eV', expected: (1.00866491588 * AMU * C * C) / E, unit: /eV|MeV/ },
  { name: '100 eV / 1 electron', input: '100 eV / 1 electron', expected: 100, unit: 'V' },
  { name: '1 MeV / 1 electron', input: '1 MeV / 1 electron', expected: 1e6, unit: 'V' },
  { name: 'electron / proton mass ratio', input: '1 electronRestMass / 1 protonRestMass', expected: 9.1093837015e-31 / (1.007276466621 * AMU) },
  { name: '1 photon * 100 eV', input: '1 photon * 100 eV to Joule', expected: 100 * E },
  { name: '1 barn * 1 megabarrels', input: '1 barn * 1 megabarrels', expected: 1e-28 * 1e6 * OIL_BBL * 0.001, unit: /m\^5/ },
])

compat('Angular dynamics', [
  { name: '10 Newton * 2 meter', input: '10 Newton * 2 meter', expected: 20, unit: /N|J/ },
  { name: '100 Lbf * 2 Foot', input: '100 Lbf * 2 Foot to Foot * PoundForce', expected: 200 },
  { name: '10 N*m * 100 RPM', input: '10 N * m * 100 RPM', expected: 10 * 100 * ((2 * Math.PI) / 60), unit: 'W' },
  { name: '1 HP / 1750 RPM', input: '1 HP / 1750 RPM', expected: HP / (1750 * ((2 * Math.PI) / 60)) },
  { name: '1 kg * m^2 * 10 rad/s^2', input: '1 kg * m^2 * 10 rad / s^2', expected: 10, unit: /N|J/ },
  { name: '2 Slug * Foot^2 * 5 rad/s', input: '2 Slug * Foot^2 * 5 rad / s to Slug * Foot^2 / s', expected: 10 },
  { name: '360 degree / 1 second to RPM', input: '360 degree / 1 second to RPM', expected: 60, unit: /RPM/i },
  { name: '2 * pi * rad / 1 minute to RPM', input: '2 * pi * rad / 1 minute to RPM', expected: 1, unit: /RPM/i },
  { name: '100 Hertz * 2 * pi * rad', input: '100 Hertz * 2 * pi * rad', expected: 200 * Math.PI, unit: /rad \/ s/ },
  { name: '1000 RPM / 60', input: '1000 RPM to Hz', expected: 1000 / 60 },
])

compat('Ideal gas law & thermodynamics', [
  { name: '1 atmosphere * 22.414 liter', input: '1 atmosphere * 22.414 liter', expected: 101325 * 0.022414, unit: 'J' },
  { name: '100 kPa * 1 meter^3', input: '100 kPa * 1 meter^3', expected: 1e5, unit: 'J' },
  { name: '1 BTU / 1 Lbm to J / kg', input: '1 BTU / 1 Lbm to Joule / kilogram', expected: BTU / LB },
  { name: '1 calorie / 1 gram', input: '1 calorie / 1 gram', expected: 4184, unit: /Joule \/ kilogram|J \/ kg/i },
  { name: '1 Therm / 1000 Gallon to Joule / milliliter', input: '1 Therm / 1000 Gallon to Joule / milliliter', expected: (1e5 * BTU) / (1000 * US_GAL * 1000) },
  { name: '1000 Watt * 1 hour to BTU', input: '1000 Watt * 1 hour to BTU', expected: 3.6e6 / BTU, unit: 'BTU' },
  { name: '1 HorsePower * 1 hour to BTU', input: '1 HorsePower * 1 hour to BTU', expected: (HP * 3600) / BTU, unit: 'BTU' },
  { name: '1 calorie / 1 second', input: '1 calorie / 1 second', expected: 4.184, unit: 'W' },
  { name: '1 BTU / 1 hour', input: '1 BTU / 1 hour', expected: BTU / 3600, unit: 'W' },
  { name: '1000 BTU / 1 minute to kilowatt', input: '1000 BTU / 1 minute to kilowatt', expected: (1000 * BTU) / 60 / 1000, unit: 'kilowatt' },
])

const IN = 0.0254
const MI = 1609.344
const YD = 0.9144
const ACRE = 4046.8564224
const PSI = 6894.757293168361
const FURLONG = 660 * FT
const ATM = 101325
const TORR = ATM / 760
const NMI = 1852

type Cross = {
  name: string
  expr: string
  si: number
  siUnit?: string
  alt?: number
  altUnit?: string
  /** When false, skip the no-target Quick Calc check (addition keeps the first operand's unit). */
  live?: boolean
}

function asTyped(expr: string): string {
  return expr.replace(/\bpi\b/gi, 'π')
}

function cross(title: string, cases: Cross[]) {
  const out: Case[] = []
  const live: Case[] = []
  for (const c of cases) {
    out.push({
      name: c.siUnit ? `${c.name} [${c.siUnit}]` : c.name,
      input: c.siUnit ? `${c.expr} to ${c.siUnit}` : c.expr,
      expected: c.si,
    })
    if (c.alt !== undefined && c.altUnit) {
      out.push({
        name: `${c.name} [${c.altUnit}]`,
        input: `${c.expr} to ${c.altUnit}`,
        expected: c.alt,
      })
    }
    if (c.live !== false) {
      live.push({ name: `${c.name} (Quick Calc)`, input: asTyped(c.expr), expected: c.si })
    }
  }
  compat(title, out)
  if (live.length) compat(`${title} (Quick Calc)`, live)
}

cross('Cross-unit: length × length → area', [
  { name: '1. 20 m * 2 in', expr: '20 m * 2 in', si: 20 * 2 * IN, siUnit: 'm^2', alt: (20 * 2) / IN, altUnit: 'in^2' },
  { name: '2. 10 ft * 5 m', expr: '10 ft * 5 m', si: 10 * FT * 5, siUnit: 'm^2', alt: (10 * 5) / FT, altUnit: 'ft^2' },
  { name: '3. 1 mile * 1 furlong', expr: '1 mile * 1 furlong', si: MI * FURLONG, siUnit: 'm^2', alt: 80, altUnit: 'acres' },
  { name: '4. 100 cm * 1 yd', expr: '100 cm * 1 yd', si: 1 * YD, siUnit: 'm^2', alt: YD / (FT * FT), altUnit: 'ft^2' },
  { name: '5. 2 km * 500 ft', expr: '2 km * 500 ft', si: 2000 * 500 * FT, siUnit: 'm^2', alt: (2000 * 500 * FT) / ACRE, altUnit: 'acres' },
  { name: '6. 1 km * 1 mm', expr: '1 km * 1 mm', si: 1, siUnit: 'm^2', alt: 10000, altUnit: 'cm^2' },
  { name: '7. 50 in * 200 mm', expr: '50 in * 200 mm', si: 50 * IN * 0.2, siUnit: 'm^2', alt: (50 * IN * 0.2) / (IN * IN), altUnit: 'in^2' },
])

cross('Cross-unit: area × length → volume', [
  { name: '8. 1 acre * 1 ft', expr: '1 acre * 1 ft', si: ACRE * FT, siUnit: 'm^3', alt: 43560, altUnit: 'ft^3' },
  { name: '9. 10 m^2 * 50 cm', expr: '10 m^2 * 50 cm', si: 5, siUnit: 'm^3', alt: 5000, altUnit: 'L' },
  { name: '10. 100 sq ft * 2 in', expr: '100 sq ft * 2 in', si: 100 * FT * FT * 2 * IN, siUnit: 'm^3', alt: 100 * (2 / 12), altUnit: 'ft^3' },
  { name: '11. 1 hectare * 10 mm', expr: '1 hectare * 10 mm', si: 100, siUnit: 'm^3', alt: 100000, altUnit: 'L' },
  { name: '12. 1 sq yd * 3 ft', expr: '1 sq yd * 3 ft', si: YD * YD * 3 * FT, siUnit: 'm^3', alt: 27, altUnit: 'ft^3' },
  { name: '13. 1 barn * 1e28 m', expr: '1 barn * 1e28 m', si: 1, siUnit: 'm^3', alt: 1000, altUnit: 'L' },
])

cross('Cross-unit: mass × acceleration → force', [
  { name: '14. 10 kg * 9.81 m/s^2', expr: '10 kg * 9.81 m/s^2', si: 98.1, siUnit: 'N', alt: 98.1, altUnit: 'kg * m / s^2' },
  { name: '15. 2 slug * 32.174 ft/s^2', expr: '2 slug * 32.174 ft/s^2', si: 2 * 32.174 * LBF, siUnit: 'N', alt: 2 * 32.174, altUnit: 'lbf' },
  { name: '16. 100 Lbm * 1 gravity', expr: '100 Lbm * 1 gravity', si: 100 * LBF, siUnit: 'N', alt: 100, altUnit: 'lbf' },
  { name: '17. 500 g * 2 m/s^2', expr: '500 g * 2 m/s^2', si: 1, siUnit: 'N', alt: 1e5, altUnit: 'dynes' },
  { name: '18. 1 metricTon * 1 m/s^2', expr: '1 metricTon * 1 m/s^2', si: 1000, siUnit: 'N', alt: 1, altUnit: 'kN' },
  { name: '19. 1 mg * 1 km/s^2', expr: '1 mg * 1 km/s^2', si: 0.001, siUnit: 'N', alt: 1, altUnit: 'mN' },
])

cross('Cross-unit: force × distance → energy', [
  { name: '20. 50 N * 2 m', expr: '50 N * 2 m', si: 100, siUnit: 'J', alt: 100, altUnit: 'N * m' },
  { name: '21. 10 lbf * 5 ft', expr: '10 lbf * 5 ft', si: 10 * LBF * 5 * FT, siUnit: 'J', alt: 50, altUnit: 'ft * lbf' },
  { name: '22. 1000 dyne * 50 cm', expr: '1000 dyne * 50 cm', si: 0.005, siUnit: 'J', alt: 50000, altUnit: 'ergs' },
  { name: '23. 1 kip * 10 ft', expr: '1 kip * 10 ft', si: 1000 * LBF * 10 * FT, siUnit: 'J', alt: 10000, altUnit: 'ft * lbf' },
  { name: '24. 2 kgf * 5 m', expr: '2 kgf * 5 m', si: 2 * G0 * 5, siUnit: 'J', alt: 2 * G0 * 5, altUnit: 'N * m' },
])

cross('Cross-unit: pressure × area → force', [
  { name: '25. 100 PSI * 2 in^2', expr: '100 PSI * 2 in^2', si: 200 * LBF, siUnit: 'N', alt: 200, altUnit: 'lbf' },
  { name: '26. 1 bar * 2 m^2', expr: '1 bar * 2 m^2', si: 200000, siUnit: 'N', alt: 200, altUnit: 'kN' },
  { name: '27. 1 atm * 1 m^2', expr: '1 atm * 1 m^2', si: ATM, siUnit: 'N', alt: ATM / LBF, altUnit: 'lbf' },
  { name: '28. 5 Pa * 100 cm^2', expr: '5 Pa * 100 cm^2', si: 0.05, siUnit: 'N', alt: 5000, altUnit: 'dynes' },
  { name: '29. 10 Torr * 1 sq ft', expr: '10 Torr * 1 sq ft', si: 10 * TORR * FT * FT, siUnit: 'N', alt: (10 * TORR * FT * FT) / LBF, altUnit: 'lbf' },
])

cross('Cross-unit: electrical products & quotients', [
  { name: '30. 120 V * 2 A', expr: '120 V * 2 A', si: 240, siUnit: 'W', alt: 240, altUnit: 'J / s' },
  { name: '31. 12 V * 500 mA', expr: '12 V * 500 mA', si: 6, siUnit: 'W', alt: 6, altUnit: 'J / s' },
  { name: '32. 240 V * 10 A', expr: '240 V * 10 A', si: 2400, siUnit: 'W', alt: 2.4, altUnit: 'kW' },
  { name: '33. 100 mV * 2 mA', expr: '100 mV * 2 mA', si: 0.0002, siUnit: 'W', alt: 0.2, altUnit: 'mW' },
  { name: '34. 10 C / 2 s', expr: '10 Coulomb / 2 s', si: 5, siUnit: 'A', alt: 5, altUnit: 'Coulomb / s' },
  { name: '35. 100 J / 5 C', expr: '100 J / 5 Coulomb', si: 20, siUnit: 'V', alt: 20, altUnit: 'J / Coulomb' },
  { name: '36. 12 V / 4 Ohm', expr: '12 V / 4 Ohm', si: 3, siUnit: 'A', alt: 3, altUnit: 'V / Ohm' },
  { name: '37. 12 V * 2 C', expr: '12 V * 2 Coulomb', si: 24, siUnit: 'J', alt: 24, altUnit: 'V * Coulomb' },
  { name: '38. 1 Farad * 12 V', expr: '1 Farad * 12 V', si: 12, siUnit: 'Coulomb', alt: 12, altUnit: 'A * s' },
  { name: '39. 1 uF * 10 kV', expr: '1 uF * 10 kV', si: 0.01, siUnit: 'Coulomb', alt: 10, altUnit: 'mC' },
])

cross('Cross-unit: power × time → energy', [
  { name: '40. 1 kW * 2 hr', expr: '1 kW * 2 hr', si: 7.2e6, siUnit: 'J', alt: 2, altUnit: 'kWh' },
  { name: '41. 100 W * 10 s', expr: '100 W * 10 s', si: 1000, siUnit: 'J', alt: 1, altUnit: 'kJ' },
  { name: '42. 1 hp * 1 hr', expr: '1 hp * 1 hr', si: HP * 3600, siUnit: 'J', alt: (HP * 3600) / BTU, altUnit: 'BTU' },
  { name: '43. 50 W * 1 day', expr: '50 W * 1 day', si: 50 * 86400, siUnit: 'J', alt: (50 * 86400) / 3.6e6, altUnit: 'kWh' },
  { name: '44. 1 BTU/hr * 10 hr', expr: '1 BTU/hr * 10 hr', si: 10 * BTU, siUnit: 'J', alt: 10, altUnit: 'BTU' },
  { name: '45. 10 kW * 5 ms', expr: '10 kW * 5 ms', si: 50, siUnit: 'J', alt: 0.05, altUnit: 'kJ' },
])

cross('Cross-unit: length / time → velocity & acceleration', [
  { name: '46. 100 miles / 2 hr', expr: '100 miles / 2 hr', si: 50 * (MI / 3600), siUnit: 'm/s', alt: 50, altUnit: 'mph' },
  { name: '47. 100 m / 9.58 s', expr: '100 m / 9.58 s', si: 100 / 9.58, siUnit: 'm/s', alt: (100 / 9.58) * 3.6, altUnit: 'km/h' },
  { name: '48. 1 NauticalMile / 1 hr', expr: '1 NauticalMile / 1 hr', si: NMI / 3600, siUnit: 'm/s', alt: 1, altUnit: 'knot' },
  { name: '49. 1 lightYear / 1 yr', expr: '1 lightYear / 1 yr', si: C, siUnit: 'm/s', alt: 1, altUnit: 'speedOfLight' },
  { name: '50. 10 km / 15 min', expr: '10 km / 15 min', si: 10000 / 900, siUnit: 'm/s', alt: 40, altUnit: 'km/h' },
  { name: '51. 60 mph / 5 s', expr: '60 mph / 5 s', si: 12 * (MI / 3600), siUnit: 'm/s^2', alt: 12, altUnit: 'mph / s' },
  { name: '52. 100 kph / 10 s', expr: '100 kph / 10 s', si: 10 * (1000 / 3600), siUnit: 'm/s^2', alt: 10, altUnit: 'kph / s' },
  { name: '53. 30 m/s / 3 s', expr: '30 m/s / 3 s', si: 10, siUnit: 'm/s^2', alt: 10 / G0, altUnit: 'gravity' },
  { name: '54. 20 knots / 10 s', expr: '20 knots / 10 s', si: 2 * (NMI / 3600), siUnit: 'm/s^2', alt: 2, altUnit: 'knots / s' },
])

cross('Cross-unit: mass / volume → density & volumetric flow', [
  { name: '55. 10 kg / 2 L', expr: '10 kg / 2 L', si: 5000, siUnit: 'kg / m^3', alt: 5, altUnit: 'kg / L' },
  { name: '56. 1 Lbm / 1 ft^3', expr: '1 Lbm / 1 ft^3', si: LB / (FT * FT * FT), siUnit: 'kg / m^3', alt: LB / (FT * FT * FT) / 1000, altUnit: 'g / cm^3' },
  { name: '57. 100 g / 50 cc', expr: '100 g / 50 cc', si: 2000, siUnit: 'kg / m^3', alt: 2, altUnit: 'g / cm^3' },
  { name: '58. 1 metricTon / 1 m^3', expr: '1 metricTon / 1 m^3', si: 1000, siUnit: 'kg / m^3', alt: 1, altUnit: 'g / cm^3' },
  { name: '59. 10 lb / 1 gal', expr: '10 lb / 1 gal', si: (10 * LB) / (US_GAL * 0.001), siUnit: 'kg / m^3', alt: (10 * LB) / US_GAL, altUnit: 'kg / L' },
  { name: '60. 10 gal / 1 min', expr: '10 gal / 1 min', si: (10 * US_GAL * 0.001) / 60, siUnit: 'm^3 / s', alt: (10 * US_GAL) / 60, altUnit: 'L / s' },
  { name: '61. 100 L / 1 hr', expr: '100 L / 1 hr', si: 0.1 / 3600, siUnit: 'm^3 / s', alt: 100 / 3600, altUnit: 'L / s' },
])

cross('Cross-unit: energy / time → power', [
  { name: '62. 1000 J / 10 s', expr: '1000 J / 10 s', si: 100, siUnit: 'W', alt: 100, altUnit: 'J / s' },
  { name: '63. 1 kWh / 1 hr', expr: '1 kWh / 1 hr', si: 1000, siUnit: 'W', alt: 1, altUnit: 'kW' },
  { name: '64. 1 Therm / 1 day', expr: '1 Therm / 1 day', si: (1e5 * BTU) / 86400, siUnit: 'W', alt: (1e5 * BTU) / 86400 / HP, altUnit: 'hp' },
  { name: '65. 1 cal / 1 s', expr: '1 cal / 1 s', si: 4.184, siUnit: 'W', alt: 4.184, altUnit: 'J / s' },
  { name: '66. 1 BTU / 1 min', expr: '1 BTU / 1 min', si: BTU / 60, siUnit: 'W', alt: BTU / 60 / HP, altUnit: 'hp' },
])

cross('Cross-unit: addition & subtraction of compatible dimensions', [
  { name: '67. 10 km + 500 m', expr: '10 km + 500 m', si: 10500, siUnit: 'm', alt: 10.5, altUnit: 'km', live: false },
  { name: '68. 1 ft + 2 in', expr: '1 ft + 2 in', si: FT + 2 * IN, siUnit: 'm', alt: 14, altUnit: 'in', live: false },
  { name: '69. 1 hr + 30 min', expr: '1 hr + 30 min', si: 5400, siUnit: 's', alt: 1.5, altUnit: 'hr', live: false },
  { name: '70. 1 kg + 500 g', expr: '1 kg + 500 g', si: 1.5, siUnit: 'kg', alt: 1500, altUnit: 'g', live: false },
  { name: '71. 1 lb + 8 oz', expr: '1 lb + 8 oz', si: 1.5 * LB, siUnit: 'kg', alt: 1.5, altUnit: 'lb', live: false },
  { name: '72. 1 gal + 2 qt', expr: '1 gal + 2 qt', si: 1.5 * US_GAL * 0.001, siUnit: 'm^3', alt: 1.5, altUnit: 'gal', live: false },
  { name: '73. 1 yard + 2 feet', expr: '1 yard + 2 feet', si: YD + 2 * FT, siUnit: 'm', alt: 5, altUnit: 'ft', live: false },
  { name: '74. 1 mile - 1000 m', expr: '1 mile - 1000 m', si: MI - 1000, siUnit: 'm', alt: (MI - 1000) / MI, altUnit: 'mi', live: false },
  { name: '75. 5 N + 1 lbf', expr: '5 N + 1 lbf', si: 5 + LBF, siUnit: 'N', alt: (5 + LBF) / LBF, altUnit: 'lbf', live: false },
  { name: '76. 1 atm + 5 PSI', expr: '1 atm + 5 PSI', si: ATM + 5 * PSI, siUnit: 'Pa', alt: (ATM + 5 * PSI) / ATM, altUnit: 'atm', live: false },
  { name: '77. 1 hp + 100 W', expr: '1 hp + 100 W', si: HP + 100, siUnit: 'W', alt: (HP + 100) / HP, altUnit: 'hp', live: false },
  { name: '78. 1 BTU + 500 J', expr: '1 BTU + 500 J', si: BTU + 500, siUnit: 'J', alt: (BTU + 500) / BTU, altUnit: 'BTU', live: false },
])

cross('Cross-unit: dimensionless ratios', [
  { name: '79. 1 mile / 1 km', expr: '1 mile / 1 km', si: 1.609344 },
  { name: '80. 1 hr / 60 s', expr: '1 hr / 60 s', si: 60 },
  { name: '81. 1 lb / 1 oz', expr: '1 lb / 1 oz', si: 16 },
  { name: '82. 1 gal / 1 qt', expr: '1 gal / 1 qt', si: 4 },
  { name: '83. 10 N / 2 lbf', expr: '10 N / 2 lbf', si: 10 / (2 * LBF) },
  { name: '84. 1 kWh / 1 MJ', expr: '1 kWh / 1 MJ', si: 3.6 },
  { name: '85. 1 hp / 1 W', expr: '1 hp / 1 W', si: HP },
  { name: '86. 1 atm / 1 Pa', expr: '1 atm / 1 Pa', si: ATM },
  { name: '87. 1 acre / 1 sq ft', expr: '1 acre / 1 sq ft', si: 43560 },
  { name: '88. 60 RPM / 1 Hz', expr: '60 RPM / 1 Hz', si: 1 },
])

cross('Cross-unit: rotational & frequency operations', [
  { name: '89. 360 deg / 1 s', expr: '360 deg / 1 s', si: 2 * Math.PI, siUnit: 'rad / s', alt: 60, altUnit: 'RPM' },
  { name: '90. 2 * pi rad / 1 s', expr: '2 * pi rad / 1 s', si: 2 * Math.PI, siUnit: 'rad / s', alt: 1, altUnit: 'Hz' },
  { name: '91. 120 RPM * 10 s', expr: '120 RPM * 10 s', si: 20 * 2 * Math.PI, siUnit: 'rad', alt: 20, altUnit: 'revolutions' },
  { name: '92. 1000 deg / 1 min', expr: '1000 deg / 1 min', si: ((1000 * Math.PI) / 180) / 60, siUnit: 'rad / s', alt: 1000 / 360, altUnit: 'RPM' },
])

cross('Cross-unit: multi-operator physics expressions', [
  { name: '93. 0.5 * 2 kg * (10 m/s)^2', expr: '0.5 * 2 kg * (10 m/s)^2', si: 100, siUnit: 'J', alt: 100, altUnit: 'kg * m^2 / s^2' },
  { name: '94. 0.5 * 1 slug * (60 ft/s)^2', expr: '0.5 * 1 slug * (60 ft/s)^2', si: 1800 * LBF * FT, siUnit: 'J', alt: 1800, altUnit: 'ft * lbf' },
  { name: '95. 10 kg * 9.81 m/s^2 * 5 m', expr: '10 kg * 9.81 m/s^2 * 5 m', si: 490.5, siUnit: 'J', alt: 490.5, altUnit: 'N * m' },
  { name: '96. 100 Lbm * 1 gravity * 10 ft', expr: '100 Lbm * 1 gravity * 10 ft', si: 1000 * LBF * FT, siUnit: 'J', alt: 1000, altUnit: 'ft * lbf' },
  { name: '97. (100 N * 5 m) / 2 s', expr: '(100 N * 5 m) / 2 s', si: 250, siUnit: 'W', alt: 250, altUnit: 'J / s' },
  { name: '98. (100 PSI * 10 in^2) * 2 ft', expr: '(100 PSI * 10 in^2) * 2 ft', si: 2000 * LBF * FT, siUnit: 'J', alt: 2000, altUnit: 'ft * lbf' },
  { name: '99. 1 kg * (1 speedOfLight)^2', expr: '1 kg * (1 speedOfLight)^2', si: C * C, siUnit: 'J', alt: C * C, altUnit: 'kg * m^2 / s^2' },
  { name: '100. 1 AMU * (1 speedOfLight)^2', expr: '1 AMU * (1 speedOfLight)^2', si: AMU * C * C, siUnit: 'J', alt: (AMU * C * C) / E / 1e6, altUnit: 'MeV' },
])

function grid(title: string, units: Array<[string, number]>, amounts: number[] = [1, 2, 3, 5, 8, 10]) {
  const cases: Case[] = []
  for (const amt of amounts) {
    for (const [a, sa] of units) {
      for (const [b, sb] of units) {
        cases.push({
          name: `${amt} ${a} to ${b}`,
          input: `${amt} ${a} to ${b}`,
          expected: (amt * sa) / sb,
        })
      }
    }
  }
  compat(title, cases)
}

function scaledCross(title: string, make: (n: number) => Cross, from = 1, to = 96) {
  const rows: Cross[] = []
  for (let n = from; n <= to; n++) rows.push(make(n))
  cross(title, rows)
}

grid(
  'Length & Distance',
  [
    ['in', IN],
    ['ft', FT],
    ['yd', YD],
    ['m', 1],
    ['km', 1000],
    ['mi', MI],
  ],
  [1, 2, 5, 10],
)
grid(
  'Length conversions & operations',
  [
    ['Inch', IN],
    ['Foot', FT],
    ['Yard', YD],
    ['meter', 1],
    ['Mile', MI],
    ['Furlong', FURLONG],
    ['NauticalMile', NMI],
  ],
  [1, 2, 4],
)
grid(
  'Mass & Weight',
  [
    ['oz', 0.028349523125],
    ['lb', LB],
    ['g', 0.001],
    ['kg', 1],
    ['tonne', 1000],
  ],
  [1, 2, 5, 10],
)
grid(
  'Mass conversions & operations',
  [
    ['OunceMass', 0.028349523125],
    ['PoundMass', LB],
    ['gram', 0.001],
    ['kilogram', 1],
    ['metricTon', 1000],
    ['Grain', 64.79891e-6],
  ],
  [1, 2, 5],
)

const tempCases: Case[] = []
for (const c of [-40, -20, -10, 0, 5, 10, 15, 20, 25, 30, 37, 40, 50, 80, 100, 150, 200, -273.15, 1000, -50, 12.5, 36.5]) {
  tempCases.push(
    { name: `${c} C to F`, input: `${c} celsius to fahrenheit`, expected: (c * 9) / 5 + 32 },
    { name: `${c} C to K`, input: `${c} celsius to kelvin`, expected: c + 273.15 },
    { name: `${c} C to R`, input: `${c} celsius to rankine`, expected: (c + 273.15) * (9 / 5) },
  )
}
for (const f of [-40, 0, 32, 50, 68, 72, 98.6, 100, 212, 451]) {
  tempCases.push(
    { name: `${f} F to C`, input: `${f} fahrenheit to celsius`, expected: ((f - 32) * 5) / 9 },
    { name: `${f} F to K`, input: `${f} fahrenheit to kelvin`, expected: ((f - 32) * 5) / 9 + 273.15 },
  )
}
for (const k of [0, 100, 255.37, 273.15, 293.15, 310.15, 373.15, 500]) {
  tempCases.push(
    { name: `${k} K to C`, input: `${k} kelvin to celsius`, expected: k - 273.15 },
    { name: `${k} K to F`, input: `${k} kelvin to fahrenheit`, expected: ((k - 273.15) * 9) / 5 + 32 },
  )
}
compat('Temperature', tempCases)

grid(
  'Volume & Capacity',
  [
    ['ml', 0.001],
    ['L', 1],
    ['gal', US_GAL],
    ['qt', 0.946352946],
    ['cup', 0.2365882365],
    ['floz', US_FLOZ],
  ],
  [1, 2, 4, 8],
)
grid(
  'Area',
  [
    ['m^2', 1],
    ['ft^2', FT * FT],
    ['acre', ACRE],
    ['hectare', 10000],
    ['in^2', IN * IN],
  ],
  [1, 2, 5, 10],
)
grid(
  'Area & volume',
  [
    ['liter', 0.001],
    ['Gallon', US_GAL * 0.001],
    ['cc', 1e-6],
    ['meter^3', 1],
    ['Foot^3', FT ** 3],
  ],
  [1, 2, 4, 8],
)
grid(
  'Speed & Velocity',
  [
    ['mph', MI / 3600],
    ['km/h', 1000 / 3600],
    ['m/s', 1],
    ['knot', NMI / 3600],
    ['ft/s', FT],
  ],
  [1, 2, 5, 10, 20, 60],
)
grid(
  'Velocity & acceleration',
  [
    ['MPH', MI / 3600],
    ['kph', 1000 / 3600],
    ['m/s', 1],
    ['Knot', NMI / 3600],
  ],
  [1, 5, 10, 20, 50],
)
grid(
  'Time',
  [
    ['s', 1],
    ['min', 60],
    ['hr', 3600],
    ['day', 86400],
    ['week', 604800],
  ],
  [1, 2, 3, 6, 12],
)
grid(
  'Time conversions & operations',
  [
    ['second', 1],
    ['minute', 60],
    ['hour', 3600],
    ['day', 86400],
    ['year', YEAR],
  ],
  [1, 2, 4, 10],
)
grid(
  'Digital Data Storage',
  [
    ['kilobytes', 1024],
    ['megabytes', 1024 ** 2],
    ['gigabytes', 1024 ** 3],
    ['terabytes', 1024 ** 4],
  ],
  [1, 2, 4, 8, 16, 32, 64],
)
grid(
  'Energy & Power',
  [
    ['J', 1],
    ['kJ', 1000],
    ['cal', 4.184],
    ['BTU', BTU],
    ['kWh', 3.6e6],
  ],
  [1, 2, 5, 10],
)
grid(
  'Energy & Power',
  [
    ['W', 1],
    ['kW', 1000],
    ['hp', HP],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Work, energy & power',
  [
    ['Joule', 1],
    ['calorie', 4.184],
    ['BTU', BTU],
    ['eV', E],
  ],
  [1, 2, 5, 10],
)
grid(
  'Work, energy & power',
  [
    ['Watt', 1],
    ['HorsePower', HP],
    ['kilowatt', 1000],
  ],
  [1, 2, 5, 10, 20],
)
grid(
  'Force & pressure',
  [
    ['N', 1],
    ['lbf', LBF],
    ['dyne', 1e-5],
    ['kgf', G0],
  ],
  [1, 2, 5, 10],
)
grid(
  'Force & pressure',
  [
    ['Pa', 1],
    ['bar', 1e5],
    ['atm', ATM],
    ['PSI', PSI],
    ['Torr', TORR],
  ],
  [1, 2, 5, 10],
)
grid(
  'Charge & electrical units',
  [
    ['amp', 1],
    ['milliamp', 0.001],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Charge & electrical units',
  [
    ['Volt', 1],
    ['kilovolt', 1000],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Charge & electrical units',
  [
    ['Ohm', 1],
    ['kiloohm', 1000],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Charge & electrical units',
  [
    ['Farad', 1],
    ['microfarad', 1e-6],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Charge & electrical units',
  [
    ['Henry', 1],
    ['millihenry', 0.001],
  ],
  [1, 2, 5, 10, 20, 50, 100],
)
grid(
  'Charge & electrical units',
  [
    ['Coulomb', 1],
    ['electron', E],
  ],
  [1, 2, 5, 10],
)
grid(
  'Angle, frequency & angular velocity',
  [
    ['deg', Math.PI / 180],
    ['rad', 1],
    ['rev', 2 * Math.PI],
  ],
  [1, 2, 6, 10, 60, 90, 180],
)
grid(
  'Angle, frequency & angular velocity',
  [
    ['Hz', 1],
    ['RPM', 1 / 60],
    ['kHz', 1000],
  ],
  [1, 2, 6, 10, 60],
)
grid(
  'SI prefixes',
  [
    ['millimeter', 0.001],
    ['centimeter', 0.01],
    ['kilometer', 1000],
    ['megameter', 1e6],
    ['nanometer', 1e-9],
    ['micrometer', 1e-6],
  ],
  [1, 2, 5, 10],
)

const autoCases: Case[] = []
for (const n of [1, 3, 4, 5, 6, 8, 10, 12, 15, 20, 24, 30, 36, 48, 50, 60, 72, 90, 100]) {
  autoCases.push(
    { name: `${n} in default mm`, input: `${n} in`, expected: n * 25.4 },
    { name: `${n} ft default m`, input: `${n} ft`, expected: n * FT },
    { name: `${n} lb default kg`, input: `${n} lb`, expected: n * LB },
    { name: `${n} m default ft`, input: `${n} m`, expected: n / FT },
    { name: `${n} kg default lb`, input: `${n} kg`, expected: n / LB },
    { name: `${n} mph default km/h`, input: `${n} mph`, expected: n * 1.609344 },
  )
}
compat('Automatic SI ↔ US conversion', autoCases)

const catalogExtra: Case[] = []
for (const n of [2, 5, 10]) {
  catalogExtra.push(
    { name: `${n} Fermi to m`, input: `${n} Fermi to m`, expected: n * 1e-15 },
    { name: `${n} angstrom to m`, input: `${n} angstrom to m`, expected: n * 1e-10 },
    { name: `${n} Yds to m`, input: `${n} Yds to m`, expected: n * YD },
    { name: `${n} Fathom to ft`, input: `${n} Fathom to ft`, expected: n * 6 },
    { name: `${n} Rod to ft`, input: `${n} Rod to ft`, expected: n * 16.5 },
    { name: `${n} Chain to ft`, input: `${n} Chain to ft`, expected: n * 66 },
    { name: `${n} Furlong to ft`, input: `${n} Furlong to ft`, expected: n * 660 },
    { name: `${n} League to mi`, input: `${n} League to mi`, expected: n * 3 },
    { name: `${n} NauticalMile to m`, input: `${n} NauticalMile to m`, expected: n * 1852 },
    { name: `${n} decade to years`, input: `${n} decade to years`, expected: n * 10 },
    { name: `${n} century to years`, input: `${n} century to years`, expected: n * 100 },
    { name: `${n} millenium to years`, input: `${n} millenium to years`, expected: n * 1000 },
    { name: `${n} revolution to deg`, input: `${n} revolution to deg`, expected: n * 360 },
    { name: `${n} RPM to Hz`, input: `${n} RPM to Hz`, expected: n / 60 },
    { name: `${n} Hertz to RPM`, input: `${n} Hertz to RPM`, expected: n * 60 },
    { name: `${n} lightSpeed to m/s`, input: `${n} lightSpeed to m/s`, expected: n * C },
    { name: `${n} gravity to m/s^2`, input: `${n} gravity to m/s^2`, expected: n * G0 },
    { name: `${n} dyne to N`, input: `${n} dyne to N`, expected: n * 1e-5 },
    { name: `${n} kgForce to N`, input: `${n} kgForce to N`, expected: n * G0 },
    { name: `${n} PoundForce to N`, input: `${n} PoundForce to N`, expected: n * G0 * LB },
    { name: `${n} kip to lbf`, input: `${n} kip to lbf`, expected: n * 1000 },
    { name: `${n} eV to J`, input: `${n} eV to J`, expected: n * E },
    { name: `${n} erg to J`, input: `${n} erg to J`, expected: n * 1e-7 },
    { name: `${n} Therm to BTU`, input: `${n} Therm to BTU`, expected: n * 1e5 },
    { name: `${n} HorsePower to W`, input: `${n} HorsePower to W`, expected: n * HP },
    { name: `${n} barn to m^2`, input: `${n} barn to m^2`, expected: n * 1e-28 },
    { name: `${n} Drop to mL`, input: `${n} Drop to mL`, expected: n * 0.05 },
    { name: `${n} Tbs to tsp`, input: `${n} Tbs to tsp`, expected: n * 3 },
    { name: `${n} Barrel to gal`, input: `${n} Barrel to gal`, expected: n * 42 },
    { name: `${n} Cord to ft3`, input: `${n} Cord to ft3`, expected: n * 128 },
    { name: `${n} electron to Coulomb`, input: `${n} electron to Coulomb`, expected: n * E },
    { name: `${n} Lbm to kg`, input: `${n} Lbm to kg`, expected: n * LB },
    { name: `${n} Slug to kg`, input: `${n} Slug to kg`, expected: n * SLUG },
    { name: `${n} ShortTon to lbs`, input: `${n} ShortTon to lbs`, expected: n * 2000 },
    { name: `${n} LongTon to lbs`, input: `${n} LongTon to lbs`, expected: n * 2240 },
    { name: `${n} AMU to kg`, input: `${n} AMU to kg`, expected: n * AMU },
    { name: `${n} parsec to au`, input: `${n} parsec to au`, expected: n * (648000 / Math.PI) },
    { name: `${n} lightYear to m`, input: `${n} lightYear to m`, expected: n * C * YEAR },
    { name: `${n} astronomicalUnit to m`, input: `${n} astronomicalUnit to m`, expected: n * AU },
    { name: `${n} Ozm to g`, input: `${n} Ozm to g`, expected: n * 28.349523125 },
    { name: `${n} Snail to slugs`, input: `${n} Snail to slugs`, expected: n * 12 },
    { name: `${n} Peck to bushels`, input: `${n} Peck to bushels`, expected: n / 4 },
    { name: `${n} amp to milliamp`, input: `${n} amp to milliamp`, expected: n * 1000 },
    { name: `${n} Volt to kilovolt`, input: `${n} Volt to kilovolt`, expected: n / 1000 },
    { name: `${n} Farad to millifarad`, input: `${n} Farad to millifarad`, expected: n * 1000 },
    { name: `${n} Henry to millihenry`, input: `${n} Henry to millihenry`, expected: n * 1000 },
    { name: `${n} Dimensionless to units`, input: `${n} Dimensionless to units`, expected: n },
    { name: `${n} arcSecond to deg`, input: `${n} arcSecond to deg`, expected: n / 3600 },
    { name: `${n} arcMinute to deg`, input: `${n} arcMinute to deg`, expected: n / 60 },
    { name: `${n} km/hr to m/s`, input: `${n} km/hr to m/s`, expected: n / 3.6 },
  )
}
compat('Requested catalog aliases', catalogExtra)

grid(
  'Kinematics & mechanics',
  [
    ['N', 1],
    ['lbf', LBF],
    ['dyne', 1e-5],
    ['kgf', G0],
  ],
  [1, 2, 5, 10, 20],
)
grid(
  'Kinematics & mechanics',
  [
    ['J', 1],
    ['kJ', 1000],
    ['erg', 1e-7],
  ],
  [1, 2, 5, 10, 20],
)
grid(
  'Pressure, stress & fluid dynamics',
  [
    ['Pa', 1],
    ['bar', 1e5],
    ['PSI', PSI],
    ['atm', ATM],
    ['Torr', TORR],
  ],
  [1, 2, 5, 10],
)
grid(
  'Electromagnetism',
  [
    ['amp', 1],
    ['milliamp', 0.001],
  ],
  [1, 2, 5, 10, 20, 50],
)
grid(
  'Electromagnetism',
  [
    ['Volt', 1],
    ['kilovolt', 1000],
  ],
  [1, 2, 5, 10, 20, 50],
)
grid(
  'Electromagnetism',
  [
    ['Ohm', 1],
    ['kiloohm', 1000],
  ],
  [1, 2, 5, 10, 20, 50],
)
grid(
  'Electromagnetism',
  [
    ['Watt', 1],
    ['kilowatt', 1000],
  ],
  [1, 2, 5, 10, 20, 50],
)
grid(
  'Quantum, relativistic & atomic physics',
  [
    ['eV', E],
    ['Joule', 1],
  ],
  [1, 2, 5, 10, 100, 1000, 1e6],
)
grid(
  'Quantum, relativistic & atomic physics',
  [
    ['AMU', AMU],
    ['kg', 1],
    ['electronRestMass', 9.1093837015e-31],
  ],
  [1, 2, 5, 10, 20],
)
grid(
  'Quantum, relativistic & atomic physics',
  [
    ['electron', E],
    ['Coulomb', 1],
  ],
  [1, 2, 5, 10, 100, 1000],
)
grid(
  'Angular dynamics',
  [
    ['RPM', 1 / 60],
    ['Hz', 1],
  ],
  [1, 2, 10, 60, 120, 360, 1000],
)
grid(
  'Angular dynamics',
  [
    ['rad', 1],
    ['deg', Math.PI / 180],
    ['rev', 2 * Math.PI],
  ],
  [1, 2, 10, 60, 90, 180, 360],
)
grid(
  'Ideal gas law & thermodynamics',
  [
    ['Joule', 1],
    ['BTU', BTU],
    ['calorie', 4.184],
    ['kWh', 3.6e6],
  ],
  [1, 2, 5, 10],
)
grid(
  'Ideal gas law & thermodynamics',
  [
    ['Watt', 1],
    ['kilowatt', 1000],
    ['HorsePower', HP],
  ],
  [1, 2, 5, 10, 20],
)
grid(
  'Ideal gas law & thermodynamics',
  [
    ['atmosphere', ATM],
    ['Pa', 1],
    ['bar', 1e5],
  ],
  [1, 2, 5, 10],
)
grid(
  'Dimensionless, unit prefix scaling & aliases',
  [
    ['milli', 1e-3],
    ['centi', 1e-2],
    ['kilo', 1e3],
    ['mega', 1e6],
    ['micro', 1e-6],
    ['nano', 1e-9],
  ],
  [1, 2, 5, 10],
)

describe('Every requested unit alias', () => {
  it.each(REQUESTED_ALIASES)('2 %s to self', (alias) => {
    expect(evaluateLine(`2 ${alias} to ${alias}`).value?.n, alias).toBeCloseTo(2, 8)
  })
  it.each(REQUESTED_ALIASES)('5 %s to self', (alias) => {
    expect(evaluateLine(`5 ${alias} to ${alias}`).value?.n, alias).toBeCloseTo(5, 8)
  })
  it.each(REQUESTED_ALIASES)('10 %s / 2 %s', (alias) => {
    expect(evaluateLine(`10 ${alias} / 2 ${alias}`).value?.n, alias).toBeCloseTo(5, 6)
  })
  it.each(REQUESTED_ALIASES)('0 %s to self', (alias) => {
    expect(evaluateLine(`0 ${alias} to ${alias}`).value?.n, alias).toBeCloseTo(0, 10)
  })
})

describe('Every requested SI prefix', () => {
  it.each(PREFIX_NAMES)('accepts %s second', (prefix) => {
    const r = evaluateLine(`1 ${prefix} second to s`)
    expect(Number.isFinite(r.value!.n), prefix).toBe(true)
  })
  it.each(PREFIX_NAMES)('accepts %s gram', (prefix) => {
    const r = evaluateLine(`1 ${prefix} gram to g`)
    expect(Number.isFinite(r.value!.n), prefix).toBe(true)
  })
  it.each(PREFIX_NAMES)('accepts %s watt', (prefix) => {
    const r = evaluateLine(`1 ${prefix} watt to W`)
    expect(Number.isFinite(r.value!.n), prefix).toBe(true)
  })
  it.each(PREFIX_NAMES)('accepts %s newton', (prefix) => {
    const r = evaluateLine(`1 ${prefix} newton to N`)
    expect(Number.isFinite(r.value!.n), prefix).toBe(true)
  })
})

describe('Workspace default units', () => {
  const keep: Array<[string, string, Record<string, string>]> = []
  for (const n of [1, 2, 3, 5, 8, 10, 12, 15, 20, 25, 40, 50]) {
    keep.push(
      [`${n} in stays in`, `${n} in`, { length: 'in' }],
      [`${n} ft stays ft`, `${n} ft`, { length: 'ft' }],
      [`${n} m stays m`, `${n} m`, { length: 'm' }],
      [`${n} kg stays kg`, `${n} kg`, { mass: 'kg' }],
      [`${n} lb stays lb`, `${n} lb`, { mass: 'lb' }],
      [`${n} s stays s`, `${n} s`, { time: 's' }],
      [`${n} hr stays hr`, `${n} hr`, { time: 'hr' }],
      [`${n} deg stays deg`, `${n} deg`, { angle: 'deg' }],
    )
  }
  it.each(keep)('%s', (_name, expr, defaults) => {
    const n = Number(expr.split(' ')[0])
    const r = evaluateLine(expr, { defaultUnits: defaults })
    expect(r.value?.n).toBeCloseTo(n, 8)
  })
  it('converts mm into default inches for several values', () => {
    for (const n of [25.4, 50.8, 76.2, 127]) {
      const r = evaluateLine(`${n} mm`, { defaultUnits: { length: 'in' } })
      expect(r.display).toMatch(/in$/)
      expect(r.value?.n).toBeCloseTo(n / 25.4, 6)
    }
  })
})

describe('Unit conversion phrases', () => {
  const ok: Array<[string, number]> = [
    ['5 in to cm', 12.7],
    ['5 inches to cm', 12.7],
    ['5 in in cm', 12.7],
    ['12 in to ft', 1],
    ['3 ft to yd', 1],
    ['1760 yd to mi', 1],
    ['1000 m to km', 1],
    ['2.54 cm to in', 1],
    ['16 oz to lb', 1],
    ['1000 g to kg', 1],
    ['60 s to min', 1],
    ['60 min to hr', 1],
    ['24 hr to day', 1],
    ['7 day to week', 1],
    ['1000 mL to L', 1],
    ['4 qt to gal', 1],
    ['8 floz to cup', 1],
    ['2 cup to pt', 1],
    ['2 pt to qt', 1],
    ['180 deg to rad', Math.PI],
    ['1 rad to deg', 180 / Math.PI],
    ['60 RPM to Hz', 1],
    ['1 hp to W', HP],
    ['1000 W to kW', 1],
    ['1 atm to Pa', ATM],
    ['14.696 psi to atm', (14.696 * PSI) / ATM],
    ['1 kWh to J', 3.6e6],
    ['1 cal to J', 4.184],
    ['10 miles to kilometers', 16.09344],
    ['10 kilometers to miles', 10 / 1.609344],
    ['6 feet to meters', 6 * FT],
    ['100 yards to meters', 100 * YD],
    ['2 in to mm', 50.8],
    ['2 in to m', 0.0508],
    ['3 kg to g', 3000],
    ['500 g to kg', 0.5],
    ['2 L to mL', 2000],
    ['1 m^2 to cm^2', 10000],
    ['1 acre to ft^2', 43560],
    ['1 hectare to m^2', 10000],
    ['60 mph to km/h', 96.56064],
    ['100 km/h to m/s', 100 / 3.6],
    ['1 N to dynes', 1e5],
    ['1 bar to Pa', 1e5],
    ['1 kPa to Pa', 1000],
    ['1 MJ to J', 1e6],
    ['1 kW to W', 1000],
    ['1 mA to A', 0.001],
    ['1 kV to V', 1000],
    ['1 microfarad to Farad', 1e-6],
    ['1 nanofarad to Farad', 1e-9],
    ['1 mH to H', 0.001],
    ['1 kOhm to Ohm', 1000],
    ['1 GHz to Hz', 1e9],
    ['1 ms to s', 0.001],
    ['1 us to s', 1e-6],
    ['1 ns to s', 1e-9],
    ['1 mg to g', 0.001],
    ['1 ug to g', 1e-6],
    ['1 mm to m', 0.001],
    ['1 cm to m', 0.01],
    ['1 nm to m', 1e-9],
    ['1 um to m', 1e-6],
    ['1 nmi to m', 1852],
    ['1 ly to m', C * YEAR],
    ['1 au to m', AU],
    ['1 millihenry to Henry', 0.001],
    ['2 kilovolt to Volt', 2000],
  ]
  it.each(ok)('%s', (input, expected) => {
    const r = evaluateLine(input)
    expect(r.value?.n, input).toBeCloseTo(expected, 6)
  })
  const bad = [
    '1 kg to s',
    '2 L to W',
    '3 m to Pa',
    '4 J to m',
    '5 A to kg',
    '6 V to m/s',
    '7 Ohm to liter',
    '8 Farad to acre',
    '9 Henry to day',
    '1 coulomb to meter',
    '2 watts to grams',
    '3 psi to seconds',
    '4 hp to inches',
    '5 K to meters',
    '6 rad to kg',
    '7 Hz to liters',
    '8 N to seconds',
    '9 Pa to amps',
    '1 m^2 to kg',
    '2 ft^3 to W',
    '3 mph to J',
    '4 acre to volt',
    '5 gal to N',
    '6 week to meter',
    '7 tonne to liter',
    '8 eV to amp',
    '9 barn to second',
    '1 drop to watt',
    '2 slug to volt',
    '3 furlong to kg',
  ]
  it.each(bad)('rejects %s', (input) => {
    expect(() => evaluateLine(input), input).not.toThrow()
    const r = evaluateLine(input)
    expect(r.display, input).toBe('improper unit conversion')
  })
})

describe('Requested unit conversion examples', () => {
  const rows: Array<[string, number, string]> = []
  for (const n of [1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 20, 24, 25, 30, 36, 40, 48, 50, 60, 72, 90, 100, 120, 144, 200, 250, 300, 360, 500, 1000]) {
    rows.push([`${n} in`, n * 25.4, 'mm'], [`${n} in to m`, n * 0.0254, 'm'], [`${n} ft to m`, n * FT, 'm'])
  }
  it.each(rows)('%s', (input, expected, unit) => {
    const r = evaluateLine(input)
    expect(r.value?.n, input).toBeCloseTo(expected, 6)
    expect(r.display, input).toMatch(new RegExp(`${unit}$`))
  })
  for (const n of [1, 2, 5, 10, 20]) {
    it(`${n} lb defaults to kg`, () => {
      const r = evaluateLine(`${n} lb`)
      expect(r.value?.n).toBeCloseTo(n * LB, 8)
      expect(r.display).toMatch(/kg$/)
    })
    it(`${n} kg defaults to lbs`, () => {
      const r = evaluateLine(`${n} kg`)
      expect(r.value?.n).toBeCloseTo(n / LB, 8)
    })
  }
})

scaledCross('Cross-unit: length × length → area', (n) => ({
  name: `extra ${n} m * 2 in`,
  expr: `${n} m * 2 in`,
  si: n * 2 * IN,
  siUnit: 'm^2',
}))
scaledCross('Cross-unit: area × length → volume', (n) => ({
  name: `extra ${n} m^2 * 2 cm`,
  expr: `${n} m^2 * 2 cm`,
  si: n * 0.02,
  siUnit: 'm^3',
}))
scaledCross('Cross-unit: mass × acceleration → force', (n) => ({
  name: `extra ${n} kg * 2 m/s^2`,
  expr: `${n} kg * 2 m/s^2`,
  si: n * 2,
  siUnit: 'N',
}))
scaledCross('Cross-unit: force × distance → energy', (n) => ({
  name: `extra ${n} N * 2 m`,
  expr: `${n} N * 2 m`,
  si: n * 2,
  siUnit: 'J',
}))
scaledCross('Cross-unit: pressure × area → force', (n) => ({
  name: `extra ${n} Pa * 2 m^2`,
  expr: `${n} Pa * 2 m^2`,
  si: n * 2,
  siUnit: 'N',
}))
scaledCross('Cross-unit: electrical products & quotients', (n) => ({
  name: `extra ${n} V * 2 A`,
  expr: `${n} V * 2 A`,
  si: n * 2,
  siUnit: 'W',
}))
scaledCross('Cross-unit: power × time → energy', (n) => ({
  name: `extra ${n} W * 2 s`,
  expr: `${n} W * 2 s`,
  si: n * 2,
  siUnit: 'J',
}))
scaledCross('Cross-unit: length / time → velocity & acceleration', (n) => ({
  name: `extra ${n} m / 2 s`,
  expr: `${n} m / 2 s`,
  si: n / 2,
  siUnit: 'm/s',
}))
scaledCross('Cross-unit: mass / volume → density & volumetric flow', (n) => ({
  name: `extra ${n} kg / 2 L`,
  expr: `${n} kg / 2 L`,
  si: (n / 0.002),
  siUnit: 'kg / m^3',
}))
scaledCross('Cross-unit: energy / time → power', (n) => ({
  name: `extra ${n} J / 2 s`,
  expr: `${n} J / 2 s`,
  si: n / 2,
  siUnit: 'W',
}))
scaledCross('Cross-unit: dimensionless ratios', (n) => ({
  name: `extra ${n} m / 1 m`,
  expr: `${n} m / 1 m`,
  si: n,
}))
scaledCross('Cross-unit: rotational & frequency operations', (n) => ({
  name: `extra ${n} deg / 1 s`,
  expr: `${n} deg / 1 s`,
  si: (n * Math.PI) / 180,
  siUnit: 'rad / s',
}))
scaledCross('Cross-unit: multi-operator physics expressions', (n) => ({
  name: `extra 0.5 * ${n} kg * (2 m/s)^2`,
  expr: `0.5 * ${n} kg * (2 m/s)^2`,
  si: 0.5 * n * 4,
  siUnit: 'J',
}))
scaledCross(
  'Cross-unit: addition & subtraction of compatible dimensions',
  (n) => ({
    name: `extra ${n} m + 2 m`,
    expr: `${n} m + 2 m`,
    si: n + 2,
    siUnit: 'm',
    live: false,
  }),
  1,
  96,
)
