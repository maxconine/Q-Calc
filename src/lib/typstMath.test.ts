import { describe, expect, it } from 'vitest'
import { TYPST_PREAMBLE, copiedEquation, previewMath, toTypstMath, typstAnswer, typstDocument, typstPreviewUseful } from './typstMath'

describe('copiedEquation', () => {
  it('keeps the typed equation until Typst copy is on', () => {
    expect(copiedEquation('√(2)+1', false)).toBe('√(2)+1')
    expect(copiedEquation('√(2)+1', true)).toBe('sqrt(2) + 1')
    expect(copiedEquation('2^8', true)).toBe('2 ^ 8')
  })
})

describe('toTypstMath', () => {
  it('writes the last answer as an upright word, so a chain like `+ 2` has a left side', () => {
    expect(toTypstMath('ans+ 2')).toBe('"ans" + 2')
    expect(toTypstMath('sin(ans)')).toBe('sin("ans")')
  })

  it('typesets ordinary calculations', () => {
    expect(toTypstMath('sin(90)')).toBe('sin(90)')
    expect(toTypstMath('sqrt(2)')).toBe('sqrt(2)')
    expect(toTypstMath('2^8')).toBe('2 ^ 8')
    expect(toTypstMath('1/2')).toBe('1 / 2')
    expect(toTypstMath('π')).toBe('pi')
    expect(toTypstMath('√(2)')).toBe('sqrt(2)')
    expect(toTypstMath('2 * 3')).toBe('2 dot 3')
    expect(toTypstMath('2 dot 3')).toBe('2 dot.op 3')
    expect(toTypstMath('2 · 3')).toBe('2 dot.op 3')
  })

  it('a decimal point is a point, and a lone one still finishes', () => {
    expect(toTypstMath('.')).toBe('.')
    expect(toTypstMath('2+.')).toBe('2 + .')
    expect(toTypstMath('.5')).toBe('.5')
    expect(toTypstMath('1.5')).toBe('1.5')
  })

  it('uses the preamble macros for the matching names', () => {
    expect(toTypstMath('bu + bv')).toBe('#bu + #bv')
    expect(toTypstMath('arctan(x)')).toBe('(#tan1)(x)')
    expect(toTypstMath('±')).toBe('#pm')
    expect(toTypstMath('ip(x, y)')).toBe('#ip($x$, $y$)')
    expect(toTypstMath('nCr(5, 2)')).toBe('binom(5, 2)')
  })

  it('typesets a sum and an integral', () => {
    expect(toTypstMath('Σ n^2, n=1..10')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('Σ_x=2^5 2')).toBe('sum_(x = 2)^(5) 2')
    expect(toTypstMath('Σ_n=1^10 n^2')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('Σ_{n=1}^{10} n')).toBe('sum_(n = 1)^(10) n')
    expect(toTypstMath('Π_n=1^5 n')).toBe('product_(n = 1)^(5) n')
    expect(toTypstMath('∫(x^2, 0, 1)')).toBe('integral_(0)^(1) x ^ 2')
    expect(toTypstMath('∫_23')).toBe('integral_(23)')
    expect(toTypstMath('∫_0^1 x^2')).toBe('integral_(0)^(1) x ^ 2')
    expect(toTypstMath('∫_0^{} x')).toBe('integral_(0) x')
    expect(toTypstMath('∫0..1 x')).toBe('integral_(0)^(1) x')
    expect(toTypstMath('∫_{L_0}^L (dl)/l')).toBe('integral_(L_(0))^(L) (dif l) / l')
    expect(toTypstMath('∫_0^1 x^2 dx')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('∫_0^π/2 cos(x) dx')).toBe('integral_(0)^(pi / 2) cos(x) dif x')
    expect(toTypstMath('∫(x^2 dx, 0, 1)')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('∫(x^2, x, 0, 1)')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('int_0^1 x^2 dx')).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(toTypstMath('sum_{n=1}^{10} n^2')).toBe('sum_(n = 1)^(10) n ^ 2')
    expect(toTypstMath('prod_n=1^5 n')).toBe('product_(n = 1)^(5) n')
    expect(toTypstMath('2*(Σ_n=1^10 n^2)+1')).toBe('2 dot (sum_(n = 1)^(10) n ^ 2) + 1')
    expect(copiedEquation('∫_0^1 x^2 dx', true)).toBe('integral_(0)^(1) x ^ 2 dif x')
    expect(copiedEquation('Σ_n=1^10 n^2', true)).toBe('sum_(n = 1)^(10) n ^ 2')
  })

  it('writes words Typst does not know upright, so a unit or a name never fails the preview', () => {
    expect(toTypstMath('5 km')).toBe('5 "km"')
    expect(toTypstMath('2xy')).toBe('2 "xy"')
    expect(toTypstMath('x1 + x2')).toBe('x_(1) + x_(2)')
    expect(toTypstMath('alpha + Omega')).toBe('alpha + Omega')
    expect(toTypstMath('factorial(5)')).toBe('op("factorial")(5)')
    expect(toTypstMath('floor(2.5)')).toBe('floor(2.5)')
    expect(toTypstMath('arcsec(2)')).toBe('op("arcsec")(2)')
    expect(toTypstMath('gcd(4, 6)')).toBe('gcd(4, 6)')
  })

  it('keeps unicode letters, stray brackets and what follows them', () => {
    expect(toTypstMath('2α + ∂')).toBe('2 α + ∂')
    expect(toTypstMath('2+3)*4')).toBe('2 + 3 \\) dot 4')
    expect(toTypstMath('[1, 2')).toBe('\\[ 1 , 2')
    expect(toTypstMath("f'(x)")).toBe("f ' (x)")
  })

  it('typesets e notation as a power of ten', () => {
    expect(toTypstMath('1.26765060023e+30')).toBe('1.26765060023 times 10^(30)')
    expect(toTypstMath('1e-20')).toBe('1 times 10^(-20)')
  })

  it('wraps a document that imports the preamble and includes the answer', () => {
    const doc = typstDocument('x^2', '4')
    expect(doc).toContain('#import "/macros.typ": *')
    expect(doc).toContain('$ x ^ 2 = 4 $')
    expect(doc).not.toContain(TYPST_PREAMBLE)
    expect(typstDocument('   ')).toBeNull()
    expect(typstDocument('x^2-4=0', '±2', 'x')).toContain('$ x ^ 2 - 4 = 0 quad arrow.r.double quad x = #pm 2 $')
    expect(typstDocument('sys', 'x = 1, y = 2')).toContain('quad arrow.r.double quad x = 1 , y = 2')
    expect(typstAnswer('sqrt(2)/2', '0.707')).toBe('sqrt(2)/2 ≈ 0.707')
    expect(typstAnswer(undefined, '4')).toBe('4')
  })
})

describe('matrices', () => {
  const m = (body: string) => `mat(delim: "[", ${body})`
  it('typesets every way a matrix is typed or answered', () => {
    expect(toTypstMath('[[1,2],[3,4]]')).toBe(m('1, 2; 3, 4'))
    expect(toTypstMath('[1,2;3,4]')).toBe(m('1, 2; 3, 4'))
    expect(toTypstMath('[19 22; 43 50]')).toBe(m('19, 22; 43, 50'))
    expect(toTypstMath('[1 2 3]')).toBe(m('1, 2, 3'))
    expect(toTypstMath('[1; 2; 3]')).toBe(m('1; 2; 3'))
    expect(toTypstMath('[3/5 -7/10; -1/5 2/5]')).toBe(m('3 / 5, - 7 / 10; - 1 / 5, 2 / 5'))
    expect(toTypstMath('[[sin(30), 1/2],[pi, x^2]]')).toBe(m('sin(30), 1 / 2; pi, x ^ 2'))
  })

  it('keeps a comma list a list', () => {
    expect(toTypstMath('[1,2,3]')).toBe('lr(\\[1 , 2 , 3\\])')
    expect(toTypstMath('[1 + 2]')).toBe('lr(\\[1 + 2\\])')
  })

  it('writes transpose and inverse as exponents', () => {
    const a = m('1, 2; 3, 4')
    expect(toTypstMath('[[1,2],[3,4]]ᵀ')).toBe(`${a} ^T`)
    expect(toTypstMath("[[1,2],[3,4]]'")).toBe(`${a}^T`)
    expect(toTypstMath('[[1,2],[3,4]] transpose')).toBe(`${a}^T`)
    expect(toTypstMath('[[1,2],[3,4]]⁻¹')).toBe(`${a} ^(-1)`)
    expect(toTypstMath('[[1,2],[3,4]]^-1')).toBe(`${a} ^(-1)`)
    expect(toTypstMath('inv([[1,2],[3,4]])')).toBe(`${a}^(-1)`)
    expect(toTypstMath('inverse of [[1,2],[3,4]]')).toBe(`${a}^(-1)`)
    expect(toTypstMath('transpose(A B)')).toBe('(A B)^T')
    expect(toTypstMath('2^-1')).toBe('2 ^(-1)')
  })

  it('prints the matrix functions as notation', () => {
    const a = m('1, 2; 3, 4')
    expect(toTypstMath('det([[1,2],[3,4]])')).toBe(`det ${a}`)
    expect(toTypstMath('det [[1,2],[3,4]]')).toBe(`det ${a}`)
    expect(toTypstMath('trace([[1,2],[3,4]])')).toBe(`op("tr") ${a}`)
    expect(toTypstMath('identity(3)')).toBe('I_(3)')
    expect(toTypstMath('dot([1,2],[3,4])')).toBe('lr(\\[1 , 2\\]) dot lr(\\[3 , 4\\])')
  })
})

describe('typstPreviewUseful', () => {
  it.each([
    '1/3 + 2/5', 'x^2 - 5x + 6 = 0', 'integral(x^2, 0, 1)', 'sum n, n=1..9', '[1 2; 3 4]', 'det([1 2; 3 4])',
    'inv([1 2; 3 4])', 'sqrt(2)', 'nthRoot(27, 3)', 'sin^-1(0.5)', 'abs(-3)', 'sin(x)', 'cos(pi/3)',
    '5!', '2^8', 'x_1 + x_2', 'f(x) = x^3', '10 choose 3', '1/2 + 1/3', 'x/(x+1)',
  ])('shows for %s, which typesets differently', (expr) => {
    expect(typstPreviewUseful(expr)).toBe(true)
  })

  it.each([
    '2+3', '12*4', '0.5 * 3', '42', '2 pi', '2x + 3 = 7', 'x = 5', '(9.81 +/- 0.02) * (2.50 +/- 0.01)',
    '12 kg to lb', '72 f', '100 usd to eur', '$10 for lunch + 15% tip', '20% off 80', 'today + 90 days',
    '3:45pm + 4 hr 10 min', 'graph x^2', 'periodic table', 'sys 2', 'isolate T in PV = nRT', '?', '',
  ])('hides for %s, which would only say what the bar says', (expr) => {
    expect(typstPreviewUseful(expr)).toBe(false)
  })

  it.each(['3.5/2', '10/4', '-10/4', '1/3', 'sin(90)', 'ln(2)', 'log(100)', 'arccsc(2)', 'cos(-0.5)'])(
    'hides for %s, one division of two numbers or one function of a number',
    (expr) => {
      expect(typstPreviewUseful(expr)).toBe(false)
    },
  )

  it.each([
    ['int 0..1 x^2 dx', 'integral_(0)^(1) x ^ 2 dif x'],
    ['∫0..pi sin(x) dx', 'integral_(0)^(pi) sin(x) dif x'],
    ['integral of x^2 from 0 to 1', 'integral_(0)^(1) x ^ 2 dif x'],
    ['integrate t^2 from 0 to 1', 'integral_(0)^(1) t ^ 2 dif t'],
    ['sum n^2 from 1 to 10', 'sum_(n = 1)^(10) n ^ 2'],
    ['prod k from 1 to 5', 'product_(k = 1)^(5) k'],
    ['lim x->0 sin(x)/x', 'lim_(x arrow.r 0) sin(x) / x'],
    ['limit of sin(x)/x as x -> 0', 'lim_(x arrow.r 0) sin(x) / x'],
    ['solve x^2 = 4', 'x ^ 2 = 4'],
    ['solve 2x = 6', '2 x = 6'],
    ['x^2 = 4 for x', 'x ^ 2 = 4'],
    ['solve for x: x^2 = 4', 'x ^ 2 = 4'],
    ['isolate T in P*V = n*R*T', 'P dot V = n dot R dot T'],
    ['d/dx x^3', 'frac(dif, dif x) x ^ 3'],
    ['d/dx x^2 + 1', 'frac(dif, dif x) (x ^ 2 + 1)'],
    ['d/dx x^3 at 2', 'lr(frac(dif, dif x) x ^ 3 |)_(x = 2)'],
    ['d²/dx² sin(x)', 'frac(dif^2, dif x^2) sin(x)'],
    ['second derivative of t^4', 'frac(dif^2, dif t^2) t ^ 4'],
  ])('shows %s typeset as the math it means', (expr, math) => {
    expect(typstPreviewUseful(expr)).toBe(true)
    expect(previewMath(expr)).toBe(math)
  })

  it.each(['1.2*10^3 Nm / (28*10^9 Pa * pi/2 * (25*10^-3)^4)', '50 kN / 20 mm^2', '9.81 m/s^2 * (2 s)^2 / 2'])(
    'shows for %s: a unit is quoted text but still math',
    (expr) => {
      expect(typstPreviewUseful(expr)).toBe(true)
    },
  )

  it.each(['2 hr 30 min', '10 km to mi', '3:45pm + 4 hr 10 min', '5 m * 3 m'])('hides for %s', (expr) => {
    expect(typstPreviewUseful(expr)).toBe(false)
  })

  it.each([
    'gcd(12,18)', 'max(1,2)', 'mean(1,2,3)', 'lcd(4,6)', 'gcf(12,18)', 'sin(30)+1', '2≠3', '2 sin(30)',
    'max(-1.5, 2) * 3', 'gcd(12, 18) ÷ 2', 'min(max(1, 2), 3)',
  ])('hides for %s: calls of plain numbers read the same typeset', (expr) => {
    expect(typstPreviewUseful(expr)).toBe(false)
  })

  it.each(['sqrt(2) + 1', 'abs(-3)', 'floor(2.5)', 'nCr(5, 2)', 'log2(8)', 'atan(1)', 'sin(30)/2', '5! + 1', 'x_1 + 2'])(
    'shows for %s, which draws differently',
    (expr) => {
      expect(typstPreviewUseful(expr)).toBe(true)
    },
  )

  it.each(['50 mph in km/h', '5 km to mi', '3 ft + 2 in to cm', '10 kg into lb'])('hides for the conversion %s', (expr) => {
    expect(typstPreviewUseful(expr)).toBe(false)
  })

  it.each([
    ['9.8 m/s^2', '9.8 "m" / "s" ^ 2'],
    ['3 kg * 9.8 m/s^2', '3 "kg" dot 9.8 "m" / "s" ^ 2'],
    ['F = 2 kg * 3 m/s^2', 'F = 2 "kg" dot 3 "m" / "s" ^ 2'],
  ])('sets the units of %s upright', (expr, math) => {
    expect(typstPreviewUseful(expr)).toBe(true)
    expect(previewMath(expr)).toBe(math)
  })

  it('keeps a variable italic', () => {
    expect(previewMath('m^2 + x')).toBe('m ^ 2 + x')
    expect(previewMath('x^2 - 5x')).toBe('x ^ 2 - 5 x')
    expect(previewMath('integrate t^2 from 0 to 1')).toBe('integral_(0)^(1) t ^ 2 dif t')
    expect(previewMath('2H2 + O2')).toBe('2 H_(2) + O_(2)')
  })

  it.each([
    ['1e6', '1 times 10^(6)'],
    ['2.5e-3', '2.5 times 10^(-3)'],
  ])('shows scientific notation %s as a power of ten', (expr, math) => {
    expect(typstPreviewUseful(expr)).toBe(true)
    expect(previewMath(expr)).toBe(math)
  })

  it('sets an answer unit upright, like the units typed', () => {
    expect(typstDocument('5 m * 3 m', '15 m²')).toContain('15 "m²"')
    expect(typstDocument('1.2*10^3 Nm / (28*10^9 Pa)', '4.28571428571e-8 m³')).toContain('"m³"')
    expect(typstDocument('x^2 = 4', '±2', 'x')).toContain('x = #pm 2')
  })

  it('keeps the typed words when copying', () => {
    expect(copiedEquation('solve x^2 = 4', true)).toBe('"solve" x ^ 2 = 4')
  })

  it('follows the input as it is typed', () => {
    expect(['x', 'x^', 'x^2', 'x^2 -', 'x^2 - 1'].map(typstPreviewUseful)).toEqual([false, false, true, true, true])
    expect(['1', '1/', '1/x'].map(typstPreviewUseful)).toEqual([false, false, true])
  })
})

describe('a solve still being typed', () => {
  it.each([
    ['solve ( 6 (3 / (3 + R)) = 2', '(6 (3 / (3 + R)) = 2)'],
    ['solve ( 6 (3 / (3 + R)', '(6 (3 / (3 + R)))'],
    ['solve 6 (3 / (3 + R)) =', '6 (3 / (3 + R)) ='],
    ['solve 6 (3 / (3 + R', '6 (3 / (3 + R))'],
  ])('%s previews its equation so far', (expr, math) => {
    expect(typstPreviewUseful(expr)).toBe(true)
    expect(previewMath(expr).replace(/\s+/g, ' ')).toBe(math)
  })
})

describe('and / or', () => {
  it('draws & and `and` as the wedge and `or` as the vee', () => {
    expect(toTypstMath('sin(22&3)')).toBe('sin(22 and 3)')
    expect(toTypstMath('(22&3*21)22/23')).toBe('(22 and 3 dot 21) 22 / 23')
    expect(toTypstMath('12 and 3')).toBe('12 and 3')
    expect(toTypstMath('1 or 0')).toBe('1 or 0')
    expect(toTypstMath('(2>1) or (3>4)')).toBe('(2 > 1) or (3 > 4)')
  })
})

describe('every typed character previews, and every preview compiles', () => {
  it.each([
    // floor, ceiling, norm and angle brackets, and a double prime
    ['⌊x⌋', 'floor(x)'],
    ['⌈2.5⌉', 'ceil(2.5)'],
    ['‖v‖', 'norm(v)'],
    ['⟨x, y⟩', 'lr(chevron.l x , y chevron.r)'],
    ['⌊x', 'floor.l x'],
    ['f″(x)', "f '' (x)"],
    // drawn as typed
    ['€5 + £3', '€ 5 + £ 3'],
    ['¥ ¢ ‰ § ¶ —', '¥ ¢ ‰ § ¶ —'],
    ['@ ` \\ ‹ « “ ‘', '\\@ \\` backslash ‹ « “ ‘'],
    ['x^2 "', 'x ^ 2 \\"'],
    // number literals as typed
    ['0b1010', '"0b1010"'],
    ['0xff + 1/2', '"0xff" + 1 / 2'],
    ['0o17', '"0o17"'],
    // what Typst couldn't compile
    ['sin(2;3)', 'sin(2 \\; 3)'],
    ['∛', '\\∛'],
    ['√', '\\√'],
    ['∜(16)', 'root(4, 16)'],
    ['√()', 'sqrt(zws)'],
    ['2^', '2 \\^'],
    ['/3', '\\/ 3'],
    ['⁻¹', 'zws^(-1)'],
    ['_3', 'zws_(3)'],
    ['abs()', 'abs(zws)'],
    ['frac(1)', 'op("frac")(1)'],
    ['floor(1, 2)', 'op("floor")(1, 2)'],
    ['ip(x)', 'op("ip")(x)'],
    ['amat(1, 2; 3, 4)', 'amat(1, 2; 3, 4)'],
    ['cbrt(sqrt', 'root(3, op("sqrt"))'],
    ['dx_1', '#dx _(1)'],
    ['√(1, 2)', 'sqrt(lr(1 , 2))'],
    ['log(8, 2)', 'log(8, 2)'],
  ])('%s previews as %s', (expr, math) => {
    expect(previewMath(expr)).toBe(math)
  })

  it('shows for what it typesets, and not for a number literal alone', () => {
    for (const expr of ['0b1010', '0xff', '0o17', '€5 + £3', '2^']) expect(typstPreviewUseful(expr)).toBe(false)
    for (const expr of ['⌊x⌋', '‖v‖', 'sin(2;3)', '0xff + 1/2', 'x^2 "']) expect(typstPreviewUseful(expr)).toBe(true)
  })
})
