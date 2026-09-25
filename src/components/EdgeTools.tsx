import { keepFocus } from '../lib/dom'
import { isWindowsHost } from '../lib/platform'
import { toggleAngleMode, toggleFractionMode, toggleSigFigMode, type Settings } from '../lib/settings'

type Props = {
  settings: Pick<Settings, 'angleMode' | 'fractionMode' | 'sigFigMode'>
  onToggle: (toggle: (s: Settings) => Settings) => void
  onClear: () => void
}

export function EdgeTools({ settings, onToggle, onClear }: Props) {
  const { angleMode, fractionMode, sigFigMode } = settings
  const clearKeys = isWindowsHost() ? ['Control+Shift+Backspace', 'Control Shift Backspace'] : ['Control+C', 'Control C']
  return (
    <>
      <div className="edge-tools">
        <button
          type="button"
          className="edge-tool edge-angle active"
          aria-keyshortcuts="Control+D"
          aria-label={`${angleMode === 'deg' ? 'Degrees' : 'Radians'}. Shortcut Control D`}
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
          aria-label={`Fraction results ${fractionMode ? 'on' : 'off'}. Shortcut Control F`}
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
          aria-label={`Significant figures from input ${sigFigMode ? 'on' : 'off'}. Shortcut Control S`}
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleSigFigMode)}
        >
          sf
        </button>
      </div>
      <button
        type="button"
        className="edge-tool edge-clear"
        aria-keyshortcuts={clearKeys[0]}
        aria-label={`Clear history and variables. Shortcut ${clearKeys[1]}`}
        onMouseDown={keepFocus}
        onClick={onClear}
      >
        clear
      </button>
    </>
  )
}
