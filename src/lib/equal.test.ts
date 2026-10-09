import { describe, expect, it } from 'vitest'
import { gameCommand, gameHint } from './games'
import {
  currentStreak,
  EQUAL_HINT,
  dailyAnswer,
  dailyNumber,
  dayBefore,
  emptyStats,
  evaluate,
  generate,
  guessProblem,
  isEqualCommand,
  isValidEquation,
  keyMarks,
  LEN,
  recordDaily,
  rng,
  score,
  shareText,
} from './equal'

describe('evaluate', () => {
  it('does * and / before + and -', () => {
    expect(evaluate('9*8-3')).toBe(69)
    expect(evaluate('2+3*4')).toBe(14)
    expect(evaluate('20-12/4')).toBe(17)
    expect(evaluate('1-9+9')).toBe(1)
    expect(evaluate('8/4*3')).toBe(6)
  })

  it('only divides exactly', () => {
    expect(evaluate('7/2')).toBeNull()
    expect(evaluate('5/0')).toBeNull()
    expect(evaluate('7/2*2')).toBeNull()
  })
})

describe('guessProblem', () => {
  it('takes true 8-character equations', () => {
    for (const g of ['12+35=47', '9*8-3=69', '100/4=25', '2+3*4=14', '36/4-2=7', '13-9*1=4', '1-9+10=2'])
      expect(guessProblem(g), g).toBeNull()
    expect(isValidEquation('12+35=47')).toBe(true)
  })

  it('takes a legit 0 on the right', () => {
    expect(guessProblem('12-3*4=0')).toBeNull()
    expect(guessProblem('99-99=00')).toBe('the right side is just a number')
  })

  it('gets the precedence right', () => {
    expect(guessProblem('2+3*4=14')).toBeNull()
    expect(guessProblem('2+3*4=20')).toBe('that doesn’t add up')
    expect(guessProblem('9-12/4=6')).toBeNull()
  })

  it('wants exactly one =', () => {
    expect(guessProblem('1+1=2=2+')).toBe('it needs exactly one =')
    expect(guessProblem('12+35+47')).toBe('it needs exactly one =')
  })

  it('refuses leading zeros', () => {
    expect(guessProblem('01+46=47')).toBe('no leading zeros')
    expect(guessProblem('40+07=47')).toBe('no leading zeros')
    expect(guessProblem('12+35=07')).toBe('the right side is just a number')
  })

  it('only divides exactly', () => {
    expect(guessProblem('7/2*4=14')).toBe('division has to come out exact')
    expect(guessProblem('5/0+10=6')).toBe('division has to come out exact')
    expect(guessProblem('8/2*3=12')).toBeNull()
  })

  it('refuses false sums and bad shapes', () => {
    expect(guessProblem('12+35=48')).toBe('that doesn’t add up')
    expect(guessProblem('+12+3=15')).toBe('that isn’t a sum')
    expect(guessProblem('12++3=15')).toBe('that isn’t a sum')
    expect(guessProblem('1+2=-1+4')).toBe('the right side is just a number')
    expect(guessProblem('12345=12')).toBe('the left side needs a sum')
    expect(guessProblem('12+3=15')).toBe('not enough characters')
    expect(guessProblem('12+3a=15')).toBe('only digits, + - × ÷ and =')
  })
})

describe('score', () => {
  const s = (g: string, a: string) =>
    score(g, a)
      .map((m) => (m === 'hit' ? 'G' : m === 'near' ? 'Y' : '.'))
      .join('')

  it('marks right place, elsewhere and absent', () => {
    expect(s('12+35=47', '12+35=47')).toBe('GGGGGGGG')
    expect(s('21+35=56', '12+35=47')).toBe('YYGGGG..')
  })

  it('greys a repeat once the answer has no copy left', () => {
    // the answer's only 2 is matched in place, so the 2s elsewhere are grey; the spare 4 yellows the last tile
    expect(s('22+22=44', '12+34=46')).toBe('.GG..GGY')
    // two 1s in the guess, but the answer's 1s are both used by greens
    expect(s('11+11=22', '12+10=22')).toBe('G.GG.GGG')
  })

  it('lets a later green take the copy before an earlier yellow can', () => {
    // one 4 and one 3 in the answer, both matched in place: the stray 4 and 3 go grey
    expect(s('14+33=47', '12+35=47')).toBe('G.GG.GGG')
  })

  it('yellows a repeat as often as the answer has spare copies, and no more', () => {
    // answer has one spare 9: the first stray 9 is yellow, the second grey
    expect(s('9*9-9=72', '9*8-3=69')).toBe('GGYG.G..')
    // answer has two spare 2s: both stray 2s are yellow
    expect(s('21+12=33', '11+11=22')).toBe('YGGGYG..')
  })
})

describe('keyMarks', () => {
  it('keeps the best mark each key has had', () => {
    const k = keyMarks(['21+35=56', '12+35=47'], '12+35=47')
    expect(k['1']).toBe('hit')
    expect(k['6']).toBe('miss')
    expect(k['+']).toBe('hit')
  })
})

describe('generate', () => {
  it('always makes a true 8-character equation, with no 0 answers', () => {
    const rand = rng(42)
    const seen = new Set<string>()
    for (let i = 0; i < 2000; i++) {
      const eq = generate(rand)
      expect(eq).toHaveLength(LEN)
      expect(guessProblem(eq), eq).toBeNull()
      expect(eq.split('=')[1]).not.toBe('0')
      seen.add(eq)
    }
    // and plenty of different ones
    expect(seen.size).toBeGreaterThan(1500)
  })

  it('uses every operator now and then', () => {
    const rand = rng(7)
    const all = Array.from({ length: 500 }, () => generate(rand)).join('')
    for (const op of '+-*/') expect(all).toContain(op)
  })
})

describe('the daily', () => {
  it('is the same for a date and changes with it', () => {
    expect(dailyAnswer('2026-10-09')).toBe(dailyAnswer('2026-10-09'))
    const days = ['2026-10-09', '2026-10-10', '2026-10-11', '2026-10-12', '2027-01-01']
    expect(new Set(days.map(dailyAnswer)).size).toBe(days.length)
    for (const d of days) expect(isValidEquation(dailyAnswer(d))).toBe(true)
  })

  it('numbers days and steps back across months', () => {
    expect(dailyNumber('2026-01-01')).toBe(1)
    expect(dailyNumber('2026-10-09')).toBe(282)
    expect(dayBefore('2026-03-01')).toBe('2026-02-28')
    expect(dayBefore('2027-01-01')).toBe('2026-12-31')
  })
})

describe('stats', () => {
  const a = '12+35=47'
  it('keeps the streak across days in a row and drops it on a loss or a gap', () => {
    let st = recordDaily(emptyStats(), '2026-10-07', ['21+35=56', a], a)
    expect(st).toMatchObject({ played: 1, wins: 1, streak: 1, best: 1, lastWin: '2026-10-07' })
    expect(st.dist[1]).toBe(1)
    st = recordDaily(st, '2026-10-08', [a], a)
    expect(st.streak).toBe(2)
    expect(currentStreak(st, '2026-10-09')).toBe(2)
    expect(currentStreak(st, '2026-10-10')).toBe(0)
    const lost = recordDaily(st, '2026-10-09', Array(6).fill('21+35=56'), a)
    expect(lost).toMatchObject({ played: 3, wins: 2, streak: 0, best: 2 })
    const gap = recordDaily(st, '2026-10-11', [a], a)
    expect(gap.streak).toBe(1)
  })
})

describe('shareText', () => {
  it('is a title, a count and the coloured rows', () => {
    expect(shareText(['21+35=56', '12+35=47'], '12+35=47', 'equal #282')).toBe('equal #282 2/6\n\n🟨🟨🟩🟩🟩🟩⬛⬛\n🟩🟩🟩🟩🟩🟩🟩🟩')
    expect(shareText(Array(6).fill('21+35=56'), '12+35=47', 'equal').split('\n')[0]).toBe('equal X/6')
  })
})

describe('isEqualCommand', () => {
  it('takes the name and the alias, whole', () => {
    for (const t of ['equal', 'Equal', ' EQUAL ', 'nerdle']) expect(isEqualCommand(t)).toBe(true)
    for (const t of ['equals', 'eq', 'equal 2', '1=1', 'nerd', '']) expect(isEqualCommand(t)).toBe(false)
  })

  it('opens from the bar with no peer link', () => {
    expect(gameCommand('equal')).toBe('equal')
    expect(gameCommand('Nerdle')).toBe('equal')
    expect(gameHint('equal', false)).toBe(EQUAL_HINT)
  })
})
