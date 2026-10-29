//! emberSound.ts's synth (the Mac's EmberSound.swift): the core says which notes and hisses each event makes
//! (`sound::Voice`); this plays them the way WebAudio did. A tone is one oscillator whose pitch glides
//! exponentially, with a gain that rises exponentially from 0.0001 to its level in 8 ms and falls back to 0.0001 at
//! its end; a hiss is a looped 0.6 s of white noise through a biquad (the Web Audio spec's filter, its frequency
//! sweeping exponentially), with the same envelope over min(30 ms, dur/3). Everything goes through one gain of
//! MASTER (0.22).
//!
//! The device (WASAPI, shared mode) is opened on its own thread with the first sound, only in ember's process, and
//! closed when the game goes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use qcalc_core::games::ember::sound::{Filter, MASTER, Voice, Wave};

struct Live {
    v: Voice,
    /// seconds on the synth clock
    start: f64,
    phase: f64,
    noise_pos: usize,
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
    coeff_at: i64,
}

impl Live {
    fn new(v: Voice, start: f64) -> Live {
        Live {
            v,
            start,
            phase: 0.0,
            noise_pos: 0,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            coeff_at: -1,
        }
    }
}

/// What the synth plays: its clock, its voices and the noise loop. Shared by the game's thread (which adds voices)
/// and the audio thread (which renders them).
pub struct Mixer {
    rate: f64,
    clock: f64,
    voices: Vec<Live>,
    noise: Vec<f32>,
}

/// exponentialRampToValueAtTime from a to b over d, at t into it
fn exp_ramp(a: f64, b: f64, t: f64, d: f64) -> f64 {
    if d <= 0.0 {
        return b;
    }
    a * (b / a).powf((t / d).clamp(0.0, 1.0))
}

fn envelope(vol: f64, dur: f64, t: f64, attack: f64) -> f64 {
    if t < 0.0 {
        return 0.0;
    }
    if t < attack {
        return exp_ramp(0.0001, vol, t, attack);
    }
    if t < dur {
        return exp_ramp(vol, 0.0001, t - attack, dur - attack);
    }
    0.0001
}

impl Mixer {
    pub fn new(rate: f64) -> Mixer {
        // white noise, the same every run (no need for the OS's randomness)
        let mut s: u32 = 0x1234_5678;
        let noise = (0..(rate * 0.6) as usize)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (f64::from(s) / f64::from(u32::MAX) * 2.0 - 1.0) as f32
            })
            .collect();
        Mixer {
            rate,
            clock: 0.0,
            voices: Vec::new(),
            noise,
        }
    }

    pub fn play(&mut self, list: &[Voice]) {
        // a little after now, as the TS's currentTime + 0.005
        let t0 = self.clock + 0.005 + 0.01;
        for v in list {
            let at = match *v {
                Voice::Tone { at, .. } | Voice::Hiss { at, .. } => at,
            };
            self.voices.push(Live::new(*v, t0 + at));
        }
    }

    pub fn idle(&self) -> bool {
        self.voices.is_empty()
    }

    fn set_filter(&self, l: &mut Live, f: f64) {
        let Voice::Hiss { q, filter, .. } = l.v else {
            return;
        };
        let w0 = 2.0 * std::f64::consts::PI * f.min(self.rate / 2.0 - 1.0) / self.rate;
        let (sw, cw) = w0.sin_cos();
        let (b0, b1, b2, a0, a1, a2);
        match filter {
            Filter::Lowpass | Filter::Highpass => {
                // lowpass and highpass: Q in dB, as WebAudio has it
                let alpha = sw / (2.0 * 10f64.powf(q / 20.0));
                if filter == Filter::Lowpass {
                    b0 = (1.0 - cw) / 2.0;
                    b1 = 1.0 - cw;
                    b2 = (1.0 - cw) / 2.0;
                } else {
                    b0 = (1.0 + cw) / 2.0;
                    b1 = -(1.0 + cw);
                    b2 = (1.0 + cw) / 2.0;
                }
                a0 = 1.0 + alpha;
                a1 = -2.0 * cw;
                a2 = 1.0 - alpha;
            }
            Filter::Bandpass => {
                let alpha = sw / (2.0 * q.max(0.0001));
                b0 = alpha;
                b1 = 0.0;
                b2 = -alpha;
                a0 = 1.0 + alpha;
                a1 = -2.0 * cw;
                a2 = 1.0 - alpha;
            }
        }
        l.b0 = b0 / a0;
        l.b1 = b1 / a0;
        l.b2 = b2 / a0;
        l.a1 = a1 / a0;
        l.a2 = a2 / a0;
    }

    /// The next `out.len()` mono samples.
    pub fn render(&mut self, out: &mut [f32]) {
        let dt = 1.0 / self.rate;
        out.fill(0.0);
        let frames = out.len();
        let mut voices = std::mem::take(&mut self.voices);
        let mut k = 0;
        while k < voices.len() {
            let mut l = std::mem::replace(
                &mut voices[k],
                Live::new(
                    Voice::Tone {
                        at: 0.0,
                        freq: 0.0,
                        dur: 0.0,
                        vol: 0.0,
                        wave: Wave::Sine,
                        glide_to: None,
                    },
                    0.0,
                ),
            );
            let dur = match l.v {
                Voice::Tone { dur, .. } | Voice::Hiss { dur, .. } => dur,
            };
            let end = l.start + dur + 0.02;
            for (i, o) in out.iter_mut().enumerate() {
                let now = self.clock + i as f64 * dt;
                let t = now - l.start;
                if t < 0.0 || now > end {
                    continue;
                }
                let s = match l.v {
                    Voice::Tone {
                        freq,
                        dur,
                        vol,
                        wave,
                        glide_to,
                        ..
                    } => {
                        let f = glide_to.map_or(freq, |g| exp_ramp(freq, g, t, dur));
                        l.phase += f * dt;
                        l.phase -= l.phase.floor();
                        let p = l.phase;
                        let s = match wave {
                            Wave::Square => {
                                if p < 0.5 {
                                    1.0
                                } else {
                                    -1.0
                                }
                            }
                            Wave::Triangle => {
                                if p < 0.25 {
                                    4.0 * p
                                } else if p < 0.75 {
                                    2.0 - 4.0 * p
                                } else {
                                    4.0 * p - 4.0
                                }
                            }
                            Wave::Sawtooth => 2.0 * p - 1.0,
                            Wave::Sine => (2.0 * std::f64::consts::PI * p).sin(),
                        };
                        s * envelope(vol, dur, t, 0.008)
                    }
                    Voice::Hiss {
                        dur,
                        vol,
                        freq,
                        sweep_to,
                        offset,
                        ..
                    } => {
                        let step = (t * self.rate) as i64;
                        if step / 32 != l.coeff_at {
                            l.coeff_at = step / 32;
                            let f = sweep_to.map_or(freq, |s| exp_ramp(freq, s, t, dur));
                            self.set_filter(&mut l, f);
                        }
                        let n = self.noise.len().max(1);
                        if l.noise_pos == 0 {
                            l.noise_pos = (offset * self.rate) as usize % n;
                        }
                        let x = f64::from(self.noise.get(l.noise_pos % n).copied().unwrap_or(0.0));
                        l.noise_pos += 1;
                        let y = l.b0 * x + l.b1 * l.x1 + l.b2 * l.x2 - l.a1 * l.y1 - l.a2 * l.y2;
                        l.x2 = l.x1;
                        l.x1 = x;
                        l.y2 = l.y1;
                        l.y1 = y;
                        y * envelope(vol, dur, t, 0.03f64.min(dur / 3.0))
                    }
                };
                *o += (s * MASTER) as f32;
            }
            if self.clock + frames as f64 * dt > end {
                voices.remove(k);
            } else {
                voices[k] = l;
                k += 1;
            }
        }
        // voices added while this rendered (the game's thread) come after
        voices.append(&mut self.voices);
        self.voices = voices;
        self.clock += frames as f64 * dt;
    }
}

/// The synth: the device on its own thread, opened (without waiting for it) with the first sound and closed when
/// this is dropped. Sounds asked for while it opens are kept and played once it's up.
#[derive(Default)]
pub struct Synth {
    shared: Option<Arc<Mutex<Option<Mixer>>>>,
    pending: Vec<Voice>,
    stop: Option<Arc<AtomicBool>>,
    thread: Option<JoinHandle<()>>,
}

impl Synth {
    pub fn play(&mut self, list: &[Voice]) {
        if list.is_empty() {
            return;
        }
        if self.shared.is_none() {
            self.start();
        }
        let Some(shared) = &self.shared else { return };
        let Ok(mut slot) = shared.lock() else { return };
        match slot.as_mut() {
            Some(m) => {
                if !self.pending.is_empty() {
                    m.play(&std::mem::take(&mut self.pending));
                }
                m.play(list);
            }
            None if self.pending.len() < 64 => self.pending.extend_from_slice(list),
            None => {}
        }
    }

    fn start(&mut self) {
        let stop = Arc::new(AtomicBool::new(false));
        let shared: Arc<Mutex<Option<Mixer>>> = Arc::new(Mutex::new(None));
        let (s, m) = (stop.clone(), shared.clone());
        let thread = std::thread::Builder::new()
            .name("qcalc-ember-sound".into())
            .spawn(move || {
                if let Err(e) = wasapi::run(&s, &m) {
                    crate::qlog!("ember: no sound ({e})");
                }
            });
        if let Ok(t) = thread {
            self.thread = Some(t);
            self.stop = Some(stop);
        }
        self.shared = Some(shared);
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.stop.take() {
            s.store(true, Ordering::SeqCst);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.shared = None;
        self.pending.clear();
    }
}

impl Drop for Synth {
    fn drop(&mut self) {
        self.stop();
    }
}
mod wasapi {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::Win32::Media::Audio::{
        AUDCLNT_SHAREMODE_SHARED, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::core::{GUID, Result};

    use super::Mixer;

    const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
    const FLOAT_SUBTYPE: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    /// Opens the default output, hands back the mixer, and keeps ~40 ms queued until told to stop.
    pub fn run(stop: &AtomicBool, shared: &Mutex<Option<Mixer>>) -> Result<()> {
        // SAFETY: COM on this thread for its life; every call below is on objects made here
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let out = (|| -> Result<()> {
                let devices: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
                let fmt = client.GetMixFormat()?;
                let wf: WAVEFORMATEX = *fmt;
                let float = wf.wFormatTag == WAVE_FORMAT_IEEE_FLOAT
                    || (wf.wFormatTag == WAVE_FORMAT_EXTENSIBLE && {
                        let ext = *(fmt as *const WAVEFORMATEXTENSIBLE);
                        let sub = ext.SubFormat;
                        sub == FLOAT_SUBTYPE
                    });
                let bits = wf.wBitsPerSample;
                let channels = usize::from(wf.nChannels.max(1));
                let rate = f64::from(wf.nSamplesPerSec);
                // 50 ms of buffer
                let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 500_000, 0, fmt, None);
                CoTaskMemFree(Some(fmt as *const _));
                init?;
                if !(float && bits == 32) && bits != 16 {
                    return Err(windows::core::Error::new(
                        windows::core::HRESULT(-1),
                        "an output format the synth doesn't write",
                    ));
                }
                let size = client.GetBufferSize()? as usize;
                let render: IAudioRenderClient = client.GetService()?;
                if let Ok(mut slot) = shared.lock() {
                    *slot = Some(Mixer::new(rate));
                }
                client.Start()?;
                let mut mono = vec![0f32; size];
                let want = ((rate * 0.04) as usize).min(size);
                while !stop.load(Ordering::SeqCst) {
                    let padding = client.GetCurrentPadding()? as usize;
                    if padding < want {
                        let frames = want - padding;
                        if let Ok(mut slot) = shared.lock()
                            && let Some(m) = slot.as_mut()
                        {
                            m.render(&mut mono[..frames]);
                        }
                        let buf = render.GetBuffer(frames as u32)?;
                        if float && bits == 32 {
                            let dst = std::slice::from_raw_parts_mut(
                                buf.cast::<f32>(),
                                frames * channels,
                            );
                            for (f, s) in mono[..frames].iter().enumerate() {
                                for c in 0..channels {
                                    dst[f * channels + c] = *s;
                                }
                            }
                        } else {
                            let dst = std::slice::from_raw_parts_mut(
                                buf.cast::<i16>(),
                                frames * channels,
                            );
                            for (f, s) in mono[..frames].iter().enumerate() {
                                let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                                for c in 0..channels {
                                    dst[f * channels + c] = v;
                                }
                            }
                        }
                        render.ReleaseBuffer(frames as u32, 0)?;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
                let _ = client.Stop();
                Ok(())
            })();
            CoUninitialize();
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_sounds_and_ends() {
        let mut m = Mixer::new(48_000.0);
        m.play(&[Voice::Tone {
            at: 0.0,
            freq: 440.0,
            dur: 0.05,
            vol: 0.5,
            wave: Wave::Sine,
            glide_to: None,
        }]);
        let mut buf = vec![0f32; 4800];
        m.render(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.01));
        assert!(buf.iter().all(|s| s.abs() <= 1.0));
        m.render(&mut buf);
        assert!(m.idle());
        m.play(&[Voice::Hiss {
            at: 0.0,
            dur: 0.05,
            vol: 0.5,
            freq: 1200.0,
            q: 1.0,
            sweep_to: Some(300.0),
            filter: Filter::Bandpass,
            offset: 0.1,
        }]);
        m.render(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.0001));
    }
}
