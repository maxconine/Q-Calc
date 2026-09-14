import { describe, expect, it } from 'vitest'
import { normalizeTheme, resolvedTheme } from './theme'

describe('normalizeTheme', () => {
  it('keeps light and dark', () => {
    expect(normalizeTheme('light')).toBe('light')
    expect(normalizeTheme('dark')).toBe('dark')
  })

  it('falls back to system', () => {
    expect(normalizeTheme('system')).toBe('system')
    expect(normalizeTheme(undefined)).toBe('system')
    expect(normalizeTheme('sepia')).toBe('system')
  })
})

describe('resolvedTheme', () => {
  it('honors an explicit choice', () => {
    expect(resolvedTheme('light', true)).toBe('light')
    expect(resolvedTheme('dark', false)).toBe('dark')
  })

  it('follows the system preference', () => {
    expect(resolvedTheme('system', true)).toBe('dark')
    expect(resolvedTheme('system', false)).toBe('light')
  })
})
