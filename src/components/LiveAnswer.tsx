import { useEffect, useState, type DragEvent } from 'react'
import { prettyAnswer } from '../engine/format'
import { isImproperUnitConversion } from '../engine/units'
import { hasDualAnswer, prettyRoots } from '../lib/answer'
import { useFitFont } from './useFitFont'
import { RadicalText } from './Radical'
import { answerParts } from '../lib/radical'
import type { SteadyAnswer } from './useSteadyAnswer'

type Props = {
  copied: boolean
  example: { tick: number; answer: string } | null
  display: string
  exact: string | undefined
  shown: string
  // the last real answer, dimmed, while the input is briefly invalid
  steady: SteadyAnswer | null
  // bumps on each tab through the answer's forms
  formTick: number
  // what each side of a dual answer copies and drags
  sides: { exact: string; approx: string }
  onCopy: (text: string) => void
  onRefocus: () => void
  // a solved equation: the letter shown before its roots, or a message with no roots
  label?: string
  message?: boolean
}

const CHECK = (
  <span className="live-check" aria-hidden>
    ✓
  </span>
)

// a drag needs the mousedown default, so these buttons can't keep focus by cancelling it; they hand it back instead
function answerHandle(text: string, onCopy: (text: string) => void, onRefocus: () => void) {
  return {
    draggable: Boolean(text),
    onDragStart: (e: DragEvent<HTMLElement>) => {
      e.dataTransfer.setData('text/plain', text)
      e.dataTransfer.effectAllowed = 'copy'
    },
    onDragEnd: onRefocus,
    onMouseUp: onRefocus,
    onClick: () => {
      onCopy(text)
      onRefocus()
    },
  }
}

export function LiveAnswer({ copied, example, display, exact, shown, steady, formTick, sides, onCopy, onRefocus, label, message }: Props) {
  const pretty = label ? prettyRoots(shown) : prettyAnswer(shown)
  const fitRef = useFitFont<HTMLButtonElement>(pretty)
  // the half that was clicked; ⌘C copies whichever half is the shown answer
  const [clicked, setClicked] = useState<'exact' | 'approx' | null>(null)
  useEffect(() => {
    if (!copied) setClicked(null)
  }, [copied])
  if (example && !copied) {
    return (
      <span key={example.tick} className="live live-example" aria-hidden>
        {answerParts(prettyAnswer(example.answer)).map((part, i) => (
          <RadicalText key={i} text={part} answer />
        ))}
      </span>
    )
  }
  const varLabel = label ? (
    <span className="live-var" aria-hidden>
      {label} =
    </span>
  ) : null
  if (message || isImproperUnitConversion(display)) {
    return (
      <button type="button" className="live message" disabled>
        {display}
      </button>
    )
  }
  if (exact && hasDualAnswer({ display, exact })) {
    const copiedSide = copied ? (clicked ?? (shown === exact ? 'exact' : 'approx')) : null
    return (
      <div className="live-dual" role="group" aria-label="Answer">
        {copied ? CHECK : null}
        {varLabel}
        <button
          type="button"
          className={`live live-part${copiedSide === 'exact' ? ' copied' : ''}`}
          title="Copy exact value"
          {...answerHandle(sides.exact, onCopy, onRefocus)}
          onClick={() => {
            setClicked('exact')
            onCopy(sides.exact)
            onRefocus()
          }}
        >
          <RadicalText text={label ? prettyRoots(exact) : prettyAnswer(exact)} answer />
        </button>
        <span className="live-eq" aria-hidden>
          ≈
        </span>
        <button
          type="button"
          className={`live live-part${copiedSide === 'approx' ? ' copied' : ''}`}
          title="Copy approximation"
          {...answerHandle(sides.approx, onCopy, onRefocus)}
          onClick={() => {
            setClicked('approx')
            onCopy(sides.approx)
            onRefocus()
          }}
        >
          {label ? prettyRoots(display) : prettyAnswer(display)}
        </button>
      </div>
    )
  }
  if (!shown && steady) {
    // not a button: a held answer belongs to earlier input, so it can't be copied, dragged or committed
    return (
      <span className={`live live-steady${steady.fading ? ' fading' : ''}`} aria-hidden>
        {answerParts(prettyAnswer(steady.text)).map((part, i) => (
          <RadicalText key={i} text={part} answer />
        ))}
      </span>
    )
  }
  return (
    <button
      key={formTick}
      ref={fitRef}
      type="button"
      className={`live ${shown ? '' : 'empty'}${formTick ? ' live-cycled' : ''}${copied ? ' copied' : ''}`}
      title={shown ? 'Copy to clipboard · ⌘C also copies' : undefined}
      disabled={!shown}
      {...answerHandle(shown, onCopy, onRefocus)}
    >
      <span className="live-text">
        {copied ? CHECK : null}
        {varLabel}
        <RadicalText text={pretty} answer />
      </span>
    </button>
  )
}
