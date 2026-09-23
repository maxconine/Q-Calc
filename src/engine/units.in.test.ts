import { describe, expect, it } from 'vitest'
import { evaluateLine } from './evaluate'

type Case = {
  name: string
  input: string
  expected: number
  eps?: number
  unit?: string | RegExp
}

const IN = 0.0254
const FT = 0.3048
const MI = 1609.344
const LBF = 4.4482216152605
const G0 = 9.80665
const ATM = 101325
const TORR = ATM / 760
const BTU = 1055.05585262
const HP = 745.699872
const C = 299792458
const E = 1.602176634e-19
const AMU = 1.6605390666e-27

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function run(c: Case): void {
  const r = evaluateLine(c.input)
  const label = `${c.input} → ${r.display}`
  const actual = r.value?.n
  expect(actual, label).toEqual(expect.any(Number))
  const scale = Math.max(Math.abs(c.expected), 1e-12)
  const eps = c.eps ?? Math.max(1e-8, scale * 1e-6)
  expect(Math.abs(actual! - c.expected), `${label} (expected ${c.expected})`).toBeLessThan(eps)
  if (c.unit !== undefined) {
    if (typeof c.unit === 'string') expect(r.display, label).toMatch(new RegExp(`${escapeRe(c.unit)}$`))
    else expect(r.display, label).toMatch(c.unit)
  }
}

const cases: Case[] = [
  { name: '1. kV/ohm in milliamps', input: 'kV/ohm in mA', expected: 1e6, unit: 'mA' },
  { name: '2. 1 kV / 1 ohm in mA', input: '1 kV / 1 ohm in mA', expected: 1e6, unit: 'mA' },
  { name: '3. 1 kV / 1 ohm in A', input: '1 kV / 1 ohm in A', expected: 1000, unit: 'A' },
  { name: '4. 1 kV / 1 ohm in kA', input: '1 kV / 1 ohm in kA', expected: 1, unit: 'kA' },
  { name: '5. 1 kV / 1 ohm in uA', input: '1 kV / 1 ohm in uA', expected: 1e9, unit: 'uA' },
  { name: '6. 3 V / 39 ohm in mA', input: '3V/39ohm in mA', expected: (3 / 39) * 1000, unit: 'mA' },
  { name: '7. 12 V / 4 Ohm in A', input: '12 V / 4 Ohm in A', expected: 3, unit: 'A' },
  { name: '8. 12 V / 4 kOhm in mA', input: '12 V / 4 kOhm in mA', expected: 3, unit: 'mA' },
  { name: '9. 9 V / 1.5 kOhm in mA', input: '9 V / 1.5 kOhm in mA', expected: 6, unit: 'mA' },
  { name: '10. 5 kV / 10 ohm in A', input: '5 kV / 10 ohm in A', expected: 500, unit: 'A' },
  { name: '11. 1 kV / 2 kOhm in mA', input: '1 kV / 2 kOhm in mA', expected: 500, unit: 'mA' },
  { name: '12. 1 kV / 1 kOhm in A', input: '1 kV / 1 kOhm in A', expected: 1, unit: 'A' },
  { name: '13. 100 millivolt / 50 ohm in mA', input: '100 millivolt / 50 ohm in mA', expected: 2, unit: 'mA' },
  { name: '14. 2 kV / 4 megohm in uA', input: '2 kV / 4 megohm in uA', expected: 500, unit: 'uA' },
  { name: '15. 120 V / 20 ohm in A', input: '120 V / 20 ohm in A', expected: 6, unit: 'A' },
  { name: '16. 1 V / 1 ohm in uA', input: '1 V / 1 ohm in uA', expected: 1e6, unit: 'uA' },
  { name: '17. 24 V / 8 ohm in mA', input: '24 V / 8 ohm in mA', expected: 3000, unit: 'mA' },
  { name: '18. 5 V / 10 kOhm in uA', input: '5 V / 10 kOhm in uA', expected: 500, unit: 'uA' },
  { name: '19. 1 mA * 1 kOhm in V', input: '1 mA * 1 kOhm in V', expected: 1, unit: 'V' },
  { name: '20. 2 A * 5 Ohm in V', input: '2 A * 5 Ohm in V', expected: 10, unit: 'V' },

  { name: '21. 2 A * 5 Ohm in mV', input: '2 A * 5 Ohm in mV', expected: 10000, unit: 'mV' },
  { name: '22. 120 V * 10 A in W', input: '120 V * 10 A in W', expected: 1200, unit: 'W' },
  { name: '23. 120 V * 10 A in kW', input: '120 V * 10 A in kW', expected: 1.2, unit: 'kW' },
  { name: '24. 12 V * 500 mA in W', input: '12 V * 500 mA in W', expected: 6, unit: 'W' },
  { name: '25. 100 mV * 2 mA in mW', input: '100 mV * 2 mA in mW', expected: 0.2, unit: 'mW' },
  { name: '26. 24 V * 2 A in W', input: '24 V * 2 A in W', expected: 48, unit: 'W' },
  { name: '27. 5 A * 10 s in Coulomb', input: '5 A * 10 s in Coulomb', expected: 50, unit: 'Coulomb' },
  { name: '28. 1 milliampere * 1 hour in Coulomb', input: '1 milliampere * 1 hour in Coulomb', expected: 3.6, unit: 'Coulomb' },
  { name: '29. 1 Farad * 12 V in Coulomb', input: '1 Farad * 12 V in Coulomb', expected: 12, unit: 'Coulomb' },
  { name: '30. 1 uF * 10 kV in mC', input: '1 uF * 10 kV in mC', expected: 10, unit: 'mC' },
  { name: '31. 12 V * 2 Coulomb in J', input: '12 V * 2 Coulomb in J', expected: 24, unit: 'J' },
  { name: '32. 100 J / 5 Coulomb in V', input: '100 J / 5 Coulomb in V', expected: 20, unit: 'V' },
  { name: '33. 100 Volt / 2 amp in Ohm', input: '100 Volt / 2 amp in Ohm', expected: 50, unit: 'Ohm' },
  { name: '34. 1 Watt / 1 Volt in A', input: '1 Watt / 1 Volt in A', expected: 1, unit: 'A' },
  { name: '35. 1 Joule / 1 Volt in Coulomb', input: '1 Joule / 1 Volt in Coulomb', expected: 1, unit: 'Coulomb' },
  { name: '36. 1 Farad * 1 Ohm in s', input: '1 Farad * 1 Ohm in s', expected: 1, unit: 's' },
  { name: '37. 1 Henry / 1 Ohm in s', input: '1 Henry / 1 Ohm in s', expected: 1, unit: 's' },
  { name: '38. 1 Henry * 1 A / 1 s in V', input: '1 Henry * 1 A / 1 s in V', expected: 1, unit: 'V' },
  { name: '39. 0.5 * 10 microfarad * (12 Volt)^2 in J', input: '0.5 * 10 microfarad * (12 Volt)^2 in J', expected: 0.00072, unit: 'J' },
  { name: '40. 0.5 * 2 millihenry * (5 amp)^2 in J', input: '0.5 * 2 millihenry * (5 amp)^2 in J', expected: 0.025, unit: 'J' },

  { name: '41. 10 N * 2 m in J', input: '10 N * 2 m in J', expected: 20, unit: 'J' },
  { name: '42. 10 N * 2 m in kJ', input: '10 N * 2 m in kJ', expected: 0.02, unit: 'kJ' },
  { name: '43. 10 N * 5 m in cal', input: '10 N * 5 m in cal', expected: 50 / 4.184, unit: 'cal' },
  { name: '44. 10 lbf * 5 ft in ft * lbf', input: '10 lbf * 5 ft in ft * lbf', expected: 50, unit: 'ft * lbf' },
  { name: '45. 1 lbf * 1 ft in J', input: '1 lbf * 1 ft in J', expected: LBF * FT, unit: 'J' },
  { name: '46. 10 kg * 2 m/s^2 in N', input: '10 kg * 2 m/s^2 in N', expected: 20, unit: 'N' },
  { name: '47. 10 kg * 2 m/s^2 in lbf', input: '10 kg * 2 m/s^2 in lbf', expected: 20 / LBF, unit: 'lbf' },
  { name: '48. 100 Lbm * 1 gravity in lbf', input: '100 Lbm * 1 gravity in lbf', expected: 100, unit: 'lbf' },
  { name: '49. 500 g * 2 m/s^2 in N', input: '500 g * 2 m/s^2 in N', expected: 1, unit: 'N' },
  { name: '50. 1 metricTon * 1 m/s^2 in kN', input: '1 metricTon * 1 m/s^2 in kN', expected: 1, unit: 'kN' },
  { name: '51. 100 PSI * 2 in^2 in lbf', input: '100 PSI * 2 in^2 in lbf', expected: 200, unit: 'lbf' },
  { name: '52. 1 bar * 2 m^2 in kN', input: '1 bar * 2 m^2 in kN', expected: 200, unit: 'kN' },
  { name: '53. 1 atm * 1 m^2 in N', input: '1 atm * 1 m^2 in N', expected: ATM, unit: 'N' },
  { name: '54. 50 N / 2 m^2 in Pa', input: '50 N / 2 m^2 in Pa', expected: 25, unit: 'Pa' },
  { name: '55. 1000 PoundForce / 1 Inch^2 in ksi', input: '1000 PoundForce / 1 Inch^2 in ksi', expected: 1, unit: 'ksi' },
  { name: '56. 5 Pa * 100 cm^2 in N', input: '5 Pa * 100 cm^2 in N', expected: 0.05, unit: 'N' },
  { name: '57. 10 Torr * 1 sq ft in lbf', input: '10 Torr * 1 sq ft in lbf', expected: (10 * TORR * FT * FT) / LBF, unit: 'lbf' },
  { name: '58. 1 kip * 10 ft in ft * lbf', input: '1 kip * 10 ft in ft * lbf', expected: 10000, unit: 'ft * lbf' },
  { name: '59. 1000 dyne * 50 cm in ergs', input: '1000 dyne * 50 cm in ergs', expected: 50000, unit: 'ergs' },
  { name: '60. 2 kgf * 5 m in J', input: '2 kgf * 5 m in J', expected: 2 * G0 * 5, unit: 'J' },

  { name: '61. 1 kW * 2 hr in kWh', input: '1 kW * 2 hr in kWh', expected: 2, unit: 'kWh' },
  { name: '62. 1 kW * 2 hr in J', input: '1 kW * 2 hr in J', expected: 7.2e6, unit: 'J' },
  { name: '63. 100 W * 10 s in J', input: '100 W * 10 s in J', expected: 1000, unit: 'J' },
  { name: '64. 100 W * 10 s in kJ', input: '100 W * 10 s in kJ', expected: 1, unit: 'kJ' },
  { name: '65. 1 hp * 1 s in J', input: '1 hp * 1 s in J', expected: HP, unit: 'J' },
  { name: '66. 1 hp * 1 hr in BTU', input: '1 hp * 1 hr in BTU', expected: (HP * 3600) / BTU, unit: 'BTU' },
  { name: '67. 50 W * 1 day in kWh', input: '50 W * 1 day in kWh', expected: (50 * 86400) / 3.6e6, unit: 'kWh' },
  { name: '68. 10 kW * 5 ms in J', input: '10 kW * 5 ms in J', expected: 50, unit: 'J' },
  { name: '69. 1000 J / 10 s in W', input: '1000 J / 10 s in W', expected: 100, unit: 'W' },
  { name: '70. 1 kWh / 1 hr in kW', input: '1 kWh / 1 hr in kW', expected: 1, unit: 'kW' },
  { name: '71. 1 cal / 1 s in W', input: '1 cal / 1 s in W', expected: 4.184, unit: 'W' },
  { name: '72. 1 BTU / 1 min in W', input: '1 BTU / 1 min in W', expected: BTU / 60, unit: 'W' },
  { name: '73. 1000 BTU / 1 minute in kilowatt', input: '1000 BTU / 1 minute in kilowatt', expected: (1000 * BTU) / 60 / 1000, unit: 'kilowatt' },
  { name: '74. 550 Foot * PoundForce / 1 second in HorsePower', input: '550 Foot * PoundForce / 1 second in HorsePower', expected: 1, unit: 'HorsePower' },

  { name: '75. 20 m * 2 in in m^2', input: '20 m * 2 in in m^2', expected: 40 * IN, unit: 'm^2' },
  { name: '76. 2 in * 3 in in cm^2', input: '2 in * 3 in in cm^2', expected: (2 * IN * 3 * IN) / 1e-4, unit: 'cm^2' },
  { name: '77. 10 ft * 5 m in ft^2', input: '10 ft * 5 m in ft^2', expected: (10 * 5) / FT, unit: 'ft^2' },
  { name: '78. 1 km * 1 mm in m^2', input: '1 km * 1 mm in m^2', expected: 1, unit: 'm^2' },
  { name: '79. 1 acre * 1 ft in ft^3', input: '1 acre * 1 ft in ft^3', expected: 43560, unit: 'ft^3' },
  { name: '80. 10 m^2 * 50 cm in L', input: '10 m^2 * 50 cm in L', expected: 5000, unit: 'L' },
  { name: '81. 100 sq ft * 2 in in ft^3', input: '100 sq ft * 2 in in ft^3', expected: 100 * (2 / 12), unit: 'ft^3' },
  { name: '82. 1 hectare * 10 mm in L', input: '1 hectare * 10 mm in L', expected: 100000, unit: 'L' },
  { name: '83. 1 sq yd * 3 ft in ft^3', input: '1 sq yd * 3 ft in ft^3', expected: 27, unit: 'ft^3' },
  { name: '84. 1 m + 50 cm in in', input: '1 m + 50 cm in in', expected: 1.5 / IN, unit: 'in' },

  { name: '85. 100 miles / 2 hr in mph', input: '100 miles / 2 hr in mph', expected: 50, unit: 'mph' },
  { name: '86. 100 miles / 2 hr in km/h', input: '100 miles / 2 hr in km/h', expected: 50 * 1.609344, unit: 'km/h' },
  { name: '87. 100 m / 10 s in km/h', input: '100 m / 10 s in km/h', expected: 36, unit: 'km/h' },
  { name: '88. 10 m/s in ft/s', input: '10 m/s in ft/s', expected: 10 / FT, unit: 'ft/s' },
  { name: '89. 10 m/s in in/s', input: '10 m/s in in/s', expected: 10 / IN, unit: 'in/s' },
  { name: '90. 1 NauticalMile / 1 hr in knot', input: '1 NauticalMile / 1 hr in knot', expected: 1, unit: 'knot' },
  { name: '91. 10 km / 15 min in km/h', input: '10 km / 15 min in km/h', expected: 40, unit: 'km/h' },
  { name: '92. 60 mph / 5 s in m/s^2', input: '60 mph / 5 s in m/s^2', expected: 12 * (MI / 3600), unit: 'm/s^2' },
  { name: '93. 30 m/s / 3 s in gravity', input: '30 m/s / 3 s in gravity', expected: 10 / G0, unit: 'gravity' },
  { name: '94. 1 lightYear / 1 yr in m/s', input: '1 lightYear / 1 yr in m/s', expected: C, unit: 'm/s' },

  { name: '95. 10 kg / 2 L in g/cm^3', input: '10 kg / 2 L in g/cm^3', expected: 5, unit: 'g/cm^3' },
  { name: '96. 1 g / 1 cm^3 in kg/m^3', input: '1 g / 1 cm^3 in kg/m^3', expected: 1000, unit: 'kg/m^3' },
  { name: '97. 360 deg / 1 s in RPM', input: '360 deg / 1 s in RPM', expected: 60, unit: 'RPM' },
  { name: '98. 10 kg * 9.81 m/s^2 * 5 m in J', input: '10 kg * 9.81 m/s^2 * 5 m in J', expected: 490.5, unit: 'J' },
  { name: '99. (100 N * 5 m) / 2 s in W', input: '(100 N * 5 m) / 2 s in W', expected: 250, unit: 'W' },
  { name: '100. 1 AMU * (1 speedOfLight)^2 in MeV', input: '1 AMU * (1 speedOfLight)^2 in MeV', expected: (AMU * C * C) / E / 1e6, unit: 'MeV' },
]

describe('Output units with "in __"', () => {
  it('has 100 cases', () => {
    expect(cases).toHaveLength(100)
  })

  it.each(cases)('$name', (c) => run(c))
})
