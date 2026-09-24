import { describe, expect, it } from 'vitest'
import { math } from './math'
import { solveLive, solveSystem } from './system'

function num(s: string, scope: Record<string, number> = {}): number {
  return Number(math.evaluate(s, scope))
}

function expectUnique(eqs: string[], values: Record<string, string>) {
  const r = solveSystem(eqs)
  expect(r, eqs.join(' ; ')).not.toBeNull()
  expect(r!.status).toBe('unique')
  const sol = Object.fromEntries(r!.variables.map((name, i) => [name, r!.solutions![0]![i]!]))
  for (const [name, want] of Object.entries(values)) {
    expect(num(sol[name]!), name).toBeCloseTo(num(want), 8)
  }
  for (const eq of eqs) {
    const [lhs, rhs] = eq.split('=')
    const scope = Object.fromEntries(r!.variables.map((name) => [name, num(sol[name]!)]))
    expect(num(lhs!, scope)).toBeCloseTo(num(rhs!, scope), 6)
  }
  return r!
}

function expectNone(eqs: string[]) {
  const r = solveSystem(eqs)
  expect(r).not.toBeNull()
  expect(r!.status).toBe('inconsistent')
  expect(r!.display).toBe('no solution')
}

function expectInfinite(eqs: string[], freeCount: number) {
  const r = solveSystem(eqs)
  expect(r, eqs.join(' ; ')).not.toBeNull()
  expect(r!.status).toBe('infinite')
  expect(r!.free).toHaveLength(freeCount)
  const samples = [0, 1, -2, 0.5]
  for (const t of samples) {
    const scope: Record<string, number> = {}
    r!.free!.forEach((name, i) => {
      scope[name] = t + i
    })
    r!.variables.forEach((name, i) => {
      scope[name] = num(r!.general![i]!, scope)
    })
    for (const eq of eqs) {
      const [lhs, rhs] = eq.split('=')
      expect(num(lhs!, scope)).toBeCloseTo(num(rhs!, scope), 6)
    }
  }
  return r!
}

describe('live systems', () => {
  it('answers once the typed equations determine the unknowns', () => {
    const early = solveLive(['x + y = 5', 'x - y = 1', ''])
    expect(early?.display).toBe('x = 3, y = 2')
    expect(early?.message).toBeUndefined()
  })

  it('keeps that answer while the next equation is still being typed', () => {
    expect(solveLive(['x + y = 5', 'x - y = 1', '2x +'])?.display).toBe('x = 3, y = 2')
  })

  it('waits when the typed equations still leave a free variable', () => {
    expect(solveLive(['x + y = 5', '', ''])).toBeNull()
  })

  it('uses a later equation as soon as it parses', () => {
    expect(solveLive(['x + y = 5', 'x - y = 1', 'y = 4'])?.display).toBe('no solution')
  })
})

describe('systems of equations', () => {
  it('1 reciprocal', () => {
    expectUnique(['3/x + 4/y = -5', '2/x - 5/y = 12'], { x: '1', y: '-1/2' })
  })

  it('2 exponential', () => {
    expectUnique(['2^x * 4^y = 32', '3^x / 9^y = 1/3'], { x: '2', y: '3/2' })
  })

  it('3 inconsistent', () => {
    expectNone(['1234x + 5678y = 9012', '2468x + 11356y = 9012'])
  })

  it('4 fractions', () => {
    expectUnique(['(5/7)x - (3/11)y = 81/77', '(2/3)x + (4/5)y = 26/5'], { x: '3', y: '4' })
  })

  it('5 dependent radicals', () => {
    const r = expectInfinite(['sqrt(2)x - sqrt(3)y = sqrt(6)', '2sqrt(3)x - 3sqrt(2)y = 6'], 1)
    const scope: Record<string, number> = {}
    r.free!.forEach((name) => {
      scope[name] = 5
    })
    r.variables.forEach((name, i) => {
      scope[name] = num(r.general![i]!, scope)
    })
    const y = (Math.SQRT2 * scope.x! - Math.sqrt(6)) / Math.sqrt(3)
    expect(scope.y).toBeCloseTo(y, 8)
  })

  it('6 ill conditioned', () => {
    expectUnique(['1.0001x + 2.0000y = 3.0001', '1.0000x + 2.0000y = 3.0000'], { x: '1', y: '1' })
  })

  it('7 nested radicals', () => {
    expectUnique(['3sqrt(x+1) - 2sqrt(y-2) = 11', '4sqrt(x+1) + 5sqrt(y-2) = 30'], { x: '24', y: '6' })
  })

  it('8 parameters', () => {
    const r = solveSystem(['a*x + b*y = a^2 + b^2', 'b*x - a*y = 0'])
    expect(r!.status).toBe('unique')
    const sol = Object.fromEntries(r!.variables.map((name, i) => [name, r!.solutions![0]![i]!]))
    expect(num(sol.x!, { a: 2, b: 5 })).toBeCloseTo(2, 8)
    expect(num(sol.y!, { a: 2, b: 5 })).toBeCloseTo(5, 8)
    expect(num(sol.x!, { a: -3, b: 4 })).toBeCloseTo(-3, 8)
    expect(num(sol.y!, { a: -3, b: 4 })).toBeCloseTo(4, 8)
  })

  it('9 absolute values', () => {
    expectUnique(['abs(x) + 2y = 7', '2x - abs(y) = 1'], { x: '9/5', y: '13/5' })
  })

  it('10 sine and cosine', () => {
    const r = solveSystem(['2sin(theta) + 3cos(phi) = 2', '4sin(theta) - 9cos(phi) = -1'])
    expect(r!.status).toBe('unique')
    const atoms = Object.fromEntries(r!.atoms!.map((a) => [a.expr, a.value]))
    expect(num(atoms['sin(theta)']!)).toBeCloseTo(0.5, 10)
    expect(num(atoms['cos(phi)']!)).toBeCloseTo(1 / 3, 10)
    expect(r!.angles!.theta).toEqual(['pi/6', '5*pi/6'])
    expect(r!.angles!.phi!.map((a) => num(a))).toEqual([Math.acos(1 / 3), 2 * Math.PI - Math.acos(1 / 3)])
  })

  it('11 3x3', () => {
    expectUnique(['2x + 3y + 4z = 3', '4x - 6y - 8z = -2', '6x + 9y - 4z = -3'], { x: '1/2', y: '-1/3', z: '3/4' })
  })

  it('12 inconsistent 3x3', () => {
    expectNone(['x + 2y - 3z = 4', '2x - y + 4z = 5', '3x + y + z = 10'])
  })

  it('13 one free parameter', () => {
    const r = expectInfinite(['x - 2y + 3z = 5', '2x - 3y + z = 3', '3x - 5y + 4z = 8'], 1)
    const t = 4
    const scope: Record<string, number> = {}
    r.free!.forEach((name) => {
      scope[name] = t
    })
    r.variables.forEach((name, i) => {
      scope[name] = num(r.general![i]!, scope)
    })
    expect(scope.x).toBeCloseTo(-9 + 7 * t, 8)
    expect(scope.y).toBeCloseTo(-7 + 5 * t, 8)
    expect(scope.z).toBeCloseTo(t, 8)
  })

  it('14 parameters a b c', () => {
    const r = solveSystem(['x + y = a', 'y + z = b', 'z + x = c'])!
    expect(r.status).toBe('unique')
    const sol = Object.fromEntries(r.variables.map((name, i) => [name, r.solutions![0]![i]!]))
    const scope = { a: 3, b: 5, c: 6 }
    expect(num(sol.x!, scope)).toBeCloseTo((3 - 5 + 6) / 2, 8)
    expect(num(sol.y!, scope)).toBeCloseTo((3 + 5 - 6) / 2, 8)
    expect(num(sol.z!, scope)).toBeCloseTo((-3 + 5 + 6) / 2, 8)
  })

  it('15 nontrivial null space', () => {
    const r = expectInfinite(['x + 2y - z = 0', '2x + y + z = 0', '3x + 3y = 0'], 1)
    const scope: Record<string, number> = {}
    r.free!.forEach((name) => {
      scope[name] = 3
    })
    r.variables.forEach((name, i) => {
      scope[name] = num(r.general![i]!, scope)
    })
    expect(scope.y).toBeCloseTo(-scope.x!, 8)
    expect(scope.z).toBeCloseTo(-scope.x!, 8)
  })

  it('16 reciprocals in three variables', () => {
    expectUnique(
      ['1/x + 2/y - 1/z = 3', '2/x - 1/y + 3/z = 6', '3/x + 3/y - 2/z = 5'],
      { x: '1', y: '1/2', z: '1/2' },
    )
  })

  it('17 hilbert 3', () => {
    expectUnique(
      ['x + (1/2)y + (1/3)z = 1', '(1/2)x + (1/3)y + (1/4)z = 0', '(1/3)x + (1/4)y + (1/5)z = 0'],
      { x: '9', y: '-36', z: '30' },
    )
  })

  it('18 parameter a', () => {
    const r = solveSystem(['a*x + y + z = 1', 'x + a*y + z = a', 'x + y + a*z = a^2'])!
    expect(r.status).toBe('unique')
    const sol = Object.fromEntries(r.variables.map((name, i) => [name, r.solutions![0]![i]!]))
    const a = 3
    expect(num(sol.x!, { a })).toBeCloseTo((-a - 1) / (a + 2), 8)
    expect(num(sol.y!, { a })).toBeCloseTo(1 / (a + 2), 8)
    expect(num(sol.z!, { a })).toBeCloseTo((a + 1) ** 2 / (a + 2), 8)
  })

  it('19 two quadratic solutions', () => {
    const r = solveSystem(['x + y + z = 26', 'x - y + z = 14', 'x^2 + y^2 + z^2 = 364'])!
    expect(r.status).toBe('finite')
    const sols = r.solutions!.map((sol) => Object.fromEntries(r.variables.map((name, i) => [name, num(sol[i]!)])))
    const has = (x: number, y: number, z: number) => sols.some((s) => s.x === x && s.y === y && s.z === z)
    expect(has(18, 6, 2)).toBe(true)
    expect(has(2, 6, 18)).toBe(true)
  })

  it('20 tridiagonal', () => {
    expectUnique(['4x1 - x2 = 7', '-x1 + 4x2 - x3 = 5', '-x2 + 4x3 = 15'], { x1: '5/2', x2: '3', x3: '9/2' })
  })

  it('21 4x4', () => {
    expectUnique(
      [
        'x1 + 2x2 - x3 + 3x4 = -9',
        '2x1 - x2 + 3x3 - x4 = 14',
        '-x1 + 3x2 + 2x3 - 2x4 = 1',
        '3x1 + x2 - 2x3 + 4x4 = -9',
      ],
      { x1: '1', x2: '-2', x3: '3', x4: '-1' },
    )
  })

  it('22 back substitution', () => {
    expectUnique(
      ['2x1 - 3x2 + x3 - 4x4 = 12', '3x2 + 2x3 - x4 = 5', '4x3 + 3x4 = -2', '5x4 = 10'],
      { x1: '33/2', x2: '11/3', x3: '-2', x4: '2' },
    )
  })

  it('23 vandermonde', () => {
    expectUnique(
      ['c0 = 1', 'c0 + c1 + c2 + c3 = 2', 'c0 + 2c1 + 4c2 + 8c3 = 9', 'c0 + 3c1 + 9c2 + 27c3 = 28'],
      { c0: '1', c1: '0', c2: '0', c3: '1' },
    )
  })

  it('24 inconsistent 4x4', () => {
    expectNone([
      'x1 + x2 + x3 + x4 = 4',
      '2x1 + 3x2 - x3 + 2x4 = 6',
      '3x1 + 4x2 + 0x3 + 3x4 = 11',
      '4x1 + 5x2 + x3 + 4x4 = 12',
    ])
  })

  it('25 rank 1', () => {
    const r = expectInfinite(
      [
        'x1 + 2x2 - x3 + x4 = 2',
        '2x1 + 4x2 - 2x3 + 2x4 = 4',
        '3x1 + 6x2 - 3x3 + 3x4 = 6',
        '-x1 - 2x2 + x3 - x4 = -2',
      ],
      3,
    )
    const scope: Record<string, number> = { x2: 3, x3: -1, x4: 4 }
    const x1 = num(r.general![r.variables.indexOf('x1')]!, scope)
    expect(x1).toBeCloseTo(2 - 2 * 3 + -1 - 4, 8)
  })

  it('26 circulant', () => {
    expectUnique(
      [
        'x1 + 2x2 + 3x3 + 4x4 = 10',
        '4x1 + x2 + 2x3 + 3x4 = 10',
        '3x1 + 4x2 + x3 + 2x4 = 10',
        '2x1 + 3x2 + 4x3 + x4 = 10',
      ],
      { x1: '1', x2: '1', x3: '1', x4: '1' },
    )
  })

  it('27 second difference', () => {
    expectUnique(
      ['2x1 - x2 = 1', '-x1 + 2x2 - x3 = 0', '-x2 + 2x3 - x4 = 0', '-x3 + 2x4 = 5'],
      { x1: '9/5', x2: '13/5', x3: '17/5', x4: '21/5' },
    )
  })

  it('28 mixed fractions', () => {
    expectUnique(
      [
        'x1 + x2 + x3 + x4 = 77/60',
        '12x1 - 6x2 + 4x3 - 30x4 = -1',
        '2x1 + 3x2 + 4x3 + 5x4 = 4',
        '6x1 - 9x2 + 12x3 - 15x4 = 0',
      ],
      { x1: '1/2', x2: '1/3', x3: '1/4', x4: '1/5' },
    )
  })

  it('29 two blocks', () => {
    expectUnique(['x1 + y1 = 3', 'x1 - y1 = 1', 'x2 + y2 = 7', 'x2 - y2 = 3'], { x1: '2', y1: '1', x2: '5', y2: '2' })
  })

  it('30 4x4 dense', () => {
    expectUnique(
      [
        'x1 - 2x2 + 3x3 - 4x4 = 10',
        '-4x1 + 3x2 - 2x3 + x4 = -10',
        '2x1 + 5x2 - x3 + 3x4 = -7',
        '3x1 - x2 + 4x3 - 2x4 = 10',
      ],
      { x1: '1', x2: '-1', x3: '1', x4: '-1' },
    )
  })

  it('31 triangular 5x5', () => {
    expectUnique(
      [
        'x1 + 2x2 - x3 + 3x4 - x5 = 5',
        '2x2 + x3 - 2x4 + 4x5 = 8',
        '3x3 + x4 - x5 = 3',
        '4x4 + 2x5 = 10',
        '5x5 = 15',
      ],
      { x1: '31/3', x2: '-11/6', x3: '5/3', x4: '1', x5: '3' },
    )
  })

  it('32 dense 5x5', () => {
    expectUnique(
      [
        'x1 + x2 + x3 + x4 + x5 = 15',
        'x1 - x2 + x3 - x4 + x5 = 3',
        '2x1 + 3x2 - x3 + x4 - 2x5 = -1',
        '3x1 - x2 + 2x3 + 4x4 - x5 = 18',
        '-x1 + 2x2 + 3x3 - x4 + x5 = 13',
      ],
      { x1: '1', x2: '2', x3: '3', x4: '4', x5: '5' },
    )
  })

  it('33 pentadiagonal', () => {
    expectUnique(
      ['4x1 + x2 = 6', 'x1 + 4x2 + x3 = 12', 'x2 + 4x3 + x4 = 18', 'x3 + 4x4 + x5 = 23', 'x4 + 4x5 = 20'],
      { x1: '1', x2: '2', x3: '3', x4: '4', x5: '4' },
    )
  })

  it('34 inconsistent 5x5', () => {
    expectNone([
      'x1 + 2x2 + 3x3 + 4x4 + 5x5 = 1',
      '2x1 + 3x2 + 4x3 + 5x4 + x5 = 2',
      '3x1 + 4x2 + 5x3 + x4 + 2x5 = 3',
      '6x1 + 9x2 + 12x3 + 10x4 + 8x5 = 10',
      '4x1 + 5x2 + x3 + 2x4 + 3x5 = 4',
    ])
  })

  it('35 two free parameters', () => {
    const r = expectInfinite(
      [
        'x1 + x2 + x3 + x4 + x5 = 5',
        'x1 + 2x2 + 3x3 + 4x4 + 5x5 = 15',
        'x1 + 3x2 + 5x3 + 7x4 + 9x5 = 25',
        '2x1 + 3x2 + 4x3 + 5x4 + 6x5 = 20',
        'x1 - x5 = -3',
      ],
      2,
    )
    const s = 2
    const t = -1
    const want = { x1: t - 3, x2: 6 + s, x3: 2 - 2 * t - 2 * s, x4: s, x5: t }
    const scope: Record<string, number> = {}
    r.free!.forEach((name) => {
      scope[name] = want[name as keyof typeof want]
    })
    r.variables.forEach((name, i) => {
      scope[name] = num(r.general![i]!, scope)
    })
    for (const [name, value] of Object.entries(want)) expect(scope[name]).toBeCloseTo(value, 8)
  })

  it('36 interpolation', () => {
    expectUnique(
      [
        '16a - 8b + 4c - 2d + e = 3',
        'a - b + c - d + e = 0',
        'e = 1',
        'a + b + c + d + e = 6',
        '16a + 8b + 4c + 2d + e = 27',
      ],
      { a: '1/2', b: '1', c: '3/2', d: '2', e: '1' },
    )
  })

  it('37 hilbert 5', () => {
    expectUnique(
      [
        'x1 + (1/2)x2 + (1/3)x3 + (1/4)x4 + (1/5)x5 = 1',
        '(1/2)x1 + (1/3)x2 + (1/4)x3 + (1/5)x4 + (1/6)x5 = 0',
        '(1/3)x1 + (1/4)x2 + (1/5)x3 + (1/6)x4 + (1/7)x5 = 0',
        '(1/4)x1 + (1/5)x2 + (1/6)x3 + (1/7)x4 + (1/8)x5 = 0',
        '(1/5)x1 + (1/6)x2 + (1/7)x3 + (1/8)x4 + (1/9)x5 = 0',
      ],
      { x1: '25', x2: '-300', x3: '1050', x4: '-1400', x5: '630' },
    )
  })

  it('38 diagonally dominant', () => {
    expectUnique(
      [
        '10x1 - x2 + 2x3 - x4 + x5 = 16',
        '-x1 + 10x2 - x3 + 2x4 - x5 = -4',
        '2x1 - x2 + 10x3 - x4 + 2x5 = -2',
        '-x1 + 2x2 - x3 + 10x4 - x5 = 28',
        'x1 - x2 + 2x3 - x4 + 10x5 = -20',
      ],
      { x1: '2', x2: '-1', x3: '0', x4: '3', x5: '-2' },
    )
  })

  it('39 permutation', () => {
    expectUnique(
      ['x1 + 3x3 = 5', 'x2 + 3x4 = 15', 'x3 + 3x5 = 16', 'x4 + 3x1 = 10', 'x5 + 3x2 = 14'],
      { x1: '2', x2: '3', x3: '1', x4: '4', x5: '5' },
    )
  })

  it('40 cosines', () => {
    const r = solveSystem([
      '2cos(A) + cos(B) - cos(C) + 3cos(D) - cos(E) = 4',
      'cos(A) - 2cos(B) + 2cos(C) - cos(D) + cos(E) = -1',
      '3cos(A) + cos(B) + cos(C) - 2cos(D) + 2cos(E) = 2',
      '-cos(A) + 3cos(B) - cos(C) + cos(D) - 3cos(E) = -1',
      '4cos(A) - cos(B) + 3cos(C) + 2cos(D) + cos(E) = 5/2',
    ])!
    expect(r.status).toBe('unique')
    const atoms = Object.fromEntries(r.atoms!.map((a) => [a.expr, a.value]))
    expect(num(atoms['cos(A)']!)).toBeCloseTo(1, 10)
    expect(num(atoms['cos(B)']!)).toBeCloseTo(0, 10)
    expect(num(atoms['cos(C)']!)).toBeCloseTo(-1, 10)
    expect(num(atoms['cos(D)']!)).toBeCloseTo(0.5, 10)
    expect(num(atoms['cos(E)']!)).toBeCloseTo(0.5, 10)
    const principal = (name: string) => r.angles![name]!.map((a) => num(a)).find((a) => a >= 0 && a <= Math.PI)
    expect(principal('A')).toBeCloseTo(0, 8)
    expect(principal('B')).toBeCloseTo(Math.PI / 2, 8)
    expect(principal('C')).toBeCloseTo(Math.PI, 8)
    expect(principal('D')).toBeCloseTo(Math.PI / 3, 8)
    expect(principal('E')).toBeCloseTo(Math.PI / 3, 8)
  })
})
