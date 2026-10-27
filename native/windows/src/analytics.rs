//! Anonymous usage counts, as the Mac's Analytics.swift keeps them and with the core's rules
//! (`qcalc_core::app::analytics`, the port of src/lib/analytics.ts): a stash of daily counts per event (`bump`),
//! posted at most every six hours (`due_to_send`, `usage_payload`, `after_send`) while "Share anonymous usage" is
//! on, forgotten when it is turned off. The stash lives where the Tauri app kept it: `settings.json`'s
//! `analytics`, so counts carry over in both directions. Builds that aren't installed count but never send.
//!
//! Counting is a few map updates on the UI thread; the file is written two seconds after the last count (one
//! one-shot timer); the post runs on a background thread (net.rs) and comes back through bg.rs.
//!
//! Where things are counted: `start` (launch, update), `track(app, "open")` when the bar shows, `track_row` when a
//! line is saved; other features call `track(app, "<event>")` (the core's `USAGE_EVENTS` lists the names it keeps).

use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use qcalc_core::app::analytics::{
    ANALYTICS_URL, CalcFacts, Stash, after_send, bump, calc_kind, due_to_send, local_day,
    sanitize_stash, usage_payload,
};
use qcalc_ffi::{HistoryKind, HistoryRow};
use serde_json::Value;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::app::{App, with_app};
use crate::model::now_ms;
use crate::qlog;

const SAVE_AFTER_MS: u32 = 2_000;

#[derive(Default)]
struct State {
    stash: Option<Stash>,
    sending: bool,
    timer: usize,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// 16 random bytes as 32 hex digits (`newInstallId`). std's RandomState is keyed from the OS's random source.
pub fn new_install_id() -> String {
    let half = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(now_ms() as u64);
        h.finish()
    };
    format!("{:016x}{:016x}", half(), half())
}

/// Minutes east of UTC now, from the local and UTC clocks (whole minutes, any zone).
pub fn utc_offset_minutes() -> f64 {
    // SAFETY: plain queries
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let minutes = |t: windows::Win32::Foundation::SYSTEMTIME| {
        days_from_civil(i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay)) * 1440
            + i64::from(t.wHour) * 60
            + i64::from(t.wMinute)
    };
    (minutes(local) - minutes(utc)) as f64
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn today() -> String {
    local_day(now_ms() as f64, utc_offset_minutes())
}

/// The facts the core buckets a saved line by, from the row the engine saved (BarModel.swift's trackCalc).
pub fn facts(typed: &str, row: Option<&HistoryRow>, chained: bool) -> CalcFacts {
    let lower = typed.trim().to_lowercase();
    CalcFacts {
        expr: typed.to_string(),
        system: row.is_some_and(|r| r.kind == Some(HistoryKind::System)),
        graph: lower.starts_with("graph"),
        function: row.is_some_and(|r| r.kind == Some(HistoryKind::Function)),
        solve: row.is_some_and(|r| r.solve.is_some()),
        assign: row.is_some_and(|r| r.kind == Some(HistoryKind::Definition)),
        unit: row.is_some_and(|r| r.quantity.is_some()),
        chained,
        native: false,
    }
}

/// The stash the store keeps, with `version` the running one: a new version counts `update`, a missing or
/// damaged stash starts over with a new id. Then `launch`.
pub fn resumed(raw: &Value, version: &str, day: &str, new_id: impl FnOnce() -> String) -> Stash {
    let mut next = match sanitize_stash(raw) {
        Some(kept) => {
            if !kept.version.is_empty() && kept.version != version {
                bump(&kept, "update", day)
            } else {
                kept
            }
        }
        None => Stash {
            id: new_id(),
            ..Stash::default()
        },
    };
    next.version = version.to_string();
    bump(&next, "launch", day)
}

/// Sharing on or off (launch, and the settings toggle).
pub fn set_sharing(app: &mut App, on: bool) {
    if !on {
        STATE.with(|s| s.borrow_mut().stash = None);
        if !app.store.analytics.is_null() {
            app.store.analytics = Value::Null;
            app.save_store();
        }
        return;
    }
    let stash = resumed(
        &app.store.analytics,
        crate::update::VERSION,
        &today(),
        new_install_id,
    );
    STATE.with(|s| s.borrow_mut().stash = Some(stash));
    persist(app);
    maybe_send(app);
}

/// At the first frame (bg.rs).
pub fn start(app: &mut App) {
    let on = app.store.settings.share_usage;
    set_sharing(app, on);
}

/// One more `event` today, if sharing is on.
pub fn track(app: &mut App, event: &str) {
    let changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(stash) = &s.stash else { return false };
        s.stash = Some(bump(stash, event, &today()));
        true
    });
    if changed {
        persist(app);
        maybe_send(app);
    }
}

/// A saved line: its bucket (`calc.arith`, `calc.units`...).
pub fn track_row(app: &mut App, typed: &str, row: Option<&HistoryRow>, chained: bool) {
    if STATE.with(|s| s.borrow().stash.is_none()) {
        return;
    }
    let kind = calc_kind(&facts(typed, row, chained));
    track(app, kind);
}

/// The stash into the store, and the file written two seconds after the last count.
fn persist(app: &mut App) {
    let host = app.host;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let value = s
            .stash
            .as_ref()
            .and_then(|st| serde_json::to_value(st).ok())
            .unwrap_or(Value::Null);
        app.store.analytics = value;
        // SAFETY: a thread timer with a callback, on the UI thread; replaced while counts keep coming
        s.timer = unsafe {
            if s.timer != 0 {
                let _ = KillTimer(None, s.timer);
            }
            SetTimer(None, 0, SAVE_AFTER_MS, Some(save_now))
        };
    });
    let _ = host;
}

unsafe extern "system" fn save_now(_: HWND, _: u32, id: usize, _: u32) {
    // SAFETY: our own one-shot timer
    let _ = unsafe { KillTimer(None, id) };
    STATE.with(|s| s.borrow_mut().timer = 0);
    with_app(|app| app.save_store());
}

/// Only an installed copy reports; a dev build counts and keeps its counts to itself.
fn may_send() -> bool {
    if std::env::var_os("QCALC_ANALYTICS_SEND").is_some() {
        return true;
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    crate::autostart::is_installed(&exe, &crate::autostart::Roots::from_env())
}

fn maybe_send(_app: &mut App) {
    let now = now_ms() as f64;
    let job = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.sending || !may_send() {
            return None;
        }
        let stash = s.stash.clone()?;
        if !due_to_send(&stash, now) || ANALYTICS_URL.is_empty() {
            return None;
        }
        s.sending = true;
        Some(stash)
    });
    let Some(stash) = job else { return };
    let body = usage_payload(&stash, "windows", Some(&os_name())).to_string();
    let sent: Vec<String> = stash.days.keys().cloned().collect();
    crate::bg::spawn("qcalc-usage", move || {
        let ok = match crate::net::post(ANALYTICS_URL, "text/plain", body.as_bytes()) {
            Ok(status) => (200..300).contains(&status),
            Err(e) => {
                qlog!("usage: post failed: {e}");
                false
            }
        };
        crate::bg::post(move |app| {
            let changed = STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.sending = false;
                match (&s.stash, ok) {
                    (Some(current), true) => {
                        s.stash = Some(after_send(current, &sent, &today(), now_ms() as f64));
                        true
                    }
                    _ => false,
                }
            });
            if changed {
                persist(app);
            }
        });
    });
}

/// "Windows 11 26200", as the Tauri app said it. RtlGetVersion, since the documented calls answer with Windows 8
/// unless the manifest names each later version.
pub fn os_name() -> String {
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
    }
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: a sized, live struct
    if unsafe { RtlGetVersion(&mut info) } != 0 {
        return "Windows".into();
    }
    // Windows 11 kept 10.0 as its version; its builds start at 22000
    let name = if info.dwMajorVersion >= 10 && info.dwBuildNumber >= 22000 {
        11
    } else {
        info.dwMajorVersion
    };
    format!("Windows {name} {}", info.dwBuildNumber)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn install_ids_are_32_hex_digits_and_differ() {
        let (a, b) = (new_install_id(), new_install_id());
        assert_eq!(a.len(), 32);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(a, b);
    }

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 10, 27), 20_753);
        // the core's local_day reads it back
        assert_eq!(local_day(20_753.0 * 86_400_000.0, 0.0), "2026-10-27");
    }

    #[test]
    fn a_launch_resumes_the_stash_and_counts_an_update_once() {
        let kept = json!({ "id": ID, "version": "2.0.15", "sentAt": 5, "days": { "2026-10-26": { "open": 2 } } });
        let s = resumed(&kept, "2.1.0", "2026-10-27", || unreachable!());
        assert_eq!(s.id, ID);
        assert_eq!(s.version, "2.1.0");
        assert_eq!(s.days["2026-10-27"]["update"], 1);
        assert_eq!(s.days["2026-10-27"]["launch"], 1);
        assert_eq!(s.days["2026-10-26"]["open"], 2);
        let again = resumed(
            &serde_json::to_value(&s).unwrap(),
            "2.1.0",
            "2026-10-27",
            || unreachable!(),
        );
        assert_eq!(again.days["2026-10-27"].get("update"), Some(&1));
        assert_eq!(again.days["2026-10-27"]["launch"], 2);
        // nothing kept, or junk: a new id
        let fresh = resumed(&Value::Null, "2.1.0", "2026-10-27", || ID.into());
        assert_eq!((fresh.id.as_str(), fresh.days.len()), (ID, 1));
        assert!(!fresh.days["2026-10-27"].contains_key("update"));
    }

    #[test]
    fn saved_rows_land_in_the_right_bucket() {
        let row = |kind: Option<HistoryKind>, quantity: Option<&str>| HistoryRow {
            id: "r".into(),
            expr: String::new(),
            display: String::new(),
            exact: None,
            n: None,
            meas: None,
            quantity: quantity.map(String::from),
            kind,
            equations: None,
            fn_name: None,
            fn_params: None,
            fn_body: None,
            solve: None,
            at: None,
        };
        assert_eq!(
            calc_kind(&facts("2+2", Some(&row(None, None)), false)),
            "calc.arith"
        );
        assert_eq!(
            calc_kind(&facts("3 ft to m", Some(&row(None, Some("length"))), false)),
            "calc.units"
        );
        assert_eq!(
            calc_kind(&facts(
                "x = 4",
                Some(&row(Some(HistoryKind::Definition), None)),
                false
            )),
            "calc.assign"
        );
        assert_eq!(calc_kind(&facts("graph x^2", None, false)), "calc.graph");
        assert_eq!(
            calc_kind(&facts("+ 3", Some(&row(None, None)), true)),
            "calc.chain"
        );
    }
}
