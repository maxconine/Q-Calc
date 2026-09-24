import { describe, expect, it } from 'vitest'
import { expectValue, last } from './audit.helpers'
import { evaluateSheet } from './evaluate'

describe('audit2: a function body re-reads its free variables at call time, not at definition time', () => {
  it('redefining a variable a function refers to changes later calls', () => {
    const out = evaluateSheet(['k = 10', 'f(x) = x + k', 'f(1)', 'k = 20', 'f(1)'])
    expect(out.map((r) => r.display)).toEqual(['10', 'f(x) = x + k', '11', '20', '21'])
  })

  it('redefining a function another function calls changes the caller too (dynamic dispatch)', () => {
    const out = evaluateSheet(['f(x) = 2x', 'g(x) = f(x) + 1', 'g(3)', 'f(x) = 3x', 'g(3)'])
    expect(out.map((r) => r.display)).toEqual(['f(x) = 2x', 'g(x) = f(x) + 1', '7', 'f(x) = 3x', '10'])
  })

  it('a function that reads ans sees ans as of the moment it is called', () => {
    const out = evaluateSheet(['5', 'f(x) = x + ans', 'f(1)', '10', 'f(1)'])
    expect(out.map((r) => r.display)).toEqual(['5', 'f(x) = x + ans', '6', '10', '11'])
  })
})

describe('audit2: a function parameter shadows a global variable of the same name', () => {
  it('f(x) = x*2 uses its own x, and the global x is untouched after the call', () => {
    const out = evaluateSheet(['x = 100', 'f(x) = x * 2', 'f(3)', 'x'])
    expect(out.map((r) => r.display)).toEqual(['100', 'f(x) = x * 2', '6', '100'])
  })
})

describe('audit2: redefining a function can change its whole signature, not just its body', () => {
  it('params bind positionally, so swapping the declared parameter names changes what a call means', () => {
    // first def: f(x, y) = x + y; second def completely replaces it with f(y, x) = x - y,
    // so f(10, 3) binds y=10, x=3 under the *second* definition, giving 3 - 10 = -7
    const out = evaluateSheet(['f(x, y) = x + y', 'f(y, x) = x - y', 'f(10, 3)'])
    expect(out.map((r) => r.display)).toEqual(['f(x, y) = x + y', 'f(y, x) = x - y', '-7'])
  })
})

describe('audit2: sequential assignment, not a simultaneous swap', () => {
  it('b = a after a = b reads the just-updated a, so both end up equal', () => {
    const out = evaluateSheet(['a = 1', 'b = 2', 'a = b', 'b = a', 'a', 'b'])
    expect(out.map((r) => r.display)).toEqual(['1', '2', '2', '2', '2', '2'])
  })
})

describe('audit2: theta the symbol and theta the word are the same variable', () => {
  it('assigning θ = 30 and using it in sin() works like any other variable', () => {
    const out = evaluateSheet(['θ = 30', 'sin(θ)'])
    expect(out.map((r) => r.display)).toEqual(['30', '0.5'])
  })

  it('a function parameter named θ is stored and shown as the word "theta"', () => {
    const out = evaluateSheet(['f(θ) = sin(θ)', 'f(30)'])
    expect(out[0]!.display).toBe('f(theta) = sin(theta)')
    expect(out[1]!.display).toBe('0.5')
  })
})

describe('audit2: unguarded recursion and ternaries stay within the documented blank/undefined contract', () => {
  // there is no conditional operator support in the function walker and no base-case detection,
  // so a recursive definition either times out into 'undefined' or is blank; either is accepted
  // per the existing audit ("a function that calls itself forever gives no number and does not
  // throw"), never a wrong finite number.
  it('a factorial-style recursive function without a guard does not throw and gives no number', () => {
    const out = last(['fact(n) = n * fact(n-1)', 'fact(5)'])
    expect(['', 'undefined']).toContain(out.display)
  }, 20_000)

  it('a ternary-guarded recursive function is also just blank/undefined, not a wrong finite value', () => {
    const out = last(['f(x) = x <= 1 ? 1 : x * f(x-1)', 'f(5)'])
    expect(['', 'undefined']).toContain(out.display)
  }, 20_000)
})

describe('audit2: multi-step composition across many lines', () => {
  it('three functions chained together compute correctly', () => {
    const out = evaluateSheet(['double(x) = 2x', 'addOne(x) = x + 1', 'combo(x) = addOne(double(x))', 'combo(5)'])
    expectValue(out[out.length - 1]!, 11)
  })

  it('a five-line build-up of variables and a function using all of them', () => {
    const out = evaluateSheet(['a = 2', 'b = 3', 'c = 4', 'f(x) = a*x^2 + b*x + c', 'f(2)'])
    expectValue(out[out.length - 1]!, 2 * 4 + 3 * 2 + 4)
  })
})
