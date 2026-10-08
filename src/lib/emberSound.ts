import type { GameEvent } from './emberTypes'

// ember & frost's sounds: tiny webaudio blips, no files. the context is made on the first sound (after a key press,
// so autoplay rules are happy) and every failure is swallowed: no audio just means a quiet game
export type EmberSound = { play(e: GameEvent): void; setMuted(m: boolean): void; muted: boolean }

type Ctx = AudioContext
type Wave = OscillatorType

const MASTER = 0.22
const SAME_GAP_MS = 40

export function createEmberSound(): EmberSound {
  let ac: Ctx | null = null
  let out: GainNode | null = null
  let noise: AudioBuffer | null = null
  let failed = false
  const last = new Map<string, number>()

  const ctx = (): Ctx | null => {
    if (ac || failed) return ac
    try {
      const C = (globalThis as { AudioContext?: typeof AudioContext; webkitAudioContext?: typeof AudioContext }).AudioContext ?? (globalThis as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
      if (!C) {
        failed = true
        return null
      }
      ac = new C()
      out = ac.createGain()
      out.gain.value = MASTER
      out.connect(ac.destination)
    } catch {
      failed = true
      ac = null
    }
    return ac
  }

  // a note: one oscillator with a quick attack and an exponential tail
  const tone = (c: Ctx, at: number, freq: number, dur: number, vol: number, type: Wave = 'sine', glideTo?: number) => {
    const o = c.createOscillator()
    const g = c.createGain()
    o.type = type
    o.frequency.setValueAtTime(freq, at)
    if (glideTo) o.frequency.exponentialRampToValueAtTime(glideTo, at + dur)
    g.gain.setValueAtTime(0.0001, at)
    g.gain.exponentialRampToValueAtTime(vol, at + 0.008)
    g.gain.exponentialRampToValueAtTime(0.0001, at + dur)
    o.connect(g)
    g.connect(out!)
    o.start(at)
    o.stop(at + dur + 0.02)
  }

  // filtered noise: hisses, whooshes, crackles
  const hiss = (c: Ctx, at: number, dur: number, vol: number, freq: number, q = 1, sweepTo?: number, type: BiquadFilterType = 'bandpass') => {
    if (!noise) {
      noise = c.createBuffer(1, Math.floor(c.sampleRate * 0.6), c.sampleRate)
      const d = noise.getChannelData(0)
      for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1
    }
    const src = c.createBufferSource()
    src.buffer = noise
    src.loop = true
    const f = c.createBiquadFilter()
    f.type = type
    f.frequency.setValueAtTime(freq, at)
    if (sweepTo) f.frequency.exponentialRampToValueAtTime(sweepTo, at + dur)
    f.Q.value = q
    const g = c.createGain()
    g.gain.setValueAtTime(0.0001, at)
    g.gain.exponentialRampToValueAtTime(vol, at + Math.min(0.03, dur / 3))
    g.gain.exponentialRampToValueAtTime(0.0001, at + dur)
    src.connect(f)
    f.connect(g)
    g.connect(out!)
    src.start(at, Math.random() * 0.3)
    src.stop(at + dur + 0.02)
  }

  const s: EmberSound = {
    muted: false,
    setMuted(m) {
      s.muted = m
    },
    play(e) {
      if (s.muted) return
      // the same sound twice within a blink is one sound
      const key = e.k + ('p' in e ? e.p : '') + ('on' in e ? String(e.on) : '')
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
      const prev = last.get(key)
      if (prev !== undefined && now - prev < SAME_GAP_MS) return
      last.set(key, now)
      const c = ctx()
      if (!c || !out) return
      try {
        if (c.state === 'suspended') void c.resume().catch(() => {})
        const t = c.currentTime + 0.005
        const fire = 'p' in e ? e.p === 0 : false
        switch (e.k) {
          case 'jump':
            tone(c, t, fire ? 330 : 440, 0.12, 0.18, 'triangle', fire ? 520 : 700)
            break
          case 'land':
            tone(c, t, 120, 0.06, 0.06, 'sine', 80)
            break
          case 'gem': {
            // two-note chime: ember's sits a fourth lower than frost's
            const base = e.el === 'fire' ? 784 : 1047
            tone(c, t, base, 0.18, 0.16, 'sine')
            tone(c, t + 0.07, base * 1.5, 0.3, 0.14, 'sine')
            tone(c, t + 0.07, base * 3, 0.2, 0.03, 'sine')
            break
          }
          case 'lever':
          case 'mirror':
            tone(c, t, 1600, 0.025, 0.1, 'square')
            tone(c, t + 0.035, 900, 0.04, 0.08, 'square')
            break
          case 'plate':
          case 'button':
            tone(c, t, e.on ? 520 : 380, 0.06, 0.1, 'triangle', e.on ? 700 : 300)
            break
          case 'sensor':
            if (e.on) tone(c, t, 1320, 0.15, 0.07, 'sine', 1760)
            break
          case 'die':
            if (e.cause === 'water') hiss(c, t, 0.5, 0.35, 3500, 0.6, 1800, 'highpass')
            else if (e.cause === 'goo') tone(c, t, 180, 0.25, 0.18, 'sine', 70)
            else hiss(c, t, 0.35, 0.2, 900, 2, 400)
            tone(c, t, 520, 0.4, 0.13, 'triangle', 130)
            break
          case 'door':
            tone(c, t, fire ? 523 : 659, 0.14, 0.1, 'sine')
            break
          case 'portal':
            hiss(c, t, 0.3, 0.25, 300, 3, 2400)
            tone(c, t, 300, 0.25, 0.06, 'sine', 900)
            break
          case 'push':
            hiss(c, t, 0.08, 0.05, 250, 1, undefined, 'lowpass')
            break
          case 'melt':
            hiss(c, t, 0.45, 0.25, 2800, 0.7, 900)
            for (let i = 0; i < 4; i++) tone(c, t + i * 0.04, 1800 + Math.random() * 1600, 0.05, 0.05, 'triangle')
            break
          case 'freeze':
            for (let i = 0; i < 3; i++) hiss(c, t + i * 0.025, 0.03, 0.12, 5000 + i * 900, 4)
            tone(c, t, 2093, 0.12, 0.03, 'sine')
            break
          case 'thaw':
            tone(c, t, 700, 0.08, 0.04, 'sine', 400)
            break
          case 'win': {
            // a bright little arpeggio, the top note from both elements at once
            const notes = [523, 659, 784, 1047]
            notes.forEach((f, i) => tone(c, t + i * 0.08, f, 0.3, 0.13, 'triangle'))
            tone(c, t + 0.34, 1319, 0.6, 0.1, 'sine')
            tone(c, t + 0.34, 784, 0.6, 0.08, 'sine')
            break
          }
        }
      } catch {
        // a sound is never worth an error
      }
    },
  }
  return s
}
