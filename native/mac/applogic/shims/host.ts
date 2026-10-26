// What the shims ask the native side: the Rust engine's ported functions through Engine.callJson, and constants
// written out at bundle time from the real TS (constants.json, by applogic/build.sh).
declare const __host: { call(module: string, fn: string, argsJson: string): string }

export function engine<T>(module: string, fn: string, args: unknown[], fallback: T): T {
  try {
    const out = JSON.parse(__host.call(module, fn, JSON.stringify(args))) as { value?: T }
    return out && 'value' in out ? (out.value as T) : fallback
  } catch {
    return fallback
  }
}
