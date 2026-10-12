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
            else if (e.cause === 'crush') {
              hiss(c, t, 0.12, 0.4, 500, 0.8, 120, 'lowpass')
              tone(c, t, 110, 0.14, 0.3, 'square', 50)
            }
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
          case 'airjump':
            tone(c, t, fire ? 520 : 700, 0.1, 0.14, 'triangle', fire ? 880 : 1175)
            hiss(c, t, 0.08, 0.08, 2400, 2)
            break
          case 'bounce': {
            // a springy boing, higher the harder the launch
            const f = 150 + Math.max(0, Math.min(1, (e.v - 420) / 280)) * 130
            tone(c, t, f, 0.22, 0.2, 'sine', f * 2.6)
            tone(c, t, f * 2, 0.1, 0.05, 'triangle', f * 3.4)
            break
          }
          case 'arrow':
            tone(c, t, 880, 0.08, 0.12, 'triangle')
            tone(c, t + 0.06, 1319, 0.16, 0.1, 'triangle')
            break
          case 'arrowback':
            tone(c, t, 1319, 0.08, 0.04, 'sine', 1760)
            break
          case 'key':
            tone(c, t, 1568, 0.07, 0.1, 'square')
            tone(c, t + 0.06, 2093, 0.2, 0.08, 'sine')
            break
          case 'unlock':
            tone(c, t, 1400, 0.03, 0.1, 'square')
            tone(c, t + 0.05, 700, 0.05, 0.1, 'square')
            hiss(c, t + 0.08, 0.25, 0.15, 500, 1, 200, 'lowpass')
            tone(c, t + 0.1, 784, 0.3, 0.08, 'sine', 1047)
            break
          case 'load':
            tone(c, t, 220, 0.09, 0.14, 'sine', 110)
            hiss(c, t, 0.06, 0.1, 400, 1, undefined, 'lowpass')
            break
          case 'fire':
            hiss(c, t, 0.35, 0.45, 900, 0.7, 150, 'lowpass')
            tone(c, t, 160, 0.3, 0.3, 'sine', 45)
            hiss(c, t + 0.05, 0.4, 0.12, 1800, 2, 5000)
            break
          case 'flip':
            // gravity turning over: a swoop up when it flips, down when it comes back
            tone(c, t, e.grav < 0 ? 260 : 780, 0.28, 0.14, 'sine', e.grav < 0 ? 780 : 260)
            tone(c, t, e.grav < 0 ? 390 : 1170, 0.28, 0.05, 'triangle', e.grav < 0 ? 1170 : 390)
            break
          case 'gswitch':
            tone(c, t, 1200, 0.03, 0.09, 'square')
            tone(c, t + 0.04, 600, 0.12, 0.08, 'sawtooth', 300)
            break
          case 'checkpoint':
            tone(c, t, 659, 0.12, 0.12, 'triangle')
            tone(c, t + 0.09, 988, 0.12, 0.12, 'triangle')
            tone(c, t + 0.18, 1319, 0.35, 0.1, 'sine')
            break
          case 'light':
            // a clunk of the switch, then the room: a bright hum up, or a sigh down into the dark
            tone(c, t, e.on ? 1800 : 1100, 0.03, 0.12, 'square')
            if (e.on) {
              tone(c, t + 0.03, 392, 0.4, 0.09, 'sine', 784)
              tone(c, t + 0.03, 1568, 0.3, 0.04, 'sine')
            } else {
              tone(c, t + 0.03, 392, 0.45, 0.1, 'sine', 98)
              hiss(c, t + 0.03, 0.4, 0.08, 1200, 1, 200, 'lowpass')
            }
            break
          case 'shake':
            hiss(c, t, 0.12, 0.1, 700, 1.5, 300)
            tone(c, t, 90, 0.1, 0.08, 'sine', 60)
            break
          case 'break':
            hiss(c, t, 0.3, 0.3, 1200, 0.8, 200, 'lowpass')
            for (let i = 0; i < 3; i++) tone(c, t + i * 0.045, 240 - i * 50 + Math.random() * 40, 0.07, 0.09, 'triangle', 70)
            break
          case 'mend':
            tone(c, t, 330, 0.1, 0.05, 'sine', 495)
            break
          case 'clear':
            ;[523, 659, 784].forEach((f, i) => tone(c, t + i * 0.09, f, 0.25, 0.12, 'triangle'))
            hiss(c, t + 0.3, 0.6, 0.1, 400, 1, 3000)
            break
          case 'stage':
            hiss(c, t, 0.4, 0.08, 2600, 1, 500)
            tone(c, t + 0.1, 523, 0.25, 0.07, 'sine', 784)
            break
          case 'respawn':
            hiss(c, t, 0.25, 0.1, 600, 2, 2400)
            tone(c, t + 0.05, 440, 0.12, 0.1, 'triangle', 660)
            tone(c, t + 0.13, 660, 0.18, 0.08, 'triangle', 880)
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
