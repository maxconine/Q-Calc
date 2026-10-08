import { keepFocus } from '../lib/dom'

type Letters = { varying: string[]; constant: string[]; locked: string[] }

type Props = {
  letters: Letters
  /** Move a letter to the other side: `varying` is where it should end up. */
  onToggle: (letter: string, varying: boolean) => void
}

// under a d/dt: which letters the answer took as changing with time, each a chip that moves it to the other side
export function TimeLetters({ letters, onToggle }: Props) {
  const chip = (letter: string, varying: boolean) => {
    const locked = letters.locked.includes(letter)
    return (
      <button
        key={letter}
        type="button"
        className={`time-chip ${varying ? 'varying' : 'constant'}${locked ? ' locked' : ''}`}
        disabled={locked}
        title={
          locked
            ? `${letter} is written with a dot, so it changes with time`
            : varying
              ? `Treat ${letter} as a constant`
              : `Treat ${letter} as changing with time`
        }
        aria-label={`${letter}: ${varying ? 'changing with time' : 'constant'}${locked ? '' : '. Click to switch'}`}
        onMouseDown={keepFocus}
        onClick={() => onToggle(letter, !varying)}
      >
        {letter}
      </button>
    )
  }
  return (
    <div className="time-letters" role="group" aria-label="Which letters change with time">
      {letters.varying.length ? (
        <>
          <span className="time-label">changing with time</span>
          {letters.varying.map((l) => chip(l, true))}
        </>
      ) : null}
      {letters.varying.length && letters.constant.length ? <span className="time-label">·</span> : null}
      {letters.constant.length ? (
        <>
          <span className="time-label">constant</span>
          {letters.constant.map((l) => chip(l, false))}
        </>
      ) : null}
    </div>
  )
}
