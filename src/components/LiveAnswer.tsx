import { Fragment, useEffect, useState, type DragEvent } from 'react'
import { prettyAnswer } from '../engine/format'
import { isImproperUnitConversion } from '../engine/units'
import { hasDualAnswer, prettyRoots } from '../lib/answer'
import { useFitFont } from './useFitFont'
import { RadicalText } from './Radical'
import { answerParts } from '../lib/radical'
import { hostKeys } from '../lib/platform'
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
  // a solve with several roots, each as it copies: each root is its own click, and the letter before them copies all
  roots?: string[] | null
}

// says so in words, and screen readers hear it too
const CHECK = (
  <span className="live-check" role="status">
    ✓ copied
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

export function LiveAnswer({ copied, example, display, exact, shown, steady, formTick, sides, onCopy, onRefocus, label, message, roots }: Props) {
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
  if (label && roots && roots.length > 1 && shown && !(exact && hasDualAnswer({ display, exact }))) {
    // each root reads as the answer shows it, and copies at full precision
    const parts = shown.split(', ').filter((part) => part !== '…')
    const labels = parts.length === roots.length ? parts : roots
    return (
      <div className={`live-roots${copied ? ' copied' : ''}`} role="group" aria-label="Solutions">
        {copied ? CHECK : null}
        <button type="button" className="live-var live-all" title="Copy all the solutions" {...answerHandle(shown, onCopy, onRefocus)}>
          {label} =
        </button>
        {roots.map((root, k) => (
          <Fragment key={k}>
            {k ? <span className="live-root-sep">, </span> : null}
            <button type="button" className="live live-root" title="Copy this solution" {...answerHandle(root, onCopy, onRefocus)}>
              {prettyRoots(labels[k]!)}
            </button>
          </Fragment>
        ))}
        {parts.length < shown.split(', ').length ? <span className="live-root-sep">, …</span> : null}
      </div>
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
  // the mark hangs off the wrapper, outside the button: the button clips (for the ellipsis), and a short answer's
  // button hadn't room to its left for the whole mark
  return (
    <span className="live-single">
      {copied ? CHECK : null}
      <button
        key={formTick}
        ref={fitRef}
        type="button"
        className={`live ${shown ? '' : 'empty'}${formTick ? ' live-cycled' : ''}${copied ? ' copied' : ''}`}
        title={shown ? hostKeys('Copy to clipboard · ⌘C also copies') : undefined}
        disabled={!shown}
        {...answerHandle(shown, onCopy, onRefocus)}
      >
        <span className="live-text">
          {varLabel}
          <RadicalText text={pretty} answer />
        </span>
      </button>
    </span>
  )
}
