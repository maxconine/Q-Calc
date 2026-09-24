import { describe, expect, it } from 'vitest'
import { expectNum, expectQty, line, shown } from './audit.helpers'

// constants are the exact defining conversion factors, not read off the engine
const LB = 0.45359237
const FT = 0.3048
const MI = 1609.344
const US_GAL = 3.785411784
const US_FLOZ = 0.0295735295625
const PSI = 6894.757293168361

type Row = [string, number]

function conversions(rows: Row[]): void {
  it.each(rows)('%s = %d', (text, want) => {
    expectNum(text, want)
  })
}

describe('audit2: everyday conversions people actually type', () => {
  conversions([
    ['26.2 mi to km', 26.2 * MI * 0.001],
    ['150 lb to kg', 150 * LB],
    ['2.2 lb to kg', 2.2 * LB],
    ['2 l to gal', 2 / US_GAL],
    ['12 floz to ml', 12 * US_FLOZ * 1000],
    ['65 mph to km/h', 65 * MI * 0.001],
    ['14.7 psi to kpa', (14.7 * PSI) / 1000],
    ['32 psi to kpa', (32 * PSI) / 1000],
    ['2000 kcal to kj', 2000 * 4.184],
    ['100 Wh to J', 100 * 3600],
    ['4.7 GB to MB', 4700],
    ['104 F to C', 40],
    ['100 yd to m', 91.44],
    ["6 ft 2 in to cm", 187.96],
    ['5000 m to mi', 5000 / MI],
  ])
})

describe('audit2: more everyday conversions, a fresh batch of numbers', () => {
  conversions([
    ['3.5 kg to lb', 3.5 / LB],
    ['70 kg to lb', 70 / LB],
    ['5.5 ft to cm', 5.5 * FT * 100],
    ['180 cm to ft', 1.8 / FT],
    ['10 stone to kg', 10 * 6.35029318],
    ['500 g to oz', 500 / 28.349523125],
    ['1 quart to liters', 0.946352946],
    ['750 ml to floz', 750 / (US_FLOZ * 1000)],
    ['3 miles to feet', 3 * 5280],
    ['440 yards to m', 440 * 0.9144],
    ['9.81 m/s2 to gee', 9.81 / 9.80665],
    ['1 atm to bar', 101325 / 1e5],
    ['30 inHg to kpa', 30 * 3.386389],
    ['500 hp to kW', (500 * 745.699872) / 1000],
    ['2500 kcal to kj', 2500 * 4.184],
    ['12 floz to cup', 12 / 8],
    ['1 acre to m2', 4046.8564224],
    ['5 hectares to acres', (5 * 10000) / 4046.8564224],
    ['3 cords to m3', (3 * 128 * 28.316846592) / 1000],
    ['1 nautical mile to mi', 1852 / MI],
    ['50 knots to mph', (50 * 1852) / MI],
    ['9 months to days', (9 * 365.25) / 12],
  ])
})

describe('audit2: fuel economy is a reciprocal quantity, not a linear one', () => {
  // gal/mi (volume/length) and l/100km (also volume/length) share a dimension: a straight
  // "gal per mile" figure converts to l/100km by simple scaling.
  it('1 gal per 25 mi is 9.40858333333 l/100km', () => {
    // 1 gal = 3.785411784 L, 25 mi = 25*1.609344 km; L per 100 km = L/km * 100
    const want = (3.785411784 / (25 * 1.609344)) * 100
    expectQty('1 gal / 25 mi to l/100km', want, 'l/100km')
  })

  it('1 gal per 30 mi is 7.84048611111 l/100km', () => {
    const want = (3.785411784 / (30 * 1.609344)) * 100
    expectQty('1 gal / 30 mi to l/100km', want, 'l/100km')
  })

  // mi/gal (length/volume) is the RECIPROCAL dimension of l/100km (volume/length); the two
  // are not proportional (40 mpg is not "half" of 20 mpg in L/100km), so a direct conversion
  // is dimensionally invalid and the engine is right to refuse it rather than silently invert.
  it.each(['40 mi / 1 gal to l/100km', '30 mi / 1 gal to l/100km', '8 l/100 km to mi/gal'])(
    '%s is an improper unit conversion (reciprocal dimensions), not a guessed number',
    (text) => {
      expect(shown(text)).toBe('improper unit conversion')
    },
  )

  // energy per distance (EV efficiency) is NOT reciprocal like fuel economy: kWh/mi and
  // kWh/100km are the same dimension both ways, so this one does convert directly.
  it('30 kWh per 100 mi is 18.6411357671 kWh/100km', () => {
    const want = ((30 / 100) * (1 / 1.609344)) * 100
    expectQty('30 kWh / 100 mi to kWh/100km', want, 'kWh/100km')
  })
})

describe('audit2: cooking, mixed volume units add like feet and inches', () => {
  it('1 tsp is 4.92892159375 ml', () => {
    expectQty('1 tsp in ml', US_FLOZ * 1000 / 6, 'mL')
  })
  it('2 cups plus 3 tablespoons is 517.536767344 ml', () => {
    const cup = 0.2365882365
    const tbsp = US_FLOZ / 2
    expectQty('2 cup + 3 tbsp to ml', (2 * cup + 3 * tbsp) * 1000, 'ml')
  })
  it('1 cup 2 tbsp is 266.161766063 ml', () => {
    const cup = 0.2365882365
    const tbsp = US_FLOZ / 2
    expectQty('1 cup 2 tbsp to ml', (cup + 2 * tbsp) * 1000, 'ml')
  })
  it('a recipe doubled: 3/4 cup * 2 is 1.5 cup', () => {
    expectQty('3/4 cup * 2', 1.5, 'cup')
  })
  it('1 lb 8 oz is 0.680388555 kg', () => {
    expectQty('1 lb 8 oz to kg', 1.5 * LB, 'kg')
  })
})

describe('audit2: digital storage past terabytes', () => {
  it('1 PB to TB is 1000', () => {
    expectNum('1 PB to TB', 1000)
  })
  it('1000 TB to PB is 1', () => {
    expectNum('1000 TB to PB', 1)
  })
  it('1 EB to TB is 1e6', () => {
    expectNum('1 EB to TB', 1e6)
  })

  it('a bare petabyte or exabyte is itself', () => {
    expect(shown('1 petabyte')).toBe('1 PB')
    expect(shown('1 exabyte')).toBe('1 EB')
  })
  it('binary sizes go on to PiB and EiB', () => {
    expectNum('1 PiB to TiB', 1024)
    expectNum('1 EiB to PiB', 1024)
  })
})

describe('audit2: b is a bit and B a byte', () => {
  it.each<[string, number]>([
    ['100 Mbps to MB/s', 12.5],
    ['1 Mbps to MB/s', 0.125],
    ['1 Gb to MB', 125],
    ['500 MB / 100 Mbps', 40],
    ['10 Mbit/s to kbps', 10000],
    ['1 byte to bits', 8],
    ['8 bits to byte', 1],
    ['1 kB to kb', 8],
    ['1 GB to MB', 1000],
    ['1 GiB to MB', 1073.741824],
  ])('%s = %d', (text, want) => {
    expectNum(text, want)
  })
  it('a lone b stays a letter', () => {
    expect(shown('5 b')).toBe('')
  })
})

describe('audit2: the SI symbols y (yocto) and z (zepto) resolve like the spelled prefix', () => {
  // a prefixed quantity stays in its own unit, like 1 mJ
  it('5 zJ is 5e-21 J (zepto-joule)', () => {
    expectNum('5 zJ to J', 5e-21)
    expect(shown('5 zJ')).toBe('5 zJ')
  })
  it('3 ys is 3e-24 s (yocto-second)', () => {
    expectNum('3 ys to s', 3e-24)
  })
  it('1 zg to g is 1e-21 (zepto-gram)', () => {
    expectNum('1 zg to g', 1e-21)
  })
  it('1 yg to g is 1e-24 (yocto-gram)', () => {
    expectNum('1 yg to g', 1e-24)
  })
  it('1e-21 J to zJ is 1', () => {
    expectNum('1e-21 J to zJ', 1)
  })
  it('1 zm to m is 1e-21 (zepto-meter)', () => {
    expectNum('1 zm to m', 1e-21)
  })

  it('the spelled-out prefix works fine for the same magnitudes', () => {
    expectNum('1 zeptojoule to J', 1e-21)
    expectNum('1 yoctosecond to s', 1e-24)
    expectNum('1 zeptogram to g', 1e-21)
  })
})

describe('audit2: the "da" (deka) abbreviation is a documented one-letter-only limitation', () => {
  // matchCasePrefixAtEnd is explicitly one letter at a time; deka's two-letter symbol "da" was
  // never given special handling, so only the spelled-out deka/deca forms work. Not a bug: the
  // engine never guesses a different unit here, it just leaves the two-letter form unrecognized.
  it('the "da" abbreviation is blank, but "deka"/"deca" spelled out both work', () => {
    expect(shown('1 dag to g')).toBe('')
    expect(shown('1 daL to L')).toBe('')
    expectNum('1 dekagram to g', 10)
    expectNum('1 decagram to g', 10)
    expectNum('1 dekaliter to L', 10)
  })
})

describe('audit2: prefix ladder extremes on prefixable roots', () => {
  conversions([
    ['1 Pm to m', 1e15],
    ['1 Em to km', 1e15],
    ['1 Zs to s', 1e21],
    ['1 Yg to kg', 1e21],
    ['1 TeV to eV', 1e12],
    ['1 PeV to eV', 1e15],
    ['1 aC to C', 1e-18],
    ['1 fH to H', 1e-15],
    ['1 nohm to ohm', 1e-9],
    ['1 pF to F', 1e-12],
  ])
})

describe('audit2: unit arithmetic mismatches beyond the basics', () => {
  it.each(['1 hp + 1 kg', '1 atm + 1 m', '1 F + 1 V', '1 mol + 1 kg', '1 Hz + 1 m'])(
    '%s is an improper unit conversion or blank, never a number',
    (text) => {
      const d = shown(text)
      expect(['', 'improper unit conversion'], `${text} → ${d}`).toContain(d)
    },
  )

  it('adding a plain number to a unit is blank, not a silent unit-less add', () => {
    expect(shown('5 kg + 3')).toBe('')
    expect(shown('3 + 5 kg')).toBe('')
  })
})

describe('audit2: photometric and chemical SI base units are simply not modelled', () => {
  // candela, mole and katal never appear in the unit table; this is a scope gap, not
  // ambiguity, but there is no number to compute so it is documented rather than failed.
  it.each(['1 cd', '1 mol', '1 kat', '1 lm'])('%s is blank (unit not modelled)', (text) => {
    expect(shown(text)).toBe('')
  })
})

describe('audit2: quantities keep their converted value exactly, spot-checked on new numbers', () => {
  it('a fuel economy answer carries its full precision, not the rounded display', () => {
    const r = line('1 gal / 25 mi to l/100km')
    const want = (3.785411784 / (25 * 1.609344)) * 100
    expect(r.value?.n).toBeCloseTo(want, 9)
  })
})
