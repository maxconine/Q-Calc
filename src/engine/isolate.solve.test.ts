import { describe, expect, it } from 'vitest'
import { evaluateLine, evaluateSheet } from './evaluate'
import { isIsolateCommand, isolatePrevious, parseNamedSolve } from './isolate'
import type { LineResult } from './types'

const line = (s: string, variables?: Record<string, number>) => evaluateLine(s, { variables })
const numeric = (r: LineResult, display: string) => {
  expect(r.kind).toBe('solve')
  expect(r.display).toBe(display)
}
const formula = (r: LineResult, display: string) => {
  expect(r.kind).toBe('expression')
  expect(r.display).toBe(display)
}

// `solve x` answers with a number when it can, and with x on its own when other letters are unknown
describe('solve falls back to isolate', () => {
  describe('a number whenever one exists', () => {
    it('1. solve … for x', () => numeric(line('solve 2x + 3 = 11 for x'), '4'))
    it('2. solve x in …', () => numeric(line('solve x in 2x + 3 = 11'), '4'))
    it('3. solve for x: … with two roots', () => numeric(line('solve for x: x^2 = 9'), '±3'))
    it('4. … for x without the word solve', () => numeric(line('2x + 3 = 11 for x'), '4'))
    it('5. trailing , solve x', () => numeric(line('x^2 - 5x + 6 = 0, solve x'), '2, 3'))
    it('6. no real root stays a message, not ±sqrt(-1)', () => numeric(line('solve x in x^2 = -1'), 'no real solution'))
    it('7. an exponential with one letter', () => numeric(line('solve x in 2^x = 10'), '3.32192809489'))
    it('8. a stored letter is used', () => numeric(line('solve x in a x = 6', { a: 2 }), '3'))
    it('9. stored letters written together still multiply', () => {
      expect(evaluateSheet(['y = 7', 'b = 1', 'm = 2', 'solve x in y = mx + b']).map((r) => r.display)).toEqual(['7', '1', '2', '3'])
    })
    it('10. an identity says so', () => numeric(line('solve x in x = x'), 'true for all x'))
    it('11. a numeric root sets ans', () => {
      expect(evaluateSheet(['solve x in 2x = 10', 'ans + 1']).at(-1)!.display).toBe('6')
    })
  })

  describe('x on its own when other letters are unknown', () => {
    it('12. solve … for x', () => formula(line('solve y = mx + b for x'), 'x = (y - b)/m'))
    it('13. solve x in …', () => formula(line('solve x in y = mx + b'), 'x = (y - b)/m'))
    it('14. solve for x: …', () => formula(line('solve for x: y = mx + b'), 'x = (y - b)/m'))
    it('15. solve for x … with only a space', () => formula(line('solve for x y = mx + b'), 'x = (y - b)/m'))
    it('16. …, solve x', () => formula(line('y = mx + b, solve x'), 'x = (y - b)/m'))
    it('17. … solve for x', () => formula(line('y = mx + b solve for x'), 'x = (y - b)/m'))
    it('18. … for x', () => formula(line('y = mx + b for x'), 'x = (y - b)/m'))
    it('19. letters written together multiply', () => formula(line('PV = nRT for T'), 'T = P*V/(R*n)'))
    it('20. a subscripted name, and fractions cleared', () => formula(line('solve for R in 1/R = 1/R1 + 1/R2'), 'R = R1*R2/(R2 + R1)'))
    it('21. solving for the subscripted one', () => formula(line('solve R1 in 1/R = 1/R1 + 1/R2'), 'R1 = R2*R/(R2 - R)'))
    it('22. an even power gives ±', () => formula(line('solve for c: E = mc^2'), 'c = ±sqrt(E/m)'))
    it('23. the quadratic formula', () => formula(line('solve a x^2 + b x + c = 0 for x'), 'x = (-b ± sqrt(b^2 - 4a*c))/(2a)'))
    it('24. a quadratic with a leading fraction and a flipped sign', () => formula(line('solve for t: d = v t + 1/2 a t^2'), 't = (-v ± sqrt(v^2 + 2a*d))/a'))
    it('25. an expression with no = is set to 0, number first', () => formula(line('solve x in 2x + 3y'), 'x = -3y/2'))
    it('26. under a square root', () => formula(line('solve h in v = sqrt(2gh)'), 'h = v^2/(2g)'))
    it('27. in an exponent of e', () => formula(line('solve x in y = e^(kx)'), 'x = ln(y)/k'))
    it('28. inside ln', () => formula(line('solve x in ln(x) = y'), 'x = e^y'))
    it('29. inside a base-10 log', () => formula(line('solve x in log(x) = y'), 'x = 10^y'))
    it('30. inside an absolute value', () => formula(line('solve x in |x| = y'), 'x = ±y'))
    it('31. a shifted square reads as m ± h', () => formula(line('solve x in (x+1)^2 = y'), 'x = -1 ± sqrt(y)'))
    it('32. a cube is a cube root', () => formula(line('solve x in x^3 = y'), 'x = cbrt(y)'))
    it('33. a root true on part of the domain is kept', () => formula(line('solve x in sqrt(x) = -y'), 'x = y^2'))
    it('34. a circle', () => formula(line('solve x in x^2 + y^2 = r^2'), 'x = ±sqrt(r^2 - y^2)'))
    it('35. x in a denominator', () => formula(line('solve x in 3 = y/x'), 'x = y/3'))
    it('36. theta keeps its symbol', () => formula(line('solve θ in y = r sin(θ)'), 'θ = asin(y/r)'))
    it('37. keywords in capitals', () => formula(line('SOLVE X IN Y = M X + B'), 'X = (Y - B)/M'))
    it('38. extra spaces', () => formula(line('  solve   for   x :  y=mx+b  '), 'x = (y - b)/m'))
    it('39. a stored letter is filled in, the rest stay letters', () => formula(line('solve x in y = mx + b', { m: 2 }), 'x = (y - b)/2'))
    it('40. a formula leaves ans alone', () => {
      expect(evaluateSheet(['2 + 3', 'solve x in y = mx + b', 'ans * 2']).map((r) => r.display)).toEqual(['5', 'x = (y - b)/m', '10'])
    })
  })

  describe('blank, or left to the rest of the engine', () => {
    it('41. the letter is not in the equation', () => formula(line('solve z in y = mx + b'), ''))
    it('42. x in and out of a function', () => formula(line('solve x in x + sin(x) = y'), ''))
    it('43. a fifth-degree polynomial', () => formula(line('solve x in x^5 + x = y'), ''))
    it('44. x cancels out', () => formula(line('solve x in x - x = y'), ''))
    it('45. an inequality', () => formula(line('solve x in x < y'), ''))
    it('46. two equals signs', () => formula(line('solve x in y = x = z'), ''))
    it('47. an assignment still stores', () => {
      const r = line('x = 5')
      expect(r.kind).toBe('assignment')
      expect(r.display).toBe('5')
    })
    it('48. `for` in words and graphs is not a solve', () => {
      expect(parseNamedSolve('$10 for lunch')).toBeNull()
      expect(parseNamedSolve('5 for x')).toBeNull()
      expect(parseNamedSolve('graph y = mx for x')).toBeNull()
      expect(isIsolateCommand('$10 for lunch')).toBe(false)
      expect(isIsolateCommand('solve x in y = mx + b')).toBe(true)
    })
    it('49. a solve with no letter named still solves', () => numeric(line('solve x^2 = 4'), '±2'))
  })

  describe('`solve x` alone uses the row before', () => {
    it('50. rewrites the previous equation, whatever form it was in', () => {
      expect(isolatePrevious('solve for x', 'y = mx + b')).toBe('solve y = mx + b for x')
      expect(isolatePrevious('solve x', 'isolate x in y = mx + b')).toBe('solve y = mx + b for x')
      expect(isolatePrevious('isolate b', 'solve y = mx + b for x')).toBe('isolate b in y = mx + b')
      expect(isolatePrevious('solve x', 'y = mx + b, solve m')).toBe('solve y = mx + b for x')
      expect(line(isolatePrevious('solve for x', '2x + 3 = 11')!).display).toBe('4')
      expect(line(isolatePrevious('solve for m', 'y = mx + b')!).display).toBe('m = (y - b)/x')
    })
    it('51. needs an equation before it', () => {
      expect(isolatePrevious('solve for x', '2 + 3')).toBeNull()
      expect(isolatePrevious('solve for x', 'f(x) = x^2')).toBeNull()
      expect(isolatePrevious('solve for x', 'graph y = x')).toBeNull()
      expect(isolatePrevious('solve for x', undefined)).toBeNull()
    })
  })
})
