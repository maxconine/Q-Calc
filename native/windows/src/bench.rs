//! The dev bench: lets a script drive the app and time what the targets are about (native/PERFORMANCE.md):
//! keystroke → live answer committed to the model *and painted*, median / p90 / p99. The Windows twin of
//! native/mac/Sources/App/DevBench.swift, with the same spec.
//!
//! Two ways in:
//! - `qcalc.exe --bench <spec.json>` (to a running instance it is forwarded through WM_COPYDATA), and
//! - the named pipe `\\.\pipe\qcalc-bench`, served when the app runs with `--bench-pipe` (or QCALC_BENCH=1). A
//!   client writes one JSON request and reads one JSON reply. native/windows/scripts/bench.ps1 wraps it.
//!
//! Requests:
//! - a spec: `{"inputs": ["2+2", "sin(90)"], "whole": false, "enter": false, "out": "path.json", "quit": false}`
//!   Each input is typed a character at a time as real WM_CHAR messages posted to the bar (`whole`: all at once);
//!   each keystroke is timed from the post to the end of the paint (EndDraw) that shows the live answer for that
//!   text. `enter` presses ↵ after each input (and waits for the commit to land). The reply (and `out`) is
//!   `{"ms": [...], "idx": [...], "timeouts": n, "wallMs": t, "n": n, "median": m, "p90": a, "p99": b}`.
//! - commands: `{"cmd": "show"}`, `"hide"`, `"state"` (the line, the answer, the rows, visibility),
//!   `{"cmd": "type", "text": "..."}` (posts WM_CHARs), `{"cmd": "key", "vk": 13}`, `"mem"` (working set and
//!   private bytes), `"trim"` (what a long hide does), `"quit"`.
//!
//! The app also logs `ready` with ms since process creation (log.rs), for warm-launch timing.

use std::sync::mpsc::Sender;
use std::time::Instant;

use serde::Deserialize;

use crate::model::BarModel;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub whole: bool,
    #[serde(default)]
    pub enter: bool,
    #[serde(default)]
    pub out: Option<String>,
    #[serde(default)]
    pub quit: bool,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub vk: Option<u32>,
    /// also reply with what the bar showed for each input (`answers`): the e2e parity check
    #[serde(default)]
    pub answers: bool,
}

/// One step of a run.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// post these UTF-16 units as WM_CHAR, then wait for `expect`'s answer to be painted
    Key {
        input: usize,
        units: Vec<u16>,
        expect: String,
    },
    /// ↵, then wait for the commit to land
    Enter,
    /// empty the line
    Clear,
}

/// The steps a spec makes.
pub fn steps(spec: &Spec) -> Vec<Step> {
    let mut out = Vec::new();
    for (i, input) in spec.inputs.iter().enumerate() {
        if spec.whole {
            out.push(Step::Key {
                input: i,
                units: input.encode_utf16().collect(),
                expect: input.clone(),
            });
        } else {
            let mut acc = String::new();
            for c in input.chars() {
                acc.push(c);
                out.push(Step::Key {
                    input: i,
                    units: c.encode_utf16(&mut [0; 2]).to_vec(),
                    expect: acc.clone(),
                });
            }
        }
        if spec.enter {
            out.push(Step::Enter);
        }
        out.push(Step::Clear);
    }
    out
}

/// The nearest-rank percentile of sorted values.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

pub fn summary(ms: &[f64]) -> (f64, f64, f64) {
    let mut s = ms.to_vec();
    s.sort_by(f64::total_cmp);
    (
        percentile(&s, 50.0),
        percentile(&s, 90.0),
        percentile(&s, 99.0),
    )
}

/// What the bench is waiting for.
#[derive(Clone, Debug, PartialEq)]
pub enum Wait {
    None,
    /// the live answer for this text, painted; or (the input's rules turned it into symbols) for whatever the
    /// keystroke made of the line before it (the second string)
    Answer(String, String),
    /// a commit: the line no longer this text (or a hint showed)
    Commit(String),
}

/// A run in progress (on the UI thread).
pub struct Run {
    pub spec: Spec,
    pub steps: Vec<Step>,
    pub k: usize,
    pub t0: Instant,
    pub wait: Wait,
    pub ms: Vec<f64>,
    pub idx: Vec<usize>,
    pub timeouts: u32,
    pub started: Instant,
    pub reply: Option<Sender<String>>,
    pub current_input: usize,
    /// with `spec.answers`: (input, the slot's text, the engine's display) at each input's last keystroke
    pub answers: Vec<(usize, String, String)>,
}

impl Run {
    pub fn new(spec: Spec, reply: Option<Sender<String>>) -> Run {
        Run {
            steps: steps(&spec),
            spec,
            k: 0,
            t0: Instant::now(),
            wait: Wait::None,
            ms: Vec::new(),
            idx: Vec::new(),
            timeouts: 0,
            started: Instant::now(),
            reply,
            current_input: 0,
            answers: Vec::new(),
        }
    }

    /// After a paint: whether the awaited thing is on screen now (then the next step may go).
    pub fn painted(&mut self, model: &BarModel) -> bool {
        match &self.wait {
            Wait::Answer(text, before)
                if model.live_for == model.field.text()
                    && (model.field.text() == text || model.field.text() != before) =>
            {
                self.ms.push(self.t0.elapsed().as_secs_f64() * 1000.0);
                self.idx.push(self.current_input);
                self.wait = Wait::None;
                if self.spec.answers {
                    let display = model
                        .live
                        .as_ref()
                        .map_or(String::new(), |a| a.display.clone());
                    let shown = (self.current_input, model.answer_text(), display);
                    match self.answers.last_mut() {
                        Some(last) if last.0 == self.current_input => *last = shown,
                        _ => self.answers.push(shown),
                    }
                }
                true
            }
            Wait::Commit(text) if model.field.text() != text || model.hint.is_some() => {
                self.wait = Wait::None;
                true
            }
            _ => false,
        }
    }

    pub fn result(&self) -> serde_json::Value {
        let (median, p90, p99) = summary(&self.ms);
        let mut out = serde_json::json!({
            "ms": self.ms, "idx": self.idx, "timeouts": self.timeouts,
            "wallMs": self.started.elapsed().as_secs_f64() * 1000.0,
            "n": self.ms.len(), "median": median, "p90": p90, "p99": p99,
        });
        if self.spec.answers {
            out["answers"] = self
                .answers
                .iter()
                .map(|(i, shown, display)| {
                    serde_json::json!({ "input": i, "shown": shown, "display": display })
                })
                .collect();
        }
        out
    }
}

#[cfg(windows)]
pub mod pipe {
    //! `\\.\pipe\qcalc-bench`: one request, one reply per connection, served on its own thread.
    use std::sync::mpsc::{Sender, channel};

    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
    use windows::Win32::Storage::FileSystem::{
        FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

    use crate::qlog;

    /// What the pipe hands the UI thread (boxed through the message's lParam).
    pub struct Request {
        pub text: String,
        pub reply: Sender<String>,
    }

    pub fn serve(host: HWND, msg: u32) {
        let host = host.0 as usize;
        let _ = std::thread::Builder::new()
            .name("qcalc-bench-pipe".into())
            .spawn(move || {
                loop {
                    // SAFETY: a fresh pipe instance each loop, closed below
                    let pipe = unsafe {
                        CreateNamedPipeW(
                            &windows::core::HSTRING::from(format!(
                                r"\\.\pipe\qcalc-bench{}",
                                crate::log::instance_suffix()
                            )),
                            PIPE_ACCESS_DUPLEX,
                            PIPE_TYPE_BYTE
                                | PIPE_READMODE_BYTE
                                | PIPE_WAIT
                                | PIPE_REJECT_REMOTE_CLIENTS,
                            1,
                            65536,
                            65536,
                            0,
                            None,
                        )
                    };
                    if pipe.is_invalid() {
                        qlog!("bench pipe: couldn't create");
                        return;
                    }
                    // SAFETY: a live pipe handle; blocks until a client connects (or one already has)
                    let _ = unsafe { ConnectNamedPipe(pipe, None) };
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        let mut got = 0u32;
                        // SAFETY: chunk is a live buffer
                        let ok = unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut got), None) }
                            .is_ok();
                        buf.extend_from_slice(&chunk[..got as usize]);
                        if !ok || got == 0 || buf.contains(&b'\n') {
                            break;
                        }
                        // a whole JSON value read: done (clients that don't send a newline)
                        if serde_json::from_slice::<serde_json::Value>(&buf).is_ok() {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).trim().to_string();
                    let (tx, rx) = channel();
                    let req = Box::into_raw(Box::new(Request { text, reply: tx }));
                    // SAFETY: the UI thread takes back the box (app.rs, WM_PIPE); if the post fails it is ours again
                    let posted = unsafe {
                        PostMessageW(
                            Some(HWND(host as *mut _)),
                            msg,
                            WPARAM(0),
                            LPARAM(req as isize),
                        )
                    };
                    let answer = if posted.is_ok() {
                        rx.recv()
                            .unwrap_or_else(|_| "{\"error\":\"no reply\"}".into())
                    } else {
                        // SAFETY: not posted, so still ours
                        drop(unsafe { Box::from_raw(req) });
                        "{\"error\":\"app not listening\"}".into()
                    };
                    let bytes = format!("{answer}\n");
                    let mut wrote = 0u32;
                    // SAFETY: a live pipe and buffer
                    unsafe {
                        let _ = WriteFile(pipe, Some(bytes.as_bytes()), Some(&mut wrote), None);
                        let _ = FlushFileBuffers(pipe);
                        let _ = DisconnectNamedPipe(pipe);
                        let _ = CloseHandle(pipe);
                    }
                }
            });
    }
}

/// The pipe commands the e2e suite adds (native/windows/tests/e2e), for any `cmd` app.rs doesn't know itself:
/// - `"e2e"`: which windows are up, the theme, the hotkey's state and memory, in one reply;
/// - `"settings-controls"`: the settings window's controls in screen pixels (settings_window::debug), so the suite
///   can click one for real.
#[cfg(windows)]
pub fn extra(app: &crate::app::App, cmd: &str) -> Option<serde_json::Value> {
    match cmd {
        "e2e" => {
            let (ws, private) = memory();
            let (f, s, l) = (app.bar.frame(), app.bar.scale(), &app.bar.layout);
            let screen = |r: crate::render::Rect| {
                [
                    f.left + (r.x * s) as i32,
                    f.top + (r.y * s) as i32,
                    (r.w * s) as i32,
                    (r.h * s) as i32,
                ]
            };
            Some(serde_json::json!({
                "frame": [f.left, f.top, f.right, f.bottom], "scale": s,
                "composer": screen(l.composer), "field": screen(l.field), "answer": screen(l.answer),
                "pid": std::process::id(),
                "visible": app.bar.visible, "focused": app.bar.focused,
                "settingsOpen": crate::settings_window::is_open(), "gameOpen": crate::games::is_open(),
                "theme": app.store.settings.theme, "dark": app.theme.dark,
                "hotkey": { "wanted": app.hotkey.wanted, "active": app.hotkey.active, "gaveUp": app.hotkey.gave_up },
                "rows": app.model.rows.rows.len(), "text": app.model.field.text(),
                "workingSet": ws, "private": private,
            }))
        }
        "settings-controls" => Some(crate::settings_window::debug()),
        _ => None,
    }
}

/// Process memory: (working set, private bytes), bytes.
#[cfg(windows)]
pub fn memory() -> (usize, usize) {
    use windows::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: c is a live struct of the size passed
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            c.cb,
        )
    };
    if ok.as_bool() {
        (c.WorkingSetSize, c.PrivateUsage)
    } else {
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_types_each_prefix_then_clears() {
        let spec = Spec {
            inputs: vec!["2+2".into(), "π".into()],
            enter: true,
            ..Default::default()
        };
        let s = steps(&spec);
        assert_eq!(s.len(), 3 + 2 + 1 + 2);
        assert_eq!(
            s[2],
            Step::Key {
                input: 0,
                units: vec![u16::from(b'2')],
                expect: "2+2".into()
            }
        );
        assert_eq!(s[3], Step::Enter);
        assert_eq!(s[4], Step::Clear);
        let whole = steps(&Spec {
            inputs: vec!["sin(90)".into()],
            whole: true,
            ..Default::default()
        });
        assert_eq!(whole.len(), 2);
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(summary(&v), (50.0, 90.0, 99.0));
        assert_eq!(percentile(&[3.0], 99.0), 3.0);
        assert_eq!(percentile(&[], 50.0), 0.0);
    }
}
