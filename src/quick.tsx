import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { QuickCalcPage } from './components/QuickCalc'
import { installGameSaves } from './lib/gameSaves'
import './windows.css'

// before the first render, so every game reads its save back
installGameSaves()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QuickCalcPage />
  </StrictMode>,
)
