import { keepFocus } from '../lib/dom'
import { toggleAngleMode, toggleFractionMode, toggleSigFigMode, type Settings } from '../lib/settings'

type Props = {
  settings: Pick<Settings, 'angleMode' | 'fractionMode' | 'sigFigMode'>
  onToggle: (toggle: (s: Settings) => Settings) => void
  onClear: () => void
}

export function EdgeTools({ settings, onToggle, onClear }: Props) {
  const { angleMode, fractionMode, sigFigMode } = settings
  return (
    <>
      <div className="edge-tools">
        <button
          type="button"
          className="edge-tool edge-angle active"
          aria-keyshortcuts="Control+D"
          aria-label={angleMode === 'deg' ? 'Degrees. Shortcut Control D' : 'Radians. Shortcut Control D'}
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleAngleMode)}
        >
          {angleMode}
        </button>
        <button
          type="button"
          className={`edge-tool edge-frac ${fractionMode ? 'active' : ''}`}
          aria-keyshortcuts="Control+F"
          aria-pressed={fractionMode}
          aria-label={fractionMode ? 'Fraction results on. Shortcut Control F' : 'Fraction results off. Shortcut Control F'}
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleFractionMode)}
        >
          <span className="edge-frac-mark" aria-hidden="true">
            <span>a</span>
            <span className="edge-frac-bar" />
            <span>b</span>
          </span>
        </button>
        <button
          type="button"
          className={`edge-tool ${sigFigMode ? 'active' : ''}`}
          aria-keyshortcuts="Control+S"
          aria-pressed={sigFigMode}
          aria-label={
            sigFigMode
              ? 'Significant figures from input on. Shortcut Control S'
              : 'Significant figures from input off. Shortcut Control S'
          }
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleSigFigMode)}
        >
          sf
        </button>
      </div>
      <button
        type="button"
        className="edge-tool edge-clear"
        aria-keyshortcuts="Control+C"
        aria-label="Clear history and variables. Shortcut Control C"
        onMouseDown={keepFocus}
        onClick={onClear}
      >
        clear
      </button>
    </>
  )
}
