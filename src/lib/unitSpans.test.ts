import { describe, expect, it } from 'vitest'
import { unitSpans } from './unitSpans'

const read = (text: string, variables?: string[], functions?: string[]) =>
  unitSpans(text, variables, functions).map((u) => `${text.slice(u.start, u.end)}=${u.name}`)

describe('unitSpans', () => {
  it.each([
    ['5 m', ['m=meter']],
    ['5m', ['m=meter']],
    ['1.2*10^3 Nm / (28*10^9 Pa)', ['Nm=newton meter', 'Pa=pascal']],
    ['3 nm', ['nm=nanometer']],
    ['9.8 m/s^2', ['m=meter', 's=second']],
    ['9.8 m/s^2 * 3 kg', ['m=meter', 's=second', 'kg=kilogram']],
    ['10 km to mi', ['km=kilometer', 'mi=mile']],
    ['10 km in mi', ['km=kilometer', 'mi=mile']],
    ['5 in', ['in=inch']],
    ['20 °C to F', ['°C=celsius', 'F=fahrenheit']],
    ['20 C to F', ['C=celsius', 'F=fahrenheit']],
    ['2 F * 3 V', ['F=farad', 'V=volt']],
    ['50/hr', ['hr=hour']],
  ])('finds the units in %s', (text, want) => {
    expect(read(text)).toEqual(want)
  })

  it.each(['x + 2', 'sin(30)', 'min(3, 4)', '2 pi', '3 e', 'm = 5', 'hello', 'f(x) = x^2'])('finds none in %s', (text) => {
    expect(read(text)).toEqual([])
  })

  it('leaves a stored variable or function alone', () => {
    expect(read('2 m', ['m'])).toEqual([])
    expect(read('2 g', [], ['g'])).toEqual([])
  })
})
