import { describe, expect, it } from 'vitest'
import { balance, chemAnswer, chemCopyText, isReactionInput, nullSpace, parseReaction } from './chem'
import { evaluateLine, evaluateSheet } from './evaluate'

const shown = (text: string) => evaluateLine(text).display

describe('balancing', () => {
  it.each([
    ['C3H8 + O2 -> CO2 + H2O', 'C₃H₈ + 5 O₂ → 3 CO₂ + 4 H₂O'],
    ['Fe + O2 = Fe2O3', '4 Fe + 3 O₂ → 2 Fe₂O₃'],
    ['H2 + O2 => H2O', '2 H₂ + O₂ → 2 H₂O'],
    ['H2+O2→H2O', '2 H₂ + O₂ → 2 H₂O'],
    ['Ca(OH)2 + H3PO4 -> Ca3(PO4)2 + H2O', '3 Ca(OH)₂ + 2 H₃PO₄ → Ca₃(PO₄)₂ + 6 H₂O'],
    ['CuSO4·5H2O -> CuSO4 + H2O', 'CuSO₄·5H₂O → CuSO₄ + 5 H₂O'],
    ['CuSO4.5H2O -> CuSO4 + H2O', 'CuSO₄·5H₂O → CuSO₄ + 5 H₂O'],
    ['Al2(SO4)3 + Ca(OH)2 -> Al(OH)3 + CaSO4', 'Al₂(SO₄)₃ + 3 Ca(OH)₂ → 2 Al(OH)₃ + 3 CaSO₄'],
    ['KMnO4 + HCl -> KCl + MnCl2 + H2O + Cl2', '2 KMnO₄ + 16 HCl → 2 KCl + 2 MnCl₂ + 8 H₂O + 5 Cl₂'],
    ['C6H12O6 + O2 -> CO2 + H2O', 'C₆H₁₂O₆ + 6 O₂ → 6 CO₂ + 6 H₂O'],
    ['NH3 + O2 -> NO + H2O', '4 NH₃ + 5 O₂ → 4 NO + 6 H₂O'],
    ['Cu + HNO3 -> Cu(NO3)2 + NO + H2O', '3 Cu + 8 HNO₃ → 3 Cu(NO₃)₂ + 2 NO + 4 H₂O'],
    ['O3 -> O2', '2 O₃ → 3 O₂'],
    ['NaCl(aq) + AgNO3(aq) -> AgCl(s) + NaNO3(aq)', 'NaCl(aq) + AgNO₃(aq) → AgCl(s) + NaNO₃(aq)'],
    [
      'K4[Fe(CN)6] + KMnO4 + H2SO4 -> KHSO4 + Fe2(SO4)3 + MnSO4 + HNO3 + CO2 + H2O',
      '10 K₄[Fe(CN)₆] + 122 KMnO₄ + 299 H₂SO₄ → 162 KHSO₄ + 5 Fe₂(SO₄)₃ + 122 MnSO₄ + 60 HNO₃ + 60 CO₂ + 188 H₂O',
    ],
    ['2 H2 + 7 O2 -> 9 H2O', '2 H₂ + O₂ → 2 H₂O'],
  ])('%s', (input, want) => {
    expect(shown(input)).toBe(want)
  })

  it('conserves charge for ions', () => {
    expect(shown('MnO4^- + Fe^2+ + H^+ -> Mn^2+ + Fe^3+ + H2O')).toBe('MnO₄⁻ + 5 Fe²⁺ + 8 H⁺ → Mn²⁺ + 5 Fe³⁺ + 4 H₂O')
    expect(shown('Cr2O7^2- + Fe^2+ + H^+ -> Cr^3+ + Fe^3+ + H2O')).toBe(
      'Cr₂O₇²⁻ + 6 Fe²⁺ + 14 H⁺ → 2 Cr³⁺ + 6 Fe³⁺ + 7 H₂O',
    )
    expect(shown('Na+ + Cl- -> NaCl')).toBe('Na⁺ + Cl⁻ → NaCl')
    expect(shown('Fe^{3+} + OH^- -> Fe(OH)3')).toBe('Fe³⁺ + 3 OH⁻ → Fe(OH)₃')
    expect(shown('Fe^+3 + OH- -> Fe(OH)3')).toBe('Fe³⁺ + 3 OH⁻ → Fe(OH)₃')
    expect(shown('Ag^+ + Cu -> Ag + Cu^2+')).toBe('2 Ag⁺ + Cu → 2 Ag + Cu²⁺')
  })

  it('reads its own answer back', () => {
    for (const a of ['MnO₄⁻ + 5 Fe²⁺ + 8 H⁺ → Mn²⁺ + 5 Fe³⁺ + 4 H₂O', 'CuSO₄·5H₂O → CuSO₄ + 5 H₂O']) {
      expect(shown(a)).toBe(a)
      expect(shown(chemCopyText(a))).toBe(a)
    }
  })

  it('says underdetermined for several independent reactions', () => {
    expect(shown('H2 + O2 -> H2O + H2O2')).toBe('underdetermined')
    expect(shown('C + O2 -> CO + CO2')).toBe('underdetermined')
    // only positive with larger coefficients, like Fe + FeO + 5 Fe2O3 -> 4 Fe3O4
    expect(shown('Fe + FeO + Fe2O3 -> Fe3O4')).toBe('underdetermined')
  })

  it('leaves impossible reactions blank', () => {
    expect(shown('H2 -> O2')).toBe('')
    expect(shown('NaCl -> Na2Cl3 + Cl')).toBe('')
    expect(shown('H2O -> H2O')).toBe('')
    expect(shown('Na^+ -> Na')).toBe('')
    // ambiguous charge: Fe₂⁺ or Fe²⁺
    expect(shown('Fe2+ + Ce4+ -> Fe3+ + Ce3+')).toBe('')
    expect(isReactionInput('H2 -> O2')).toBe(true)
  })

  it('uses big integers where coefficients get large', () => {
    const r = parseReaction('C99999999999H200000000000 + O2 -> CO2 + H2O')!
    const b = balance(r)
    expect(b.kind).toBe('balanced')
    if (b.kind === 'balanced') expect(b.coefficients).toEqual([1n, 149999999999n, 99999999999n, 100000000000n])
  })
})

describe('copy text', () => {
  it('turns a shown reaction into plain text', () => {
    expect(chemCopyText('C₃H₈ + 5 O₂ → 3 CO₂ + 4 H₂O')).toBe('C3H8 + 5 O2 -> 3 CO2 + 4 H2O')
    expect(chemCopyText('MnO₄⁻ + 5 Fe²⁺ + 8 H⁺ → Mn²⁺ + 5 Fe³⁺ + 4 H₂O')).toBe(
      'MnO4^- + 5 Fe^2+ + 8 H^+ -> Mn^2+ + 5 Fe^3+ + 4 H2O',
    )
  })

  it('leaves other answers alone', () => {
    for (const t of ['42', '5 cm', '2√3', 'underdetermined', 'x → y', '10²']) expect(chemCopyText(t)).toBe(t)
  })
})

describe('never steals math', () => {
  it.each([
    'x + y = 3',
    '2 + 3 = 5',
    '2 + 3',
    'f(x) = x + 1',
    'F(X) = X',
    'H = 5',
    'CO = 3',
    'H = CO',
    'A = B',
    'X = Y + Z',
    'KE = PE',
    'E = mc^2',
    'PV = nRT',
    'C = 5 F',
    'K = C + 273',
    '100 C -> F',
    '100 C → F',
    '32 F -> C',
    '1 T -> G',
    '5 km to mi',
    '100 C to F',
    'x = 5',
    'H + CO',
    'N*m = J',
    'H2 * 2 = H4',
    'H2O - H2 -> O',
    'H2 / 2 -> H',
    'H^2 -> H2',
    'a -> b',
    'CO2 = CO2',
    'x >= 3',
    'x = y = z',
  ])('%s', (text) => {
    expect(chemAnswer(text)).toBeNull()
  })

  it('keeps variables named like elements', () => {
    const rows = evaluateSheet(['H = 2', 'CO = 3', 'H + CO', 'H2 = 2 H', 'CO2 = CO + H'])
    expect(rows.map((r) => r.display)).toEqual(['2', '3', '5', '4', '5'])
    expect(rows[3]).toMatchObject({ kind: 'assignment', variable: 'H2' })
  })

  it('keeps plain math and conversions answering as before', () => {
    expect(shown('2 + 3')).toBe('5')
    expect(shown('H = 5')).toBe('5')
    expect(evaluateLine('x + 1', { variables: { x: 2 } }).display).toBe('3')
    expect(shown('100 C to F')).toBe(evaluateLine('100 degC to degF').display)
  })

  it('treats only chemical-looking arrow lines as reactions', () => {
    expect(isReactionInput('C3H8 + O2 -> CO2 + H2O')).toBe(true)
    expect(isReactionInput('100 C -> F')).toBe(false)
    expect(isReactionInput('Fe + O2 = Fe2O3')).toBe(false)
    expect(isReactionInput('x + y = 3')).toBe(false)
  })
})

describe('null space', () => {
  it('finds the integer kernel', () => {
    expect(nullSpace([[1n, -1n]], 2)).toEqual([[1n, 1n]])
    expect(nullSpace([[1n, 0n], [0n, 1n]], 2)).toEqual([])
    expect(nullSpace([[2n, 4n, -6n]], 3)).toHaveLength(2)
  })
})

// brute force over small coefficients as an independent check of the elimination
describe('against brute force', () => {
  const pool = ['H2', 'O2', 'H2O', 'CO2', 'CO', 'C', 'CH4', 'C2H6', 'C3H8', 'NH3', 'NO2', 'NO', 'HNO3', 'H2O2', 'O3', 'CH3OH', 'Fe', 'Fe2O3', 'FeO', 'Fe3O4']
  const atomsOf = (f: string) => parseReaction(`${f} -> ${f}`)!.left[0]!.atoms
  let seed = 7
  const rand = (n: number) => {
    seed = (seed * 48271) % 2147483647
    return seed % n
  }

  const proportional = (a: number[], b: number[]) => a.every((x, i) => x * b[0]! === b[i]! * a[0]!)

  function rank(rows: number[][]): number {
    const m = rows.map((r) => [...r])
    let r = 0
    for (let c = 0; c < (m[0]?.length ?? 0) && r < m.length; c++) {
      const p = m.findIndex((row, i) => i >= r && Math.abs(row[c]!) > 1e-9)
      if (p < 0) continue
      ;[m[r], m[p]] = [m[p]!, m[r]!]
      for (let i = r + 1; i < m.length; i++) {
        const f = m[i]![c]! / m[r]![c]!
        m[i] = m[i]!.map((x, j) => x - f * m[r]![j]!)
      }
      r++
    }
    return r
  }

  function search(rows: number[][], k: number, max: number): number[][] {
    const found: number[][] = []
    const c = new Array<number>(k).fill(1)
    for (;;) {
      let ok = true
      for (let r = 0; ok && r < rows.length; r++) {
        let sum = 0
        for (let i = 0; i < k; i++) sum += rows[r]![i]! * c[i]!
        ok = sum === 0
      }
      if (ok) found.push([...c])
      let i = 0
      while (i < k && c[i] === max) c[i++] = 1
      if (i === k) return found
      c[i]!++
    }
  }

  it('agrees on 150 random balanceable reactions and every impossible one it meets', () => {
    const tally = { balanced: 0, impossible: 0, underdetermined: 0 }
    for (let t = 0; t < 20000 && tally.balanced < 150; t++) {
      const k = 3 + rand(2)
      const picks = new Set<string>()
      while (picks.size < k) picks.add(pool[rand(pool.length)]!)
      const list = [...picks]
      const cut = 1 + rand(k - 1)
      const text = `${list.slice(0, cut).join(' + ')} -> ${list.slice(cut).join(' + ')}`
      const els = [...new Set(list.flatMap((f) => [...atomsOf(f).keys()]))]
      const rows = els.map((el) => list.map((f, i) => Number(atomsOf(f).get(el) ?? 0n) * (i < cut ? 1 : -1)))
      const max = k === 3 ? 12 : 8
      const solutions = search(rows, k, max)
      const b = balance(parseReaction(text)!)
      tally[b.kind]++
      if (b.kind === 'balanced') {
        const coef = b.coefficients.map(Number)
        if (Math.max(...coef) <= max) expect(solutions[0], text).toEqual(coef)
        for (const sol of solutions) expect(proportional(sol, coef), text).toBe(true)
      } else if (b.kind === 'impossible') {
        expect(solutions, text).toEqual([])
      } else {
        expect(search(rows, k, 16).length, text).toBeGreaterThan(0)
        expect(k - rank(rows), text).toBeGreaterThan(1)
      }
    }
    expect(tally.balanced).toBe(150)
    expect(tally.impossible).toBeGreaterThan(0)
    expect(tally.underdetermined).toBeGreaterThan(0)
  })
})

describe('more reactions', () => {
  it.each([
    ['N2 + H2 -> NH3', 'N₂ + 3 H₂ → 2 NH₃'],
    ['H2O2 -> H2O + O2', '2 H₂O₂ → 2 H₂O + O₂'],
    ['2H2+O2->2H2O', '2 H₂ + O₂ → 2 H₂O'],
    ['Ba(OH)2 + HNO3 -> Ba(NO3)2 + H2O', 'Ba(OH)₂ + 2 HNO₃ → Ba(NO₃)₂ + 2 H₂O'],
    ['CH4(g) + O2(g) -> CO2(g) + H2O(g)', 'CH₄(g) + 2 O₂(g) → CO₂(g) + 2 H₂O(g)'],
    ['Al + HCl -> AlCl3 + H2', '2 Al + 6 HCl → 2 AlCl₃ + 3 H₂'],
    ['P4 + O2 -> P4O10', 'P₄ + 5 O₂ → P₄O₁₀'],
    ['SO2 + O2 -> SO3', '2 SO₂ + O₂ → 2 SO₃'],
    ['C2H5OH + O2 -> CO2 + H2O', 'C₂H₅OH + 3 O₂ → 2 CO₂ + 3 H₂O'],
    ['Fe(s) + CuSO4(aq) -> FeSO4(aq) + Cu(s)', 'Fe(s) + CuSO₄(aq) → FeSO₄(aq) + Cu(s)'],
  ])('%s', (input, want) => {
    expect(shown(input)).toBe(want)
  })

  it('a typed coefficient is ignored and the equation is balanced again', () => {
    expect(shown('0 H2 + O2 -> H2O')).toBe('2 H₂ + O₂ → 2 H₂O')
    expect(shown('9 H2 + 9 O2 -> 1 H2O')).toBe('2 H₂ + O₂ → 2 H₂O')
  })

  it('lowercase is not a formula, so the line is left to math', () => {
    expect(chemAnswer('h2 + o2 -> h2o')).toBeNull()
  })

  it('a repeated species is impossible', () => {
    expect(shown('H2 + H2 -> H2')).toBe('')
    expect(shown('H2 + O2 -> H2 + O2')).toBe('')
  })

  it('a half-typed arrow is not a reaction', () => {
    expect(chemAnswer('H2 + O2 ->')).toBeNull()
    expect(chemAnswer('H2 +')).toBeNull()
  })

  it('states and a hydrate can sit on the same species', () => {
    expect(shown('CuSO4·5H2O(s) -> CuSO4(s) + H2O(g)')).toBe('CuSO₄·5H₂O(s) → CuSO₄(s) + 5 H₂O(g)')
  })
})
