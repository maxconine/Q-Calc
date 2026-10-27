//! Ember & frost's sounds (`src/lib/emberSound.ts`): tiny synthesised blips, no files. This is the score: which
//! notes and hisses each event makes, at what pitch, length and loudness. The Mac app's synth plays them the way
//! WebAudio does (an oscillator with a quick attack and an exponential tail; looped noise through a biquad), at
//! MASTER gain.

use std::collections::HashMap;

use super::types::{DeathCause, Element, EventKind, GameEvent};

pub const MASTER: f64 = 0.22;
/// the same sound twice within a blink is one sound
const SAME_GAP_MS: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Bandpass,
    Lowpass,
    Highpass,
}

/// one voice, starting `at` seconds after the sound begins
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Voice {
    /// one oscillator: vol reached 8 ms in, then an exponential tail to nothing at dur; glide_to sweeps the pitch
    Tone {
        at: f64,
        freq: f64,
        dur: f64,
        vol: f64,
        wave: Wave,
        glide_to: Option<f64>,
    },
    /// looped white noise (0.6 s of it, started at `offset` into the loop) through a biquad at freq (sweeping to
    /// sweep_to), q; vol reached at min(30 ms, dur/3), then the tail
    Hiss {
        at: f64,
        dur: f64,
        vol: f64,
        freq: f64,
        q: f64,
        sweep_to: Option<f64>,
        filter: Filter,
        offset: f64,
    },
}

fn tone(at: f64, freq: f64, dur: f64, vol: f64, wave: Wave, glide_to: Option<f64>) -> Voice {
    Voice::Tone {
        at,
        freq,
        dur,
        vol,
        wave,
        glide_to,
    }
}

#[allow(clippy::too_many_arguments)]
fn hiss(
    at: f64,
    dur: f64,
    vol: f64,
    freq: f64,
    q: f64,
    sweep_to: Option<f64>,
    filter: Filter,
    rand: &mut dyn FnMut() -> f64,
) -> Voice {
    Voice::Hiss {
        at,
        dur,
        vol,
        freq,
        q,
        sweep_to,
        filter,
        offset: rand() * 0.3,
    }
}

/// the voices an event makes (an empty list for a silent one). `rand` is Math.random's stand-in for the few
/// pitches the TS picks at random, and for where in the noise loop each hiss starts
pub fn voices(e: &GameEvent, rand: &mut dyn FnMut() -> f64) -> Vec<Voice> {
    use EventKind as K;
    use Filter::*;
    use Wave::*;
    let t = 0.0;
    let fire = e.p == Some(0);
    let on = e.on == Some(true);
    let mut v = vec![];
    match e.k {
        K::Jump => v.push(tone(
            t,
            if fire { 330.0 } else { 440.0 },
            0.12,
            0.18,
            Triangle,
            Some(if fire { 520.0 } else { 700.0 }),
        )),
        K::Land => v.push(tone(t, 120.0, 0.06, 0.06, Sine, Some(80.0))),
        K::Gem => {
            // two-note chime: ember's sits a fourth lower than frost's
            let base = if e.el == Some(Element::Fire) {
                784.0
            } else {
                1047.0
            };
            v.push(tone(t, base, 0.18, 0.16, Sine, None));
            v.push(tone(t + 0.07, base * 1.5, 0.3, 0.14, Sine, None));
            v.push(tone(t + 0.07, base * 3.0, 0.2, 0.03, Sine, None));
        }
        K::Lever | K::Mirror => {
            v.push(tone(t, 1600.0, 0.025, 0.1, Square, None));
            v.push(tone(t + 0.035, 900.0, 0.04, 0.08, Square, None));
        }
        K::Plate | K::Button => v.push(tone(
            t,
            if on { 520.0 } else { 380.0 },
            0.06,
            0.1,
            Triangle,
            Some(if on { 700.0 } else { 300.0 }),
        )),
        K::Sensor => {
            if on {
                v.push(tone(t, 1320.0, 0.15, 0.07, Sine, Some(1760.0)));
            }
        }
        K::Die => {
            match e.cause {
                Some(DeathCause::Water) => v.push(hiss(
                    t,
                    0.5,
                    0.35,
                    3500.0,
                    0.6,
                    Some(1800.0),
                    Highpass,
                    rand,
                )),
                Some(DeathCause::Goo) => v.push(tone(t, 180.0, 0.25, 0.18, Sine, Some(70.0))),
                Some(DeathCause::Crush) => {
                    v.push(hiss(t, 0.12, 0.4, 500.0, 0.8, Some(120.0), Lowpass, rand));
                    v.push(tone(t, 110.0, 0.14, 0.3, Square, Some(50.0)));
                }
                _ => v.push(hiss(t, 0.35, 0.2, 900.0, 2.0, Some(400.0), Bandpass, rand)),
            }
            v.push(tone(t, 520.0, 0.4, 0.13, Triangle, Some(130.0)));
        }
        K::Door => v.push(tone(
            t,
            if fire { 523.0 } else { 659.0 },
            0.14,
            0.1,
            Sine,
            None,
        )),
        K::Portal => {
            v.push(hiss(t, 0.3, 0.25, 300.0, 3.0, Some(2400.0), Bandpass, rand));
            v.push(tone(t, 300.0, 0.25, 0.06, Sine, Some(900.0)));
        }
        K::Push => v.push(hiss(t, 0.08, 0.05, 250.0, 1.0, None, Lowpass, rand)),
        K::Melt => {
            v.push(hiss(
                t,
                0.45,
                0.25,
                2800.0,
                0.7,
                Some(900.0),
                Bandpass,
                rand,
            ));
            for i in 0..4 {
                let f = 1800.0 + rand() * 1600.0;
                v.push(tone(t + f64::from(i) * 0.04, f, 0.05, 0.05, Triangle, None));
            }
        }
        K::Freeze => {
            for i in 0..3 {
                v.push(hiss(
                    t + f64::from(i) * 0.025,
                    0.03,
                    0.12,
                    5000.0 + f64::from(i) * 900.0,
                    4.0,
                    None,
                    Bandpass,
                    rand,
                ));
            }
            v.push(tone(t, 2093.0, 0.12, 0.03, Sine, None));
        }
        K::Thaw => v.push(tone(t, 700.0, 0.08, 0.04, Sine, Some(400.0))),
        K::Airjump => {
            v.push(tone(
                t,
                if fire { 520.0 } else { 700.0 },
                0.1,
                0.14,
                Triangle,
                Some(if fire { 880.0 } else { 1175.0 }),
            ));
            v.push(hiss(t, 0.08, 0.08, 2400.0, 2.0, None, Bandpass, rand));
        }
        K::Bounce => {
            // a springy boing, higher the harder the launch
            let f = 150.0 + ((e.v.unwrap_or(0.0) - 420.0) / 280.0).clamp(0.0, 1.0) * 130.0;
            v.push(tone(t, f, 0.22, 0.2, Sine, Some(f * 2.6)));
            v.push(tone(t, f * 2.0, 0.1, 0.05, Triangle, Some(f * 3.4)));
        }
        K::Arrow => {
            v.push(tone(t, 880.0, 0.08, 0.12, Triangle, None));
            v.push(tone(t + 0.06, 1319.0, 0.16, 0.1, Triangle, None));
        }
        K::Arrowback => v.push(tone(t, 1319.0, 0.08, 0.04, Sine, Some(1760.0))),
        K::Key => {
            v.push(tone(t, 1568.0, 0.07, 0.1, Square, None));
            v.push(tone(t + 0.06, 2093.0, 0.2, 0.08, Sine, None));
        }
        K::Unlock => {
            v.push(tone(t, 1400.0, 0.03, 0.1, Square, None));
            v.push(tone(t + 0.05, 700.0, 0.05, 0.1, Square, None));
            v.push(hiss(
                t + 0.08,
                0.25,
                0.15,
                500.0,
                1.0,
                Some(200.0),
                Lowpass,
                rand,
            ));
            v.push(tone(t + 0.1, 784.0, 0.3, 0.08, Sine, Some(1047.0)));
        }
        K::Load => {
            v.push(tone(t, 220.0, 0.09, 0.14, Sine, Some(110.0)));
            v.push(hiss(t, 0.06, 0.1, 400.0, 1.0, None, Lowpass, rand));
        }
        K::Fire => {
            v.push(hiss(t, 0.35, 0.45, 900.0, 0.7, Some(150.0), Lowpass, rand));
            v.push(tone(t, 160.0, 0.3, 0.3, Sine, Some(45.0)));
            v.push(hiss(
                t + 0.05,
                0.4,
                0.12,
                1800.0,
                2.0,
                Some(5000.0),
                Bandpass,
                rand,
            ));
        }
        K::Flip => {
            // gravity turning over: a swoop up when it flips, down when it comes back
            let up = e.grav.unwrap_or(1) < 0;
            v.push(tone(
                t,
                if up { 260.0 } else { 780.0 },
                0.28,
                0.14,
                Sine,
                Some(if up { 780.0 } else { 260.0 }),
            ));
            v.push(tone(
                t,
                if up { 390.0 } else { 1170.0 },
                0.28,
                0.05,
                Triangle,
                Some(if up { 1170.0 } else { 390.0 }),
            ));
        }
        K::Gswitch => {
            v.push(tone(t, 1200.0, 0.03, 0.09, Square, None));
            v.push(tone(t + 0.04, 600.0, 0.12, 0.08, Sawtooth, Some(300.0)));
        }
        K::Checkpoint => {
            v.push(tone(t, 659.0, 0.12, 0.12, Triangle, None));
            v.push(tone(t + 0.09, 988.0, 0.12, 0.12, Triangle, None));
            v.push(tone(t + 0.18, 1319.0, 0.35, 0.1, Sine, None));
        }
        K::Light => {
            // a clunk of the switch, then the room: a bright hum up, or a sigh down into the dark
            v.push(tone(
                t,
                if on { 1800.0 } else { 1100.0 },
                0.03,
                0.12,
                Square,
                None,
            ));
            if on {
                v.push(tone(t + 0.03, 392.0, 0.4, 0.09, Sine, Some(784.0)));
                v.push(tone(t + 0.03, 1568.0, 0.3, 0.04, Sine, None));
            } else {
                v.push(tone(t + 0.03, 392.0, 0.45, 0.1, Sine, Some(98.0)));
                v.push(hiss(
                    t + 0.03,
                    0.4,
                    0.08,
                    1200.0,
                    1.0,
                    Some(200.0),
                    Lowpass,
                    rand,
                ));
            }
        }
        K::Shake => {
            v.push(hiss(t, 0.12, 0.1, 700.0, 1.5, Some(300.0), Bandpass, rand));
            v.push(tone(t, 90.0, 0.1, 0.08, Sine, Some(60.0)));
        }
        K::Break => {
            v.push(hiss(t, 0.3, 0.3, 1200.0, 0.8, Some(200.0), Lowpass, rand));
            for i in 0..3 {
                let f = 240.0 - f64::from(i) * 50.0 + rand() * 40.0;
                v.push(tone(
                    t + f64::from(i) * 0.045,
                    f,
                    0.07,
                    0.09,
                    Triangle,
                    Some(70.0),
                ));
            }
        }
        K::Mend => v.push(tone(t, 330.0, 0.1, 0.05, Sine, Some(495.0))),
        K::Clear => {
            for (i, f) in [523.0, 659.0, 784.0].iter().enumerate() {
                v.push(tone(t + i as f64 * 0.09, *f, 0.25, 0.12, Triangle, None));
            }
            v.push(hiss(
                t + 0.3,
                0.6,
                0.1,
                400.0,
                1.0,
                Some(3000.0),
                Bandpass,
                rand,
            ));
        }
        K::Stage => {
            v.push(hiss(t, 0.4, 0.08, 2600.0, 1.0, Some(500.0), Bandpass, rand));
            v.push(tone(t + 0.1, 523.0, 0.25, 0.07, Sine, Some(784.0)));
        }
        K::Respawn => {
            v.push(hiss(t, 0.25, 0.1, 600.0, 2.0, Some(2400.0), Bandpass, rand));
            v.push(tone(t + 0.05, 440.0, 0.12, 0.1, Triangle, Some(660.0)));
            v.push(tone(t + 0.13, 660.0, 0.18, 0.08, Triangle, Some(880.0)));
        }
        K::Win => {
            // a bright little arpeggio, the top note from both elements at once
            for (i, f) in [523.0, 659.0, 784.0, 1047.0].iter().enumerate() {
                v.push(tone(t + i as f64 * 0.08, *f, 0.3, 0.13, Triangle, None));
            }
            v.push(tone(t + 0.34, 1319.0, 0.6, 0.1, Sine, None));
            v.push(tone(t + 0.34, 784.0, 0.6, 0.08, Sine, None));
        }
    }
    v
}

/// the rule that the same sound twice within a blink is one sound: the key is the kind, the player and the on
#[derive(Debug, Default)]
pub struct SoundGate {
    last: HashMap<String, f64>,
}

impl SoundGate {
    /// whether an event at `now` (ms) should sound
    pub fn pass(&mut self, e: &GameEvent, now: f64) -> bool {
        let key = format!(
            "{}{}{}",
            e.k.name(),
            e.p.map_or(String::new(), |p| p.to_string()),
            e.on.map_or(String::new(), |o| o.to_string())
        );
        if let Some(prev) = self.last.get(&key)
            && now - prev < SAME_GAP_MS
        {
            return false;
        }
        self.last.insert(key, now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_its_score() {
        let mut r = || 0.5;
        let gem = GameEvent {
            el: Some(Element::Frost),
            ..GameEvent::at_player(EventKind::Gem, 1, 0.0, 0.0)
        };
        let v = voices(&gem, &mut r);
        assert_eq!(v.len(), 3);
        assert_eq!(v[1], tone(0.07, 1570.5, 0.3, 0.14, Wave::Sine, None));
        let off = GameEvent::switched(EventKind::Sensor, 0, false, 0.0, 0.0);
        assert!(voices(&off, &mut r).is_empty());
        let mut g = SoundGate::default();
        assert!(g.pass(&gem, 0.0));
        assert!(!g.pass(&gem, 30.0));
        assert!(g.pass(&gem, 45.0));
    }
}
