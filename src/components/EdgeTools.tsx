import { nativeHandler } from '../lib/bridge'
import { keepFocus } from '../lib/dom'
import type { KeyAction } from '../lib/keybinds'
import { toggleAngleMode, toggleFractionMode, toggleSigFigMode, type Settings } from '../lib/settings'

type Props = {
  settings: Pick<Settings, 'angleMode' | 'fractionMode' | 'sigFigMode'>
  // the chosen keys, for screen readers and the hover title
  keyLabels: Record<KeyAction, string>
  onToggle: (toggle: (s: Settings) => Settings) => void
  onClear: () => void
}

function shortcut(label: string): string {
  return label ? `. Shortcut ${label}` : ''
}

export function EdgeTools({ settings, keyLabels, onToggle, onClear }: Props) {
  const { angleMode, fractionMode, sigFigMode } = settings
  // only a host has a settings window to open
  const host = nativeHandler()
  return (
    <>
      {host ? (
        <button
          type="button"
          className="edge-tool edge-settings"
          aria-label={`Settings${shortcut(keyLabels.settings)}`}
          title={keyLabels.settings ? `Settings (${keyLabels.settings})` : 'Settings'}
          onMouseDown={keepFocus}
          onClick={() => host.postMessage({ type: 'openSettings' })}
        >
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path
              fill="currentColor"
              fillRule="evenodd"
              d="M6.7 1h2.6l.4 1.9c.4.2.8.4 1.2.7l1.8-.6 1.3 2.2-1.4 1.3a5 5 0 0 1 0 1.4l1.4 1.3-1.3 2.2-1.8-.6c-.4.3-.8.5-1.2.7L9.3 15H6.7l-.4-1.9c-.4-.2-.8-.4-1.2-.7l-1.8.6L2 10.8l1.4-1.3a5 5 0 0 1 0-1.4L2 6.8l1.3-2.2 1.8.6c.4-.3.8-.5 1.2-.7L6.7 1ZM8 10.4a2.4 2.4 0 1 0 0-4.8 2.4 2.4 0 0 0 0 4.8Z"
            />
          </svg>
        </button>
      ) : null}
      <div className="edge-tools">
        <button
          type="button"
          className="edge-tool edge-angle active"
          aria-label={`${angleMode === 'deg' ? 'Degrees' : 'Radians'}${shortcut(keyLabels.angle)}`}
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleAngleMode)}
        >
          {angleMode}
        </button>
        <button
          type="button"
          className={`edge-tool edge-frac ${fractionMode ? 'active' : ''}`}
          aria-pressed={fractionMode}
          aria-label={`Fraction results ${fractionMode ? 'on' : 'off'}${shortcut(keyLabels.fraction)}`}
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
          aria-pressed={sigFigMode}
          aria-label={`Significant figures from input ${sigFigMode ? 'on' : 'off'}${shortcut(keyLabels.sigFigs)}`}
          onMouseDown={keepFocus}
          onClick={() => onToggle(toggleSigFigMode)}
        >
          sf
        </button>
      </div>
      <button
        type="button"
        className="edge-tool edge-clear"
        aria-label={`Clear history and variables${shortcut(keyLabels.clear)}`}
        onMouseDown={keepFocus}
        onClick={onClear}
      >
        clear
      </button>
    </>
  )
}
