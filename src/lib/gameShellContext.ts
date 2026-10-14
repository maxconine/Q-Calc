import { createContext, useContext } from 'react'
import type { GameFullscreen, GameFullscreenOptions } from '../components/useGameFullscreen'
import type { ShellMode } from './gameShell'

// what a game's lobby tells the shell around it (components/GameShell.tsx), and what it can ask of it

export type ShellHooks = {
  // one step back through the lobby; false on its first screen, where esc closes the game
  back: () => boolean
  // back to the lobby from a running game
  leave: () => void
  // nothing is running, so esc leaves without pausing
  idle: () => boolean
  // the link must outlive the window hiding: someone's waiting to join, or playing
  hold: () => boolean
}

export type GameShellApi = {
  setMode: (mode: ShellMode) => void
  // returns the unregister
  register: (hooks: ShellHooks) => () => void
  toggleFullscreen: () => void
  // a game with a picture of its own shape (ember) says how fullscreen lays it out; returns the undo
  fullscreenLayout: (opts: GameFullscreenOptions) => () => void
  // what esc does, for an esc button on the page
  escape: () => void
}

export const ShellContext = createContext<GameShellApi | null>(null)
// the shell's useGameFullscreen: whether it's on, its hint, and the layout
export const FullscreenContext = createContext<GameFullscreen<HTMLDivElement> | null>(null)

export const useGameShell = () => useContext(ShellContext)
export const useShellFullscreen = () => useContext(FullscreenContext)
