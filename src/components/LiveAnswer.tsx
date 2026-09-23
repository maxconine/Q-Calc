import { isImproperUnitConversion } from '../engine/units'
import { hasDualAnswer } from '../lib/answer'
import { keepFocus } from '../lib/dom'

type Props = {
  copied: boolean
  example: { tick: number; answer: string } | null
  display: string
  exact: string | undefined
  shown: string
  onCopy: () => void
  onCopyExact: () => void
  onCopyApprox: () => void
}

export function LiveAnswer({ copied, example, display, exact, shown, onCopy, onCopyExact, onCopyApprox }: Props) {
  if (copied) {
    return (
      <button type="button" className="live copied" disabled>
        copied
      </button>
    )
  }
  if (example) {
    return (
      <span key={example.tick} className="live live-example" aria-hidden>
        {example.answer}
      </span>
    )
  }
  if (isImproperUnitConversion(display)) {
    return (
      <button type="button" className="live message" disabled>
        {display}
      </button>
    )
  }
  if (exact && hasDualAnswer({ display, exact })) {
    return (
      <div className="live-dual" role="group" aria-label="Answer">
        <button type="button" className="live live-part" title="Copy exact value" onMouseDown={keepFocus} onClick={onCopyExact}>
          {exact}
        </button>
        <span className="live-eq" aria-hidden>
          ≈
        </span>
        <button type="button" className="live live-part" title="Copy approximation" onMouseDown={keepFocus} onClick={onCopyApprox}>
          {display}
        </button>
      </div>
    )
  }
  return (
    <button
      type="button"
      className={`live ${shown ? '' : 'empty'}`}
      title={shown ? 'Copy to clipboard · ⌘C also copies' : undefined}
      disabled={!shown}
      onMouseDown={keepFocus}
      onClick={onCopy}
    >
      {shown}
    </button>
  )
}
