import AVFoundation
import QCalcCore

// emberSound.ts's synth: the core says which notes and hisses each event makes (EmberVoice); this plays them the
// way WebAudio did. A tone is one oscillator whose pitch glides exponentially, with a gain that rises
// exponentially from 0.0001 to its level in 8 ms and falls back to 0.0001 at its end; a hiss is a looped 0.6 s of
// white noise through a biquad (the Web Audio spec's filter, its frequency sweeping exponentially), with the same
// envelope over min(30 ms, dur/3). Everything goes through one gain of MASTER (0.22). The engine starts with
// the first sound and stops when the game goes.

final class EmberSynth {
    private struct Live {
        let v: EmberVoice
        let start: Double // seconds on the synth clock
        var phase = 0.0
        var noisePos = 0
        var b0 = 0.0, b1 = 0.0, b2 = 0.0, a1 = 0.0, a2 = 0.0
        var x1 = 0.0, x2 = 0.0, y1 = 0.0, y2 = 0.0
        var coeffAt = -1
    }

    private let master = 0.22
    private var engine: AVAudioEngine?
    private var source: AVAudioSourceNode?
    private var rate = 48000.0
    private var clock = 0.0 // seconds rendered so far
    private var voices: [Live] = []
    private let lock = NSLock()
    private var noise: [Float] = []
    private var failed = false

    private func ready() -> Bool {
        if engine != nil { return true }
        if failed { return false }
        let e = AVAudioEngine()
        let out = e.outputNode
        let format = out.inputFormat(forBus: 0)
        rate = format.sampleRate > 0 ? format.sampleRate : 48000
        noise = (0..<Int(rate * 0.6)).map { _ in Float.random(in: -1..<1) }
        let mono = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1)!
        let node = AVAudioSourceNode(format: mono) { [weak self] _, _, frames, list -> OSStatus in
            guard let self else { return noErr }
            let buffers = UnsafeMutableAudioBufferListPointer(list)
            guard let data = buffers.first?.mData?.assumingMemoryBound(to: Float.self) else { return noErr }
            self.render(data, Int(frames))
            return noErr
        }
        e.attach(node)
        e.connect(node, to: e.mainMixerNode, format: mono)
        do {
            try e.start()
        } catch {
            failed = true
            return false
        }
        engine = e
        source = node
        return true
    }

    func play(_ list: [EmberVoice]) {
        guard !list.isEmpty, ready() else { return }
        lock.lock()
        // a little after now, as the TS's currentTime + 0.005
        let t0 = clock + 0.005 + 0.01
        for v in list { voices.append(Live(v: v, start: t0 + v.at)) }
        lock.unlock()
    }

    func stop() {
        engine?.stop()
        engine = nil
        source = nil
        lock.lock()
        voices.removeAll()
        lock.unlock()
    }

    deinit { stop() }

    /// exponentialRampToValueAtTime from a to b over d, at t into it
    @inline(__always) private func expRamp(_ a: Double, _ b: Double, _ t: Double, _ d: Double) -> Double {
        if d <= 0 { return b }
        return a * pow(b / a, min(1, max(0, t / d)))
    }

    private func envelope(_ v: EmberVoice, _ t: Double, attack: Double) -> Double {
        if t < 0 { return 0 }
        if t < attack { return expRamp(0.0001, v.vol, t, attack) }
        if t < v.dur { return expRamp(v.vol, 0.0001, t - attack, v.dur - attack) }
        return 0.0001
    }

    private func setFilter(_ l: inout Live, _ f: Double) {
        let v = l.v
        let w0 = 2 * Double.pi * min(f, rate / 2 - 1) / rate
        let cw = cos(w0), sw = sin(w0)
        var b0 = 0.0, b1 = 0.0, b2 = 0.0, a0 = 1.0, a1 = 0.0, a2 = 0.0
        switch v.filter {
        case 1, 2:
            // lowpass and highpass: Q in dB, as WebAudio has it
            let alpha = sw / (2 * pow(10, v.q / 20))
            if v.filter == 1 {
                b0 = (1 - cw) / 2
                b1 = 1 - cw
                b2 = (1 - cw) / 2
            } else {
                b0 = (1 + cw) / 2
                b1 = -(1 + cw)
                b2 = (1 + cw) / 2
            }
            a0 = 1 + alpha
            a1 = -2 * cw
            a2 = 1 - alpha
        default:
            let alpha = sw / (2 * max(v.q, 0.0001))
            b0 = alpha
            b1 = 0
            b2 = -alpha
            a0 = 1 + alpha
            a1 = -2 * cw
            a2 = 1 - alpha
        }
        l.b0 = b0 / a0
        l.b1 = b1 / a0
        l.b2 = b2 / a0
        l.a1 = a1 / a0
        l.a2 = a2 / a0
    }

    private func render(_ out: UnsafeMutablePointer<Float>, _ frames: Int) {
        lock.lock()
        defer { lock.unlock() }
        let dt = 1 / rate
        for i in 0..<frames { out[i] = 0 }
        var k = 0
        while k < voices.count {
            var l = voices[k]
            let v = l.v
            let end = l.start + v.dur + 0.02
            for i in 0..<frames {
                let now = clock + Double(i) * dt
                let t = now - l.start
                if t < 0 || now > end { continue }
                var s: Double
                if v.tone {
                    let f = v.glideTo.map { expRamp(v.freq, $0, t, v.dur) } ?? v.freq
                    l.phase += f * dt
                    l.phase -= floor(l.phase)
                    let p = l.phase
                    switch v.wave {
                    case 1: s = p < 0.5 ? 1 : -1
                    case 2: s = p < 0.25 ? 4 * p : p < 0.75 ? 2 - 4 * p : 4 * p - 4
                    case 3: s = 2 * p - 1
                    default: s = sin(2 * .pi * p)
                    }
                    s *= envelope(v, t, attack: 0.008)
                } else {
                    let step = Int(t * rate)
                    if step / 32 != l.coeffAt {
                        l.coeffAt = step / 32
                        setFilter(&l, v.sweepTo.map { expRamp(v.freq, $0, t, v.dur) } ?? v.freq)
                    }
                    if l.noisePos == 0 { l.noisePos = Int(v.offset * rate) % max(1, noise.count) }
                    let x = Double(noise[l.noisePos % noise.count])
                    l.noisePos += 1
                    let y = l.b0 * x + l.b1 * l.x1 + l.b2 * l.x2 - l.a1 * l.y1 - l.a2 * l.y2
                    l.x2 = l.x1
                    l.x1 = x
                    l.y2 = l.y1
                    l.y1 = y
                    s = y * envelope(v, t, attack: min(0.03, v.dur / 3))
                }
                out[i] += Float(s * master)
            }
            if clock + Double(frames) * dt > end {
                voices.remove(at: k)
            } else {
                voices[k] = l
                k += 1
            }
        }
        clock += Double(frames) * dt
    }
}
