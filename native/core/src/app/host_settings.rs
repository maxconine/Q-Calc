//! What the Windows shell adds to its settings window (`src/lib/hostSettings.ts`): the show/hide chord and why it
//! failed, autostart, the bar's spot reset, and the update loop's state, read leniently, plus the lines the window
//! says about them. Portable, so a native Windows UI can share it.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::app::keybinds::chord_label;

/// The shell's update loop, as it last said (update.rs's Status).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateState {
    #[default]
    Off,
    Idle,
    Checking,
    Current,
    Downloading,
    Ready,
    Installing,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub state: UpdateState,
    pub latest: String,
    /// Unix seconds; 0 when never checked.
    pub checked_at: f64,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub hotkey_chord: String,
    pub hotkey_failed: bool,
    pub hotkey_refused: String,
    pub hotkey_taken: String,
    pub autostart: bool,
    pub spot_seconds: f64,
    pub version: String,
    pub update: UpdateStatus,
    pub auto_update: bool,
}

/// The Mac's 5 minutes.
pub const DEFAULT_SPOT_SECONDS: f64 = 300.0;

impl Default for HostInfo {
    fn default() -> Self {
        HostInfo {
            hotkey_chord: String::new(),
            hotkey_failed: false,
            hotkey_refused: String::new(),
            hotkey_taken: String::new(),
            autostart: false,
            spot_seconds: DEFAULT_SPOT_SECONDS,
            version: String::new(),
            update: UpdateStatus::default(),
            auto_update: true,
        }
    }
}

fn update_status(raw: Option<&Json>, base: &UpdateStatus) -> UpdateStatus {
    let Some(o) = raw.filter(|v| v.is_object()) else {
        return base.clone();
    };
    UpdateStatus {
        state: o
            .get("state")
            .and_then(|s| serde_json::from_value(s.clone()).ok())
            .unwrap_or(base.state),
        latest: o.get("latest").and_then(Json::as_str).unwrap_or("").into(),
        checked_at: o.get("checkedAt").and_then(Json::as_f64).unwrap_or(0.0),
        error: o.get("error").and_then(Json::as_str).unwrap_or("").into(),
    }
}

/// `hostInfo(raw, base)`: each field of `raw` with the right type over `base`; the refusal and the taken chord are
/// only ever about the change just made, so they never carry over.
pub fn host_info(raw: &Json, base: &HostInfo) -> HostInfo {
    let text = |k: &str, b: &str| raw.get(k).and_then(Json::as_str).unwrap_or(b).to_string();
    let flag = |k: &str, b: bool| raw.get(k).and_then(Json::as_bool).unwrap_or(b);
    HostInfo {
        hotkey_chord: text("hotkeyChord", &base.hotkey_chord),
        hotkey_failed: flag("hotkeyFailed", base.hotkey_failed),
        hotkey_refused: text("hotkeyRefused", ""),
        hotkey_taken: text("hotkeyTaken", ""),
        autostart: flag("autostart", base.autostart),
        spot_seconds: raw
            .get("spotSeconds")
            .and_then(Json::as_f64)
            .filter(|s| *s >= 0.0)
            .map_or(base.spot_seconds, crate::jsnum::round),
        version: text("version", &base.version),
        update: update_status(raw.get("update"), &base.update),
        auto_update: flag("autoUpdate", base.auto_update),
    }
}

// "5 minutes ago", for when updates were last checked
fn ago(then: f64, now: f64) -> String {
    let r = crate::jsnum::round;
    let s = r(now - then).max(0.0);
    if s < 60.0 {
        return "just now".into();
    }
    let m = r(s / 60.0);
    if m < 60.0 {
        return if m == 1.0 {
            "1 minute ago".into()
        } else {
            format!("{m} minutes ago")
        };
    }
    let h = r(m / 60.0);
    if h < 24.0 {
        return if h == 1.0 {
            "1 hour ago".into()
        } else {
            format!("{h} hours ago")
        };
    }
    let d = r(h / 24.0);
    if d == 1.0 {
        "yesterday".into()
    } else {
        format!("{d} days ago")
    }
}

/// The line under "Software update"; `now` is unix seconds.
pub fn update_line(u: &UpdateStatus, auto: bool, now: f64) -> String {
    let checked = if u.checked_at != 0.0 {
        format!(" · checked {}", ago(u.checked_at, now))
    } else {
        String::new()
    };
    let latest = &u.latest;
    match u.state {
        UpdateState::Off => "This copy of Q Calc can’t update itself.".into(),
        UpdateState::Idle if auto => {
            "Checks for updates shortly after Q Calc starts, then daily.".into()
        }
        UpdateState::Idle => "Not checked yet.".into(),
        UpdateState::Checking => "Checking for updates…".into(),
        UpdateState::Current => format!("Up to date{checked}"),
        UpdateState::Downloading => format!("Downloading {latest}…"),
        UpdateState::Ready if auto => {
            format!(
                "{latest} is ready. It installs when Q Calc is put away and you’re idle, or now."
            )
        }
        UpdateState::Ready => format!("{latest} is ready to install."),
        UpdateState::Installing => format!("Installing {latest}…"),
        UpdateState::Failed => {
            let why = if u.error.is_empty() {
                String::new()
            } else {
                format!(": {}", u.error)
            };
            format!("Couldn’t update{why}{checked}")
        }
    }
}

/// The shell's answer when the settings page tries a chord.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyVerdict {
    pub chord: String,
    pub usable: bool,
    pub free: bool,
    pub owner: String,
    pub sure: bool,
    pub suggest: String,
}

pub fn hotkey_verdict(raw: &Json) -> Option<HotkeyVerdict> {
    let chord = raw.get("chord")?.as_str()?.to_string();
    let free = raw.get("free")?.as_bool()?;
    let text = |k: &str| raw.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    Some(HotkeyVerdict {
        chord,
        usable: raw.get("usable") != Some(&Json::Bool(false)),
        free,
        owner: text("owner"),
        sure: raw.get("sure") == Some(&Json::Bool(true)),
        suggest: text("suggest"),
    })
}

/// Launchers the shell guesses hold a shortcut: id, name, where to change it.
pub const HOTKEY_OWNERS: [(&str, &str, &str); 6] = [
    (
        "powertoys-run",
        "PowerToys Run",
        "PowerToys → PowerToys Run → Activation shortcut",
    ),
    (
        "command-palette",
        "Command Palette",
        "PowerToys → Command Palette → Activation shortcut",
    ),
    (
        "flow-launcher",
        "Flow Launcher",
        "Flow Launcher’s settings → Hotkey",
    ),
    ("wox", "Wox", "Wox’s settings → Hotkey"),
    ("raycast", "Raycast", "Raycast’s settings"),
    (
        "keypirinha",
        "Keypirinha",
        "Keypirinha’s configuration (hotkey_run)",
    ),
];

/// "Alt+Space is used by PowerToys Run. Change it in …, or pick another here. Ctrl+Alt+Space is free."
pub fn conflict_text(v: &HotkeyVerdict) -> String {
    let key = chord_label(&v.chord, true);
    let free = if v.suggest.is_empty() {
        String::new()
    } else {
        format!(" {} is free.", chord_label(&v.suggest, true))
    };
    if !v.usable {
        return format!("Q Calc can’t use {key} as a shortcut. Pick another.{free}");
    }
    if let Some((_, name, fix)) = HOTKEY_OWNERS.iter().find(|o| o.0 == v.owner) {
        let who = if v.sure {
            format!("{key} is used by {name}.")
        } else {
            format!("{key} is in use, probably by {name}.")
        };
        return format!("{who} Change it in {fix}, or pick another here.{free}");
    }
    if v.owner == "windows" {
        return format!("{key} is in use, probably by Windows. Pick another.{free}");
    }
    format!("{key} is in use by another app. Pick another.{free}")
}

/// "" when the shortcut is fine; with the shell's verdict on the taken chord, the note names who holds it.
pub fn hotkey_note(host: &HostInfo, verdict: Option<&HotkeyVerdict>) -> String {
    if let Some(v) = verdict
        && !v.free
        && !host.hotkey_taken.is_empty()
        && v.chord == host.hotkey_taken
    {
        // a launch that fell back opens with another chord meanwhile
        let fell_back = host.hotkey_failed
            && host.hotkey_refused.is_empty()
            && !host.hotkey_chord.is_empty()
            && host.hotkey_chord != host.hotkey_taken;
        let tail = if fell_back {
            format!(
                " {} opens Q Calc for now.",
                chord_label(&host.hotkey_chord, true)
            )
        } else {
            String::new()
        };
        return format!("{}{tail}", conflict_text(v));
    }
    if !host.hotkey_refused.is_empty() {
        return format!("{} is in use by another app", host.hotkey_refused);
    }
    if host.hotkey_failed {
        return "Your show / hide shortcut is in use by another app".into();
    }
    String::new()
}

const DRAFT_CHOICES: [(u32, &str); 5] = [
    (0, "Don’t keep"),
    (30, "30 seconds"),
    (60, "1 minute"),
    (120, "2 minutes"),
    (300, "5 minutes"),
];

const SPOT_CHOICES: [(u32, &str); 5] = [
    (0, "Never"),
    (60, "1 minute"),
    (300, "5 minutes"),
    (900, "15 minutes"),
    (3600, "1 hour"),
];

/// The spot-reset choices; a value set some other way still shows.
pub fn spot_choices(current: u32) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = SPOT_CHOICES
        .iter()
        .map(|(i, l)| (*i, (*l).to_string()))
        .collect();
    if !SPOT_CHOICES.iter().any(|c| c.0 == current) {
        let label = if current % 60 == 0 {
            format!("{} minutes", current / 60)
        } else {
            format!("{current} seconds")
        };
        out.push((current, label));
    }
    out
}

/// The draft choices; a value set some other way still shows, as on the Mac.
pub fn draft_choices(current: u32) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = DRAFT_CHOICES
        .iter()
        .map(|(i, l)| (*i, (*l).to_string()))
        .collect();
    if !DRAFT_CHOICES.iter().any(|c| c.0 == current) {
        out.push((current, format!("{current} seconds")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_lines() {
        let h = host_info(
            &json!({"hotkeyChord": "alt+space", "spotSeconds": 59.6, "update": {"state": "ready", "latest": "1.2"}}),
            &HostInfo::default(),
        );
        assert_eq!(h.spot_seconds, 60.0);
        assert_eq!(
            update_line(&h.update, false, 0.0),
            "1.2 is ready to install."
        );
        let v = hotkey_verdict(&json!({"chord": "alt+space", "free": false, "owner": "powertoys-run", "suggest": "ctrl+alt+space"})).unwrap();
        assert_eq!(
            conflict_text(&v),
            "Alt+Space is in use, probably by PowerToys Run. Change it in PowerToys → PowerToys Run → Activation shortcut, or pick another here. Ctrl+Alt+Space is free."
        );
        assert_eq!(
            spot_choices(30).last().map(|c| c.1.as_str()),
            Some("30 seconds")
        );
    }
}
