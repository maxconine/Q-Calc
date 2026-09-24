import type { DragEvent } from 'react'
import { prettyAnswer } from '../engine/format'
import { isImproperUnitConversion } from '../engine/units'
import { hasDualAnswer, prettyRoots } from '../lib/answer'
import { useFitFont } from './useFitFont'
import { RadicalText } from './Radical'
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
  if (example && !copied) {
    return (
      <span key={example.tick} className="live live-example" aria-hidden>
        {prettyAnswer(example.answer)}
      </span>
    )
  }
  const letter = label ? (
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
    return (
      <div className={`live-dual ${copied ? 'copied' : ''}`} role="group" aria-label="Answer">
        {copied ? CHECK : null}
        {letter}
        <button type="button" className="live live-part" title="Copy exact value" {...answerHandle(sides.exact, onCopy, onRefocus)}>
          <RadicalText text={label ? prettyRoots(exact) : prettyAnswer(exact)} answer />
        </button>
        <span className="live-eq" aria-hidden>
          ≈
        </span>
        <button type="button" className="live live-part" title="Copy approximation" {...answerHandle(sides.approx, onCopy, onRefocus)}>
          {label ? prettyRoots(display) : prettyAnswer(display)}
        </button>
      </div>
    )
  }
  if (!shown && steady) {
    // not a button: a held answer belongs to earlier input, so it can't be copied, dragged or committed
    return (
      <span className={`live live-steady${steady.fading ? ' fading' : ''}`} aria-hidden>
        <RadicalText text={prettyAnswer(steady.text)} answer />
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
        {letter}
        <RadicalText text={pretty} answer />
      </span>
    </button>
  )
}
