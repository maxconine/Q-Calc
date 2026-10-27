//! The games, ported one-to-one from `src/lib`: the shell around every game (`gameShell`, `gameClock`,
//! `gameSaves`, `gameFullscreen`, `lobby`, `nickname`, `peer`, `peerOnline`, `hello`) and six of the games with
//! their computers and online sessions (`pong*`, `connect4*`, `ultimate*`, `crack`, `equal`, `numbers`, and the
//! chalk paths of `components/ultimateChalk.ts`).
//!
//! Pure and deterministic like the TS: time comes in as ms, randomness as a `FnMut() -> f64` in [0, 1), and
//! messages for the other side go out through a `send` the caller drains. No I/O: `peer_online` is the relay link
//! as a state machine whose sockets and timers the shell runs.

pub mod clock;
pub mod connect4;
pub mod connect4_bot;
pub mod connect4_session;
pub mod crack;
// pub mod equal;
pub mod fullscreen;
pub mod hello;
pub mod lobby;
pub mod nickname;
// pub mod numbers;
pub mod peer;
pub mod peer_online;
// pub mod pong;
// pub mod pong_session;
pub mod saves;
pub mod shell;
pub mod ultimate;
pub mod ultimate_bot;
pub mod ultimate_chalk;
pub mod ultimate_session;

use serde_json::Value as Json;

/// Randomness as the TS takes it: `() => number` in [0, 1).
pub type Rand<'a> = &'a mut dyn FnMut() -> f64;

/// The two seats of a linked game (`Side` in `pong.ts`): the host, and the guest who joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Host,
    Guest,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Host => Side::Guest,
            Side::Guest => Side::Host,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Host => "host",
            Side::Guest => "guest",
        }
    }
}

/// mulberry32, the small seeded generator `equal.ts` and `ultimateChalk.ts` share.
pub fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// A non-cryptographic generator for the shells (`Math.random` in the TS): xorshift64*, seeded by the caller.
#[derive(Clone, Debug)]
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed | 1)
    }

    /// Seeded from the clock and an address, for a shell that has no seed of its own.
    pub fn from_entropy() -> Random {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let local = 0u8;
        let addr = &local as *const u8 as u64;
        Random::new(t ^ addr.rotate_left(32) ^ 0x2545_f491_4f6c_dd1d)
    }

    pub fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---- JSON as JSON.stringify writes it: keys in insertion order, numbers the JS way

/// One value of a message being written.
#[derive(Clone, Debug)]
pub enum Out {
    S(String),
    N(f64),
    B(bool),
    A(Vec<Out>),
}

/// `JSON.stringify` of an object with these keys in this order.
pub fn stringify(fields: &[(&str, Out)]) -> String {
    let mut s = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&quote(k));
        s.push(':');
        write_out(&mut s, v);
    }
    s.push('}');
    s
}

fn write_out(s: &mut String, v: &Out) {
    match v {
        Out::S(t) => s.push_str(&quote(t)),
        Out::N(n) => s.push_str(&if n.is_finite() {
            crate::jsnum::to_string(*n)
        } else {
            "null".into()
        }),
        Out::B(b) => s.push_str(if *b { "true" } else { "false" }),
        Out::A(items) => {
            s.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                write_out(s, x);
            }
            s.push(']');
        }
    }
}

fn quote(t: &str) -> String {
    serde_json::to_string(t).unwrap_or_else(|_| "\"\"".into())
}

/// `JSON.parse`, or None.
pub fn parse(raw: &str) -> Option<Json> {
    serde_json::from_str(raw).ok()
}

/// A finite number (`typeof v === 'number' && Number.isFinite(v)`).
pub fn finite(v: Option<&Json>) -> Option<f64> {
    v.and_then(Json::as_f64).filter(|n| n.is_finite())
}

/// `typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= max`.
pub fn count(v: Option<&Json>, max: f64) -> Option<f64> {
    finite(v).filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= max)
}

/// `Math.round` the JS way (halves go up).
pub fn js_round(x: f64) -> f64 {
    crate::jsnum::round(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulberry_matches_the_ts() {
        // rng(1)() in equal.ts
        let mut r = mulberry32(1);
        assert!((r() - 0.627_073_940_588_161_3).abs() < 1e-15);
    }

    #[test]
    fn stringify_writes_numbers_the_js_way() {
        assert_eq!(
            stringify(&[
                ("t", Out::S("p".into())),
                ("at", Out::N(300.0)),
                ("y", Out::N(1.5))
            ]),
            r#"{"t":"p","at":300,"y":1.5}"#
        );
    }
}
