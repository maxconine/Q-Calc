import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { SettingsPage } from './components/SettingsPage'
import './windows.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SettingsPage />
  </StrictMode>,
)
