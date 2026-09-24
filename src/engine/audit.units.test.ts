import { describe, expect, it } from 'vitest'
import { expectBlankOr, expectNum, expectQty, line, shown } from './audit.helpers'

// conversion factors come from the exact definitions (1 in = 2.54 cm, 1 lb = 0.45359237 kg, ...)
const FT = 0.3048
const MI = 1609.344
const LB = 0.45359237
const PSI = (LB * 9.80665) / 0.0254 ** 2
const YEAR_DAYS = 365.25

type Row = [string, number]

function conversions(rows: Row[]) {
  it.each(rows)('%s = %d', (text, want) => {
    expectNum(text, want)
  })
}

describe('audit: length', () => {
  conversions([
    ['1 mi to km', 1.609344],
    ['1 in to cm', 2.54],
    ['1 ft to in', 12],
    ['1 yd to m', 0.9144],
    ['1 nmi to m', 1852],
    ['1 km to mi', 1000 / MI],
    ['100 m to ft', 100 / FT],
    ['1 mile to feet', 5280],
    ['1 ly to km', (299792458 * YEAR_DAYS * 86400) / 1000],
    ['1 au to km', 149597870.7],
    ['1 pc to ly', (149597870700 * 648000) / Math.PI / (299792458 * YEAR_DAYS * 86400)],
    ['1 angstrom to nm', 0.1],
    ['1 mil to mm', 0.0254],
    ['1 furlong to m', 201.168],
    ['1 league to mi', 3],
    ['1 fathom to ft', 6],
    ['1 chain to ft', 66],
    ['1 rod to ft', 16.5],
    ['5 ft 10 in to cm', 177.8],
    ["5'10\" to cm", 177.8],
    ['6 ft in m', 6 * FT],
    ['12 in in cm', 30.48],
    ['3/8 in to mm', 9.525],
    ['1 m + 1 ft to ft', 1 / FT + 1],
    ['2 km + 500 m to m', 2500],
    ['1 km - 1 mi to m', 1000 - MI],
    ['10 km / 4 to m', 2500],
    ['1 micron to nm', 1000],
    ['1 fm to m', 1e-15],
  ])

  it('a bare length converts to its counterpart', () => {
    expectQty('1 in', 25.4, 'mm')
    expectQty('1 mi', 1.609344, 'km')
    expectQty('1 km', 1000 / MI, 'mi')
  })
})

describe('audit: mass', () => {
  conversions([
    ['1 lb to kg', LB],
    ['1 kg to lb', 1 / LB],
    ['1 oz to g', 28.349523125],
    ['1 lb to oz', 16],
    ['1 st to lb', 14],
    ['1 ton to lb', 2000],
    ['1 ton to kg', 2000 * LB],
    ['1 long ton to lb', 2240],
    ['1 tonne to kg', 1000],
    ['1 t to lb', 1000 / LB],
    ['1 grain to mg', 64.79891],
    ['7000 grains to lb', 1],
    ['1 slug to kg', (9.80665 * LB) / FT],
    ['1 amu to kg', 1.6605390666e-27],
    ['1000 mg to g', 1],
    ['1 ug to mg', 0.001],
    ['1/2 lb to oz', 8],
    ['1 lb + 1 kg to kg', 1 + LB],
    ['2 kg * 3 to g', 6000],
  ])
})

describe('audit: temperature', () => {
  conversions([
    ['100 C to F', 212],
    ['32 F to C', 0],
    ['-40 C to F', -40],
    ['-40 F to C', -40],
    ['0 C to F', 32],
    ['0 K to C', -273.15],
    ['0 C to K', 273.15],
    ['100 C to K', 373.15],
    ['-273.15 C to K', 0],
    ['300 K to F', 80.33],
    ['98.6 F to C', 37],
    ['37 C to F', 98.6],
    ['451 F to C', (451 - 32) * (5 / 9)],
    ['0 F to K', (-32 * 5) / 9 + 273.15],
    ['0 K to F', -459.67],
    ['-459.67 F to K', 0],
    ['491.67 R to F', 32],
    ['1 R to K', 5 / 9],
    ['100 °C to °F', 212],
    ['212 °F in °C', 100],
    ['20 degC to degF', 68],
    ['100 celsius to fahrenheit', 212],
    ['37 celsius to fahrenheit', 98.6],
    ['300 kelvin to celsius', 26.85],
  ])

  it('a bare temperature converts to the other scale', () => {
    expectQty('72 F', ((72 - 32) * 5) / 9, '°C')
    expectQty('100 C', 212, '°F')
    expectQty('20 °C', 68, '°F')
  })

  it('offset scales never take part in arithmetic', () => {
    expect(shown('72 °F * 2')).toBe('')
    expect(shown('20 °C + 5 °C')).toBe('')
  })
})

describe('audit: volume', () => {
  conversions([
    ['1 gal to l', 3.785411784],
    ['1 l to ml', 1000],
    ['1 cup to ml', 236.5882365],
    ['1 tbsp to tsp', 3],
    ['1 cup to tbsp', 16],
    ['1 cup to floz', 8],
    ['1 gal to floz', 128],
    ['1 pt to cup', 2],
    ['1 qt to pt', 2],
    ['1 gal to qt', 4],
    ['1 imp gal to l', 4.54609],
    ['1 m3 to l', 1000],
    ['1 m^3 to l', 1000],
    ['1 m³ to l', 1000],
    ['1 ft3 to l', 28.316846592],
    ['1 yd3 to ft3', 27],
    ['1 cm3 to ml', 1],
    ['1 cc to ml', 1],
    ['1 bbl to gal', 42],
    ['1 bushel to peck', 4],
    ['1 in3 to cm3', 16.387064],
    ['1 floz to ml', 29.5735295625],
    ['2 cm * 3 cm * 4 cm to ml', 24],
    ['1 dL to mL', 100],
    ['1 cL to mL', 10],
  ])
})

describe('audit: area', () => {
  conversions([
    ['1 acre to sqft', 43560],
    ['1 ha to m2', 10000],
    ['1 ha to acre', 10000 / 4046.8564224],
    ['1 sqmi to acre', 640],
    ['1 km2 to ha', 100],
    ['1 sqft to in2', 144],
    ['1 m^2 to cm^2', 10000],
    ['1 m² to ft²', 1 / FT ** 2],
    ['1 yd2 to sqft', 9],
    ['1 barn to m2', 1e-28],
    ['1 sq ft to m2', FT ** 2],
    ['1 square mile to km2', MI ** 2 / 1e6],
    ['10 m * 20 m to ha', 0.02],
  ])

  it('length times length is an area', () => {
    expectQty('2 m * 3 m', 6, 'm²')
    expectQty('(3 m)^2', 9, 'm²')
  })
})

describe('audit: speed and acceleration', () => {
  conversions([
    ['60 mph to km/h', 96.56064],
    ['100 km/h to mph', 100000 / MI],
    ['1 knot to km/h', 1.852],
    ['10 m/s to km/h', 36],
    ['1 m/s to ft/s', 1 / FT],
    ['1 lightspeed to m/s', 299792458],
    ['100 km / 2 hr to km/h', 50],
    ['60 mi / 1 hr to mph', 60],
    ['1 mi / 1 min to mph', 60],
    ['1 gee to m/s^2', 9.80665],
    ['1 m/s^2 to ft/s^2', 1 / FT],
    ['100 kph to m/s', 100 / 3.6],
  ])
})

describe('audit: time durations', () => {
  conversions([
    ['1 hr to min', 60],
    ['1 h to min', 60],
    ['1 day to hr', 24],
    ['1 wk to day', 7],
    ['1 wk to hr', 168],
    ['90 min to hr', 1.5],
    ['1.5 hours to minutes', 90],
    ['2.5 hours to minutes', 150],
    ['3600 s to hr', 1],
    ['86400 s to day', 1],
    ['1 yr to day', 365.25],
    ['1 month to day', 365.25 / 12],
    ['1 decade to yr', 10],
    ['1 century to yr', 100],
    ['1 min to s', 60],
    ['1 ms to s', 0.001],
    ['1 s to ms', 1000],
    ['1 us to ns', 1000],
    ['1 µs to ns', 1000],
    ['1 ns to ps', 1000],
    ['2 hr + 30 min to min', 150],
    ['1 day - 1 hr to hr', 23],
    ['1 hr 30 min to min', 90],
  ])

  it('juxtaposed durations add', () => {
    expectQty('1 hr 30 min', 1.5, 'hr')
  })

  it('min(3, 4) is the function, not minutes', () => {
    expectNum('min(3, 4)', 3)
    expectNum('max(3, 4) + min(1, 2)', 5)
  })

  it('clock readings stay blank', () => {
    for (const t of ['2:30', '12:00', '2:30 + 1:45', '1:30:00', '10:30 * 2']) expect(shown(t), t).toBe('')
  })
})

describe('audit: digital storage (GB is 10^9, GiB is 2^30)', () => {
  conversions([
    ['1 GB to MB', 1000],
    ['1 GiB to MiB', 1024],
    ['1 GiB to GB', 1.073741824],
    ['1 TB to GiB', 1e12 / 2 ** 30],
    ['1 kib to kb', 1.024],
    ['1 MB to KiB', 1e6 / 1024],
    ['500 GB to TB', 0.5],
    ['1 TiB to GiB', 1024],
    ['1 GB + 1 GiB to GB', 1 + 2 ** 30 / 1e9],
    ['1 GB / 1 MB', 1000],
    ['1 gigabyte to megabytes', 1000],
    ['1 gibibyte to mebibytes', 1024],
  ])
})

describe('audit: energy, power, pressure, force', () => {
  conversions([
    ['1 kWh to J', 3.6e6],
    ['1 kcal to kJ', 4.184],
    ['1 cal to J', 4.184],
    ['1 BTU to J', 1055.05585262],
    ['1 eV to J', 1.602176634e-19],
    ['1 therm to BTU', 100000],
    ['1 Wh to J', 3600],
    ['1 J to erg', 1e7],
    ['1 MJ to kWh', 1 / 3.6],
    ['1 keV to eV', 1000],
    ['1 GeV to J', 1.602176634e-10],
    ['1 hp to W', 745.699872],
    ['1 kW to W', 1000],
    ['1 MW to kW', 1000],
    ['1 GW to MW', 1000],
    ['1 mW to W', 0.001],
    ['100 W * 1 hr to kWh', 0.1],
    ['1 hp * 1 hr to kWh', 0.745699872],
    ['1 kWh / 1 hr to W', 1000],
    ['1 atm to Pa', 101325],
    ['1 atm to psi', 101325 / PSI],
    ['1 bar to kPa', 100],
    ['760 torr to atm', 1],
    ['1 atm to mmHg', 760],
    ['1 psi to kPa', PSI / 1000],
    ['1 MPa to psi', 1e6 / PSI],
    ['1 ksi to psi', 1000],
    ['1 inHg to kPa', 3.386389],
    ['1 kPa to Pa', 1000],
    ['1 hPa to Pa', 100],
    ['1000 mbar to bar', 1],
    ['1 GPa to MPa', 1000],
    ['1 lbf to N', 4.4482216152605],
    ['1 kgf to N', 9.80665],
    ['1 kN to N', 1000],
    ['1 kN to lbf', 1000 / 4.4482216152605],
    ['1 N to dyn', 100000],
    ['1 kip to lbf', 1000],
    ['1 mN to N', 0.001],
    ['1 MN to kN', 1000],
    ['1 N*m to J', 1],
    ['1 kg*m/s^2 to N', 1],
    ['1 J/s to W', 1],
  ])

  it('mass times acceleration is a force, force times distance is energy', () => {
    expectQty('10 kg * 9.8 m/s^2', 98, 'N')
    expectQty('5 N * 2 m', 10, 'J')
    expectQty('1 kWh / 1 hr', 1000, 'W')
  })
})

describe('audit: angle and frequency', () => {
  conversions([
    ['180 deg to rad', Math.PI],
    ['1 rad to deg', 180 / Math.PI],
    ['1 rev to deg', 360],
    ['60 arcmin to deg', 1],
    ['3600 arcsec to deg', 1],
    ['90° to rad', Math.PI / 2],
    ['1 mrad to rad', 0.001],
    ['60 rpm to Hz', 1],
    ['1 Hz to rpm', 60],
    ['1 kHz to Hz', 1000],
    ['1 MHz to kHz', 1000],
    ['1 GHz to Hz', 1e9],
  ])
})

describe('audit: electrical, and the F / C / H rules', () => {
  conversions([
    ['1 mA to A', 0.001],
    ['1 mA to uA', 1000],
    ['1 mV to V', 0.001],
    ['1 kV to V', 1000],
    ['1 kohm to ohm', 1000],
    ['1 kΩ to Ω', 1000],
    ['1 Mohm to kohm', 1000],
    ['1 megohm to ohm', 1e6],
    ['1 uF to nF', 1000],
    ['1 F to uF', 1e6],
    ['10 uF to F', 1e-5],
    ['1 nF to pF', 1000],
    ['1 pF to F', 1e-12],
    ['1 mH to H', 0.001],
    ['1 H to mH', 1000],
    ['1 uH to H', 1e-6],
    ['1 C to mC', 1000],
    ['1 kC to C', 1000],
    ['9 V / 3 kohm to mA', 3],
    ['5 V / 10 ohm to mA', 500],
    ['3V/39ohm in mA', 3000 / 39],
    ['1 A * 1 s to C', 1],
  ])

  it('capital F, C, H in arithmetic are farad, coulomb, henry', () => {
    expectQty('10 V * 2 A', 20, 'W')
    expectQty('12 V / 4 A', 3, 'Ω')
    expectQty('1 mA * 1 kohm', 1, 'V')
    expectQty('100 uF * 10 kohm', 1, 's')
    expectQty('5 C / 1 s', 5, 'A')
    expectQty('2 C * 3', 6, 'C')
    expectQty('1 H * 1 A / 1 s', 1, 'V')
  })

  it('a bare 1 C or 1 F in a plain conversion is a temperature', () => {
    expectQty('1 C to F', 33.8, '°F')
    expectQty('10 F to C', ((10 - 32) * 5) / 9, '°C')
  })

  it('capital H is henry, lowercase h is hours', () => {
    expectQty('1 H', 1, 'H')
    expectQty('2 h', 2, 'hr')
  })
})

describe('audit: prefix spellings', () => {
  conversions([
    ['1 km to m', 1000],
    ['1 mm to um', 1000],
    ['1 nm to m', 1e-9],
    ['1 nm to angstrom', 10],
    ['1 kg to g', 1000],
    ['1 mg to g', 0.001],
    ['1 mL to L', 0.001],
    ['1 kL to L', 1000],
    ['1 kilometer to meter', 1000],
    ['1 millisecond to second', 0.001],
    ['1 megawatt to kilowatt', 1000],
    ['1 kilojoule to joules', 1000],
    ['1 MeV to keV', 1000],
  ])

  // the alias table is matched lower-cased, so 'MPa', 'mg' and 'mm' win over their other-case siblings
  it('1 mPa (millipascal) to Pa = 0.001, not the megapascal 1e6', () => {
    expectNum('1 mPa to Pa', 0.001)
  })
  it('1 Mg (megagram) to kg = 1000, not the milligram 1e-6', () => {
    expectNum('1 Mg to kg', 1000)
  })
  it('1 Mm (megameter) to km = 1000, not the millimeter 1e-6', () => {
    expectNum('1 Mm to km', 1000)
  })
})

describe('audit: functions of quantities', () => {
  // mathjs reads "9 m^2" as its own Unit, and the scope's sqrt gives NaN for it
  it('sqrt(9 m^2) is 3 m or blank, not "undefined"', () => {
    expectBlankOr('sqrt(9 m^2)', ['3 m', '9.84251968504 ft'])
  })
})

describe('audit: mismatched dimensions never give a number', () => {
  it.each(['5 cm + 2 kg', '1 m to kg', '1 m to s', '1 kg to l', '1 J to W'])('%s', (text) => {
    const d = shown(text)
    expect(['', 'improper unit conversion'], `${text} → ${d}`).toContain(d)
  })

  it('mixed ± on units is blank or right', () => {
    expectBlankOr('(1.0 ± 0.1) m + (2.0 ± 0.1) m', ['3.0 ± 0.2 m'])
  })
})

describe('audit: unit answers keep their full value', () => {
  it('value.n is the converted amount, not the rounded display', () => {
    const r = line('1 km to mi')
    expect(r.value?.n).toBeCloseTo(1000 / MI, 14)
  })
})
