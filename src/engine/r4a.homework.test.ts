// whole homework sessions typed line by line, with stored quantities carried across
import { describe, expect, it } from 'vitest'
import { evaluateSheet } from './evaluate'

function rows(lines: string[]) {
  return evaluateSheet(lines)
}

function last(lines: string[]) {
  return rows(lines).at(-1)!
}

/** The last line's value converted to `unit`, so the display unit doesn't matter. */
function inUnit(lines: string[], unit: string): number | undefined {
  const r = last([...lines.slice(0, -1), `${lines.at(-1)} to ${unit}`])
  return r.value?.kind === 'number' && r.value.unit ? r.value.n : undefined
}

describe('a stored quantity times a plain variable whose name is also a unit', () => {
  const setup = ['a = 5 cm', 'b = 2']

  it('multiplies, divides and converts', () => {
    expect(inUnit([...setup, 'a*b'], 'cm')).toBeCloseTo(10, 10)
    expect(inUnit([...setup, 'b*a'], 'cm')).toBeCloseTo(10, 10)
    expect(inUnit([...setup, 'a/b'], 'cm')).toBeCloseTo(2.5, 10)
    expect(last([...setup, 'b*a to mm']).display).toBe('100 mm')
  })

  it('still reads the variable, not the unit, when it is the only thing there', () => {
    expect(last(['n = 5', '2 n']).value?.n).toBe(10)
    expect(last(['n = 5', 'n*2']).value?.unit).toBeUndefined()
    expect(last(['m = 2', '5 m']).value?.n).toBe(10)
  })

  it('leaves a unit that is also a variable blank rather than picking one', () => {
    expect(last(['s = 3', '10 m/s']).display).toBe('')
    expect(last(['m = 2', 'a = 5 cm', 'a to m']).display).toBe('')
  })

  it('a unit literal times such a variable', () => {
    expect(last(['b = 2', '5 kg * b']).display).toBe('10 kg')
    expect(last(['b = 2', '2 b kg']).display).toBe('4 kg')
  })
})

describe('a quantity converted into something that is not a unit', () => {
  it('is blank, not a product with inches', () => {
    expect(last(['x = 2', '5 cm in x']).display).toBe('')
    expect(last(['m = 2', 'a = 5 cm', 'a in m']).display).toBe('')
    expect(last(['1 in in cm']).display).toBe('2.54 cm')
  })
})

describe('a one-letter unit assigned to its own name', () => {
  it('V = 12 V stores 12 volts instead of solving V = 0', () => {
    const r = rows(['V = 12 V', 'R = 4 ohm', 'V/R'])
    expect(r[0]!.kind).toBe('assignment')
    expect(r[0]!.display).toBe('12 V')
    expect(r[2]!.display).toBe('3 A')
  })

  it.each(['s', 'A', 'W', 'L', 'K'])('%s assigned to itself', (u) => {
    const r = rows([`${u} = 12 ${u}`, `${u}*2`])
    expect(r[0]!.kind).toBe('assignment')
    expect(r[1]!.value?.n).toBeCloseTo(24 * (r[0]!.value!.n / 12), 9)
  })

  it('reassigns', () => {
    expect(last(['V = 12 V', 'V = 24 V', 'V']).display).toBe('24 V')
  })

  it('a real equation in the same letter still solves', () => {
    expect(last(['x = 12 x']).display).toBe('0')
    expect(last(['s = 2s + 1']).display).toBe('-1')
  })
})

describe('an assignment that looks like a reaction', () => {
  it('V1 = 9 V is 9 volts, not V₁ → V', () => {
    const r = rows(['V1 = 9 V', 'V1*2'])
    expect(r[0]!.display).toBe('9 V')
    expect(r[1]!.display).toBe('18 V')
    expect(last(['V2 = 3V']).display).toBe('3 V')
  })

  it('real one-species reactions still balance', () => {
    expect(last(['H2 = 2H']).display).toBe('H₂ → 2 H')
    expect(last(['O3 = O2']).display).toBe('2 O₃ → 3 O₂')
    expect(last(['2O3 = 3O2']).display).toBe('2 O₃ → 3 O₂')
  })
})

describe('kinematics with stored quantities', () => {
  const setup = ['v0 = 12 m/s', 'a = 9.8 m/s^2', 't = 2 s']

  it('an acceleration shown in g is reusable', () => {
    expect(inUnit([...setup, 'v0*t + 0.5*a*t^2'], 'm')).toBeCloseTo(43.6, 9)
    expect(inUnit([...setup, 'v0 + a*t'], 'm/s')).toBeCloseTo(31.6, 9)
    expect(inUnit([...setup, 'v0^2/(2a)'], 'm')).toBeCloseTo(144 / 19.6, 9)
    expect(inUnit([...setup, '0.5 a t^2'], 'm')).toBeCloseTo(19.6, 9)
  })
})

describe('compound units stored in a variable', () => {
  it('a spring constant in N/m chains', () => {
    const setup = ['k = 200 N/m', 'x0 = 5 cm']
    expect(last([...setup, '0.5*k*x0^2']).display).toBe('0.25 J')
    expect(last([...setup, '0.5 k x0^2']).display).toBe('0.25 J')
    expect(last([...setup, 'k*x0']).display).toBe('10 N')
  })

  it('hc = 1240 eV nm gives a photon energy', () => {
    expect(last(['hc = 1240 eV nm', 'hc/(500 nm) to eV']).value?.n).toBeCloseTo(2.48, 9)
  })
})

describe('photon energy from E = hc/λ', () => {
  const setup = ['lam = 500 nm', 'En = 6.626e-34 J s * 2.998e8 m/s / lam']

  it('in eV and kJ/mol', () => {
    const joules = (6.626e-34 * 2.998e8) / 500e-9
    expect(last([...setup, 'En to eV']).value?.n).toBeCloseTo(joules / 1.602176634e-19, 9)
    expect(last([...setup, 'En*6.022e23 to kJ']).value?.n).toBeCloseTo((joules * 6.022e23) / 1000, 9)
  })
})

describe('lab data with ± added and subtracted', () => {
  const setup = ['L = 1.25 ± 0.02 m', 'W = 0.80 ± 0.01 m']

  it('perimeter and sums', () => {
    expect(last([...setup, '2L + 2W']).display).toBe('4.10 ± 0.06 m')
    expect(last([...setup, '2(L+W)']).display).toBe('4.10 ± 0.06 m')
    expect(last([...setup, 'L + W']).display).toBe('2.05 ± 0.03 m')
    expect(last([...setup, 'L + 1 m']).display).toBe('2.25 ± 0.02 m')
    expect(last(['(1.25 ± 0.02 m) + (0.80 ± 0.01 m)']).display).toBe('2.05 ± 0.03 m')
  })

  it('matches the same sum without units', () => {
    expect(last(['1.25 ± 0.02 + 0.80 ± 0.01']).display).toBe('2.05 ± 0.03')
  })

  it('mixed length units add', () => {
    expect(last(['q = 5.0 ± 0.1 cm', 'q + 1 in']).display).toBe('7.54 ± 0.10 cm')
  })

  it('a difference of zero has no relative uncertainty, so it stays blank', () => {
    expect(last(['q = 5.0 ± 0.1 cm', 'q - q']).display).toBe('')
  })

  it('products still work as before', () => {
    expect(last([...setup, 'L*W']).display).toBe('1.00 ± 0.03 m²')
    expect(last([...setup, 'L^2']).display).toBe('1.56 ± 0.05 m²')
  })
})

describe('general chemistry', () => {
  it('isotope averages', () => {
    expect(last(['34.969x+36.966(1-x)=35.45']).value?.n).toBeCloseTo((36.966 - 35.45) / (36.966 - 34.969), 12)
    expect(last(['6.0151a + 7.0160(1-a) = 6.941']).value?.n).toBeCloseTo((7.016 - 6.941) / (7.016 - 6.0151), 12)
    expect(last(['34.969p/100+36.966(100-p)/100=35.45']).value?.n).toBeCloseTo((100 * (36.966 - 35.45)) / (36.966 - 34.969), 10)
  })

  it('molar mass and moles', () => {
    expect(last(['M = 2*1.008 + 15.999', 'm = 36.0', 'n = m/M']).value?.n).toBeCloseTo(36 / 18.015, 12)
    expect(last(['C = 12.011', 'H = 1.008', 'O = 15.999', '6C + 12H + 6O']).value?.n).toBeCloseTo(180.156, 10)
  })

  it('ideal gas with plain numbers', () => {
    expect(last(['P = 1.00', 'V = 22.4', 'R = 0.08206', 'T = 273.15', 'n = P*V/(R*T)']).value?.n).toBeCloseTo(22.4 / (0.08206 * 273.15), 12)
  })
})

describe('circuits', () => {
  it('ohm, power and capacitors', () => {
    expect(last(['V1 = 9 V', 'R1 = 3 kohm', 'V1/R1 to mA']).display).toBe('3 mA')
    expect(last(['C1 = 10 uF', 'Vc = 5 V', 'Q = C1*Vc', 'Q to uC']).display).toBe('50 uC')
    expect(last(['R1 = 100', 'R2 = 220', 'R1 R2/(R1+R2)']).value?.n).toBeCloseTo(68.75, 12)
  })

  it('kΩ is one unit, not kelvin times ohm', () => {
    expect(last(['9 V / (3 kΩ) to mA']).display).toBe('3 mA')
    expect(last(['3 kΩ * 2']).display).toBe('6 kΩ')
    expect(last(['2 MΩ * 1 uA']).display).toBe('2 V')
    expect(last(['3 µΩ * 2']).display).toBe('6 μΩ')
  })
})

describe('every prefixed unit survives being stored', () => {
  const prefixes = ['k', 'M', 'G', 'm', 'µ', 'μ', 'u', 'n', 'p']
  const bases = ['m', 'g', 's', 'L', 'J', 'W', 'Pa', 'Hz', 'N', 'V', 'A', 'Ω', 'ohm', 'F', 'C', 'eV']
  it.each(prefixes.flatMap((p) => bases.map((b) => p + b)))('%s', (u) => {
    const r = evaluateSheet([`q = 3 ${u}`, 'z = 2', `q*z to ${u}`, 'q/q'])
    expect(r[2]!.display).toBe(`6 ${r[2]!.value?.unit}`)
    expect(r[3]!.value?.n).toBe(1)
  })
})
