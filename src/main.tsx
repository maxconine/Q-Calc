import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { QuickCalc } from './components/QuickCalc'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <div className="page">
      <div>
        <QuickCalc onClose={() => undefined} />
        <p className="page-hint">
          Mac app: <kbd>⌃</kbd> <kbd>⌥</kbd> <kbd>Space</kbd> to show · <kbd>esc</kbd> to hide · <kbd>⌘C</kbd> copies the
          answer · <kbd>⌃C</kbd> clears history
        </p>
      </div>
    </div>
  </StrictMode>,
)
