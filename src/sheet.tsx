import { StrictMode, useEffect, useLayoutEffect, useRef } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './windows.css'
import { IdentityCard } from './components/IdentityCard'
import { PeriodicCard } from './components/PeriodicCard'
import { nativeHandler } from './lib/bridge'
import { identitySheetById } from './lib/identities'
import { closesSheet, fitScale, sheetFromSearch } from './lib/sheetWindow'

// the windows shell's reference sheets, each in a window of its own as on the mac: the periodic table puts what's
// clicked in the bar, and an identity sheet copies its rows
const post = (message: Record<string, unknown>) => nativeHandler()?.postMessage(message)
const close = () => post({ type: 'closeSheet' })

function PeriodicWindow() {
  const box = useRef<HTMLDivElement>(null)
  // measured once at its usual size, then scaled to whatever the window is
  useLayoutEffect(() => {
    const card = box.current?.firstElementChild as HTMLElement | null
    if (!card) return
    const natural = { width: card.offsetWidth, height: card.offsetHeight }
    const fit = () => {
      card.style.zoom = String(fitScale(natural, { width: window.innerWidth, height: window.innerHeight }))
    }
    fit()
    window.addEventListener('resize', fit)
    return () => window.removeEventListener('resize', fit)
  }, [])
  return (
    <div ref={box} className="sheet-fit">
      <PeriodicCard windowed onPick={(text) => post({ type: 'insert', text })} onClose={close} />
    </div>
  )
}

function SheetPage() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!closesSheet(e)) return
      e.preventDefault()
      close()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const which = sheetFromSearch(window.location.search)
  if (which?.kind === 'periodic') return <PeriodicWindow />
  const sheet = which?.kind === 'identities' ? identitySheetById(which.id) : null
  return sheet ? <IdentityCard windowed sheet={sheet} onClose={close} /> : null
}

document.documentElement.classList.add('sheet-window')
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SheetPage />
  </StrictMode>,
)
