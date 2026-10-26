// WP-SYMBOLIC corpus: mathjs simplify / derivative / rationalize / toString on generated and hand-picked
// expressions, through the engine's own `math` instance (with scientific.ts's divide/pow/mod overrides), so the
// symbolic.* fixtures reach far more rules than the engine's tests alone. Nothing is asserted.
import { it } from 'vitest'
import { math } from '../../../src/engine/math'
import '../../../src/engine/scientific'
import { int, pick, rng, type Rng } from './gen'

type Node = ReturnType<typeof math.parse>

const ATOMS = ['x', 'x', 'x', 'y', 'a', 'b', '2', '3', '1', '0', '4', '5', '0.5', '2.5', '0.1', '1.5', 'pi', 'e', '10']
const FUNCS = ['sin', 'cos', 'tan', 'sqrt', 'log', 'exp', 'abs', 'cbrt', 'asin', 'atan', 'sec', 'cosh', 'log10']

function expr(r: Rng, depth: number): string {
  const k = r()
  if (depth <= 0 || k < 0.25) return pick(r, ATOMS)
  if (k < 0.35) return `-${expr(r, depth - 1)}`
  if (k < 0.45) return `${pick(r, FUNCS)}(${expr(r, depth - 1)})`
  if (k < 0.52) return `${int(r, 2, 9)}${pick(r, ['x', 'y', 'a', 'x y', '(x+1)', 'x^2'])}`
  if (k < 0.62) return `(${expr(r, depth - 1)})^${pick(r, ['2', '3', '-1', '0.5', '1/2', 'n', '2', 'x'])}`
  const op = pick(r, [' + ', ' - ', ' * ', ' / ', ' + ', ' * ', ' - '])
  const wrap = (s: string) => (r() < 0.5 ? `(${s})` : s)
  return `${wrap(expr(r, depth - 1))}${op}${wrap(expr(r, depth - 1))}`
}

function rational(r: Rng, depth: number): string {
  const k = r()
  if (depth <= 0 || k < 0.3) return pick(r, ['x', 'x', 'a', 'b', '1', '2', '3', '-1', '0.5', '4'])
  if (k < 0.4) return `(${rational(r, depth - 1)})^${int(r, 2, 3)}`
  const op = pick(r, [' + ', ' - ', ' * ', ' / '])
  return `(${rational(r, depth - 1)})${op}(${rational(r, depth - 1)})`
}

const HAND = [
  '2x + 3x', 'x + x', 'x - x', 'x * x', 'x * x * x', 'x / x', '2 * x / 4', 'x/2 + x/3', '0.5 * x', '0.4 * x',
  '(x + 1)^2', '(x + 1)(x - 1)', '(a + b)^2 - a^2', 'x^2 * x^3', 'x^2 / x', 'x^-1', '1/x + 1/y', '1/(1/a + 1/b)',
  '-(x + y)', '-(-x)', '--x', 'x + -y', '2 - -3x', '3 * (x + 2) - 3x', 'x * 2 * y * 3', 'pi * 2 * x', 'e^x * e^y',
  'log(e)', 'log(e) * x', 'sin(x)^2 + cos(x)^2', 'sqrt(x)^2', 'sqrt(4)', 'sqrt(2) * x', '2^0.5 * x', '(-8)^(1/3)',
  '0 * x', '1 * x', 'x^0', 'x^1', '0 / x', 'x / 1', '0 + x', 'x - 0', '0 - x', '1/0', '0/0', '2/0 * x', 'x mod 3',
  '7 mod 3', '(x^2 - 1)/(x - 1)', '(x^2 + 2x + 1)/(x + 1)', '(a*b^3 + a^3*b)/(a*b^2 + a^3)', '(1 + 2*a + a^2)/(2 + a)',
  '(-1 - a)/(2 + a)', '(-c + b + a)/2', 'x*y/(x*z)', '3/(6x)', '(2x)/(4y)', 'x/(y/z)', 'x/y/z', '(x/y)/(z/w)',
  '1e5 * x', '0.0001 * x', '123456.7 * x', '1e21 + x', '0.1 + 0.2', '1/3 + x', '0.333 * x', '2.675 * x',
  'x^2 + 2x^2 - x^2', '3x^2 - 2x + 1 - (x^2 + x)', 'x * (x + 1) - x^2', 'a*x + b*x', 'a*x - b*x', 'x*a + a',
  '(x+1)*(x+2)*(x+3)', '(2x - 1)^3', 'x^(1/2) * x^(1/2)', 'x^a * x^b', '(x^a)^b', '(2^x)^2', 'sin(x)*sin(x)',
  'cos(2x) * 3 / 6', 'abs(-3) * x', 'exp(0) + x', 'x - (y + z)', 'x - (y - z)', 'a - b - c', 'a / b / c',
  'x deg__', 'sin(x deg__)', 'k0__ * x + k1__', '__held0 * x^2 / __held0', '50% * x', '2 x y z', '-2x', '-x^2',
  '(-x)^2', '-(x^2)', '2^-x', 'x^-2', '1/x^2', 'x^2/y^3', 'n1 + n2', 'true and x', 'x or false',
]

const PRINT_OPTIONS = [
  undefined,
  { parenthesis: 'keep' },
  { parenthesis: 'auto' },
  { parenthesis: 'all' },
  { parenthesis: 'auto', implicit: 'hide' },
  { parenthesis: 'auto', implicit: 'show' },
  { implicit: 'show' },
]

function attempt(f: () => unknown): void {
  try {
    f()
  } catch {
    // recorded as a throw
  }
}

function exercise(text: string, r: Rng | null): void {
  let node: Node
  try {
    node = math.parse(text)
  } catch {
    return
  }
  for (const o of r ? [pick(r, PRINT_OPTIONS), pick(r, PRINT_OPTIONS)] : PRINT_OPTIONS) attempt(() => node.toString(o as never))
  attempt(() => math.simplify(node))
  attempt(() => math.simplify(text))
  attempt(() => math.derivative(node, 'x', { simplify: false }))
  if (!r || r() < 0.3) attempt(() => math.derivative(node, 'x'))
  attempt(() => math.rationalize(node))
}

it('hand-picked symbolic expressions', () => {
  for (const t of HAND) exercise(t, null)
})

it('generated symbolic expressions', () => {
  const r = rng(5150)
  for (let i = 0; i < 700; i++) exercise(expr(r, int(r, 1, 3)), r)
})

it('generated rational expressions', () => {
  const r = rng(8086)
  for (let i = 0; i < 300; i++) exercise(rational(r, int(r, 1, 3)), r)
})

it('simplified and differentiated trees print', () => {
  const r = rng(1999)
  for (let i = 0; i < 300; i++) {
    const text = expr(r, int(r, 1, 3))
    attempt(() => {
      const s = math.simplify(text)
      for (const o of PRINT_OPTIONS) s.toString(o as never)
      const d = math.derivative(text, 'x', { simplify: false })
      for (const o of PRINT_OPTIONS) d.toString(o as never)
    })
  }
})
