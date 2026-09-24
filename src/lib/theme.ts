export type Theme = 'system' | 'light' | 'dark'

export const THEME_OPTIONS: Array<{ id: Theme; label: string }> = [
  { id: 'system', label: 'System' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
]

export function normalizeTheme(value: unknown): Theme {
  return value === 'light' || value === 'dark' ? value : 'system'
}

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
}

export function resolvedTheme(theme: Theme, prefersDark = systemPrefersDark()): 'light' | 'dark' {
  if (theme === 'light' || theme === 'dark') return theme
  return prefersDark ? 'dark' : 'light'
}

export function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  root.dataset.theme = theme
  if (theme === 'system') root.style.removeProperty('color-scheme')
  else root.style.colorScheme = theme
  const meta = document.querySelector('meta[name="theme-color"]')
  meta?.setAttribute('content', resolvedTheme(theme) === 'dark' ? '#2c2c2e' : '#ffffff')
}
