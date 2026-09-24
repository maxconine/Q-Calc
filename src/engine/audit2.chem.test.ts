// adversarial coverage for chemical equation balancing, on top of chem.test.ts. every balanced
// equation below is checked by hand-counting atoms on both sides, not by trusting the display.
import { describe, expect, it } from 'vitest'
import { balance, chemAnswer, isReactionInput, parseReaction } from './chem'
import { evaluateLine } from './evaluate'

const shown = (text: string) => evaluateLine(text).display

describe('balancing: real reactions, atom-counted by hand', () => {
  it.each([
    // 2 Al + 3 Br2 -> 2 AlBr3: Al 2=2, Br 6=6
    ['Al + Br2 -> AlBr3', '2 Al + 3 Br₂ → 2 AlBr₃'],
    // Mg3N2 + 6 H2O -> 3 Mg(OH)2 + 2 NH3: Mg 3=3, N 2=2, H 12=6*2+2*3=12, O 6=6
    ['Mg3N2 + H2O -> Mg(OH)2 + NH3', 'Mg₃N₂ + 6 H₂O → 3 Mg(OH)₂ + 2 NH₃'],
    // C2H6 + 7/2 O2 -> 2 CO2 + 3 H2O, doubled: 2 C2H6 + 7 O2 -> 4 CO2 + 6 H2O
    ['C2H6 + O2 -> CO2 + H2O', '2 C₂H₆ + 7 O₂ → 4 CO₂ + 6 H₂O'],
    // 4 FeS2 + 11 O2 -> 2 Fe2O3 + 8 SO2: Fe 4=4, S 8=8, O 22=6+16=22
    ['FeS2 + O2 -> Fe2O3 + SO2', '4 FeS₂ + 11 O₂ → 2 Fe₂O₃ + 8 SO₂'],
    // 2 KClO3 -> 2 KCl + 3 O2: K 2=2, Cl 2=2, O 6=6
    ['KClO3 -> KCl + O2', '2 KClO₃ → 2 KCl + 3 O₂'],
    // Ba(NO3)2 + Na2SO4 -> BaSO4 + 2 NaNO3
    ['Ba(NO3)2 + Na2SO4 -> BaSO4 + NaNO3', 'Ba(NO₃)₂ + Na₂SO₄ → BaSO₄ + 2 NaNO₃'],
    // 2 C4H10 + 13 O2 -> 8 CO2 + 10 H2O: C 8=8, H 20=20, O 26=16+10=26
    ['C4H10 + O2 -> CO2 + H2O', '2 C₄H₁₀ + 13 O₂ → 8 CO₂ + 10 H₂O'],
    // Pb(NO3)2 + 2 KI -> PbI2 + 2 KNO3
    ['Pb(NO3)2 + KI -> PbI2 + KNO3', 'Pb(NO₃)₂ + 2 KI → PbI₂ + 2 KNO₃'],
    // 2 Na + 2 H2O -> 2 NaOH + H2
    ['Na + H2O -> NaOH + H2', '2 Na + 2 H₂O → 2 NaOH + H₂'],
    // 4 P + 5 O2 -> 2 P2O5
    ['P + O2 -> P2O5', '4 P + 5 O₂ → 2 P₂O₅'],
    // 2 Al2O3 -> 4 Al + 3 O2
    ['Al2O3 -> Al + O2', '2 Al₂O₃ → 4 Al + 3 O₂'],
    // CaCO3 -> CaO + CO2
    ['CaCO3 -> CaO + CO2', 'CaCO₃ → CaO + CO₂'],
    // N2 + 3 H2 -> 2 NH3
    ['N2 + H2 -> NH3', 'N₂ + 3 H₂ → 2 NH₃'],
  ])('%s', (input, want) => {
    expect(shown(input)).toBe(want)
  })
})

describe('balancing: ions, with explicit coefficients already present', () => {
  it('a supplied coefficient is ignored and rebalanced from scratch', () => {
    // written with a wrong leading coefficient; the engine rebalances 2 H2 + O2 -> 2 H2O
    expect(shown('5 H2 + O2 -> H2O')).toBe('2 H₂ + O₂ → 2 H₂O')
  })

  it('conserves charge with three charged species', () => {
    // Zn + 2 Ag+ -> Zn2+ + 2 Ag: Zn 1=1, Ag 2=2, charge 2 = 2+0
    expect(shown('Zn + Ag^+ -> Zn^2+ + Ag')).toBe('Zn + 2 Ag⁺ → Zn²⁺ + 2 Ag')
  })

  it('conserves charge and atoms for a triple-charged ion', () => {
    // Al -> Al3+ + 3e- style half reaction isn't representable (no e-), so use a full redox:
    // 3 Cu^2+ + 2 Al -> 3 Cu + 2 Al^3+: Cu 3=3, Al 2=2, charge 6=6
    expect(shown('Cu^2+ + Al -> Cu + Al^3+')).toBe('3 Cu²⁺ + 2 Al → 3 Cu + 2 Al³⁺')
  })
})

describe('balancing: hydrates', () => {
  it('a hydrate with a larger multiplier', () => {
    // Na2CO3.10H2O -> Na2CO3 + 10 H2O
    expect(shown('Na2CO3.10H2O -> Na2CO3 + H2O')).toBe('Na₂CO₃·10H₂O → Na₂CO₃ + 10 H₂O')
  })
  it('a hydrate on both sides forming a different one', () => {
    // CaCl2 + 6 H2O -> CaCl2.6H2O, trivially 1:6:1
    expect(shown('CaCl2 + H2O -> CaCl2.6H2O')).toBe('CaCl₂ + 6 H₂O → CaCl₂·6H₂O')
  })
})

describe('math that must not be read as chemistry', () => {
  it.each([
    // no marked species (no digits/parens/charges/hydrates) on either side: stays math
    'a + b -> c',
    'F = ma',
    'V = IR',
    'y = mx + b',
    '2a -> 2b',
    'W = F * d',
    'p = mv',
    // = with a defined name on one side keeps it math per chemAnswer's isName guard
  ])('%s', (text) => {
    expect(chemAnswer(text)).toBeNull()
  })

  it('a defined variable named like a formula keeps math with =', () => {
    // CO2 is a stored variable here, so "CO2 = 44" must stay an assignment, not chemistry
    expect(chemAnswer('CO2 = 44', (name) => name === 'CO2')).toBeNull()
  })

  it('an arrow line that is not chemical-looking is not treated as a reaction', () => {
    expect(isReactionInput('a -> b -> c')).toBe(false)
    expect(isReactionInput('5 -> 10')).toBe(false)
  })
})

describe('impossible and underdetermined, verified independently', () => {
  it('an odd number of atoms on one side that can never balance', () => {
    // one O can never split into whole H2O molecules matching a single H2 on the other side
    expect(shown('H2 -> H2O')).toBe('')
  })
  it('mismatched elements entirely', () => {
    expect(shown('Na -> Cl')).toBe('')
  })
  it('two independent hydrocarbons can form in any ratio from the same reactants', () => {
    // C + 2H2 -> CH4 and 2C + 3H2 -> C2H6 are both balanced on their own, so the combined
    // equation has a free direction, like the C + O2 -> CO + CO2 case in chem.test.ts
    expect(shown('C + H2 -> CH4 + C2H6')).toBe('underdetermined')
  })
})

describe('copy-and-reread round trip on newly balanced reactions', () => {
  it.each([
    'Al + Br2 -> AlBr3',
    'FeS2 + O2 -> Fe2O3 + SO2',
    'Cu^2+ + Al -> Cu + Al^3+',
    'Na2CO3.10H2O -> Na2CO3 + H2O',
  ])('%s round-trips through its own display', (input) => {
    const first = shown(input)
    expect(shown(first)).toBe(first)
  })
})

describe('a plain = line that also happens to parse as a balanced reaction', () => {
  it('balances when written with = and no name collides', () => {
    expect(shown('H2 + O2 = H2O')).toBe('2 H₂ + O₂ → 2 H₂O')
  })
  it('a name collision blocks an equation that would otherwise balance', () => {
    // without the collision this balances to 4 Fe + 3 O2 -> 2 Fe2O3 (see chem.test.ts);
    // marking Fe as a stored variable name must suppress it
    expect(chemAnswer('Fe + O2 = Fe2O3', (n) => n === 'Fe')).toBeNull()
    expect(chemAnswer('Fe + O2 = Fe2O3', () => false)).not.toBeNull()
  })
})

describe('balancing: a wide batch of standard reactions, each atom-counted by hand', () => {
  it.each([
    // 2K + 2H2O -> 2KOH + H2: K2=2,O2=2,H:LHS4,RHS KOH H2+H2 H2=4
    ['K + H2O -> KOH + H2', '2 K + 2 H₂O → 2 KOH + H₂'],
    // Zn + 2HCl -> ZnCl2 + H2
    ['Zn + HCl -> ZnCl2 + H2', 'Zn + 2 HCl → ZnCl₂ + H₂'],
    // Mg + 2HCl -> MgCl2 + H2
    ['Mg + HCl -> MgCl2 + H2', 'Mg + 2 HCl → MgCl₂ + H₂'],
    // 2Al + 6HCl -> 2AlCl3 + 3H2: Al2=2,Cl6=6,H6=6
    ['Al + HCl -> AlCl3 + H2', '2 Al + 6 HCl → 2 AlCl₃ + 3 H₂'],
    // CH4 + 2O2 -> CO2 + 2H2O: C1,H4=4,O4=2+2=4
    ['CH4 + O2 -> CO2 + H2O', 'CH₄ + 2 O₂ → CO₂ + 2 H₂O'],
    // 2C2H2 + 5O2 -> 4CO2 + 2H2O: C4=4,H4=4,O10=8+2=10
    ['C2H2 + O2 -> CO2 + H2O', '2 C₂H₂ + 5 O₂ → 4 CO₂ + 2 H₂O'],
    // 2H2O2 -> 2H2O + O2
    ['H2O2 -> H2O + O2', '2 H₂O₂ → 2 H₂O + O₂'],
    // 2AgNO3 + MgCl2 -> 2AgCl + Mg(NO3)2: Ag2,N2,O6,Cl2,Mg1
    ['AgNO3 + MgCl2 -> AgCl + Mg(NO3)2', '2 AgNO₃ + MgCl₂ → 2 AgCl + Mg(NO₃)₂'],
    // BaCl2 + Na2SO4 -> BaSO4 + 2NaCl
    ['BaCl2 + Na2SO4 -> BaSO4 + NaCl', 'BaCl₂ + Na₂SO₄ → BaSO₄ + 2 NaCl'],
    // 2NaOH + H2SO4 -> Na2SO4 + 2H2O: Na2,O6=2+4=6,H4=2+2=4,S1
    ['NaOH + H2SO4 -> Na2SO4 + H2O', '2 NaOH + H₂SO₄ → Na₂SO₄ + 2 H₂O'],
    ['CaO + H2O -> Ca(OH)2', 'CaO + H₂O → Ca(OH)₂'],
    ['Mg + O2 -> MgO', '2 Mg + O₂ → 2 MgO'],
    // 4Na + O2 -> 2Na2O
    ['Na + O2 -> Na2O', '4 Na + O₂ → 2 Na₂O'],
    ['S + O2 -> SO2', 'S + O₂ → SO₂'],
    ['SO2 + O2 -> SO3', '2 SO₂ + O₂ → 2 SO₃'],
    ['SO3 + H2O -> H2SO4', 'SO₃ + H₂O → H₂SO₄'],
    ['N2 + O2 -> NO', 'N₂ + O₂ → 2 NO'],
    ['NO + O2 -> NO2', '2 NO + O₂ → 2 NO₂'],
    // 3NO2 + H2O -> 2HNO3 + NO: N3,O7=6+1=7=6+1,H2
    ['NO2 + H2O -> HNO3 + NO', '3 NO₂ + H₂O → 2 HNO₃ + NO'],
    ['C + O2 -> CO', '2 C + O₂ → 2 CO'],
    ['CO + O2 -> CO2', '2 CO + O₂ → 2 CO₂'],
    // CaCO3 + 2HCl -> CaCl2 + H2O + CO2: Ca1,C1,O3=1+2,H2,Cl2
    ['CaCO3 + HCl -> CaCl2 + H2O + CO2', 'CaCO₃ + 2 HCl → CaCl₂ + H₂O + CO₂'],
    ['Na2CO3 + HCl -> NaCl + H2O + CO2', 'Na₂CO₃ + 2 HCl → 2 NaCl + H₂O + CO₂'],
    ['KOH + H2SO4 -> K2SO4 + H2O', '2 KOH + H₂SO₄ → K₂SO₄ + 2 H₂O'],
    // Fe2O3 + 3CO -> 2Fe + 3CO2: Fe2,O6=3+3,C3
    ['Fe2O3 + CO -> Fe + CO2', 'Fe₂O₃ + 3 CO → 2 Fe + 3 CO₂'],
    // 3Fe + 4H2O -> Fe3O4 + 4H2
    ['Fe + H2O -> Fe3O4 + H2', '3 Fe + 4 H₂O → Fe₃O₄ + 4 H₂'],
    ['Al + Fe2O3 -> Al2O3 + Fe', '2 Al + Fe₂O₃ → Al₂O₃ + 2 Fe'],
    ['CuO + H2 -> Cu + H2O', 'CuO + H₂ → Cu + H₂O'],
    ['CuO + C -> Cu + CO2', '2 CuO + C → 2 Cu + CO₂'],
    ['Pb(NO3)2 + NaCl -> PbCl2 + NaNO3', 'Pb(NO₃)₂ + 2 NaCl → PbCl₂ + 2 NaNO₃'],
    ['AgNO3 + NaCl -> AgCl + NaNO3', 'AgNO₃ + NaCl → AgCl + NaNO₃'],
    // 2NaHCO3 -> Na2CO3 + H2O + CO2: Na2,H2,C2,O6=3+1+2
    ['NaHCO3 -> Na2CO3 + H2O + CO2', '2 NaHCO₃ → Na₂CO₃ + H₂O + CO₂'],
    // CaC2 + 2H2O -> Ca(OH)2 + C2H2: Ca1,C2,H4=4=2+2,O2
    ['CaC2 + H2O -> Ca(OH)2 + C2H2', 'CaC₂ + 2 H₂O → Ca(OH)₂ + C₂H₂'],
    // Al4C3 + 12H2O -> 4Al(OH)3 + 3CH4: Al4,C3,H24=24=12+12,O12
    ['Al4C3 + H2O -> Al(OH)3 + CH4', 'Al₄C₃ + 12 H₂O → 4 Al(OH)₃ + 3 CH₄'],
  ])('%s', (input, want) => {
    expect(shown(input)).toBe(want)
  })
})

describe('null space and balance as independent checks', () => {
  it('a straightforward 2-species combustion-style ratio', () => {
    const r = parseReaction('O3 -> O2')!
    const b = balance(r)
    expect(b.kind).toBe('balanced')
    // 2 O3 -> 3 O2: O 6=6
    if (b.kind === 'balanced') expect(b.coefficients).toEqual([2n, 3n])
  })
})
