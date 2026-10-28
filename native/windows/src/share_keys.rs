//! This app's end-to-end key and each contact's pinned one (the Mac's ShareKeys.swift; native/PRIVACY.md).
//!
//! The X25519 private key is made the first time it's needed and kept in the app's data folder (so each instance,
//! `QCALC_DATA` copy and test build has its own) as `share-key.bin`, protected with DPAPI for this Windows user
//! (CryptProtectData: never in plain text, unreadable to another user or machine). Its public half is published
//! with the name on the relay (`POST /pubkey`, with the name's own key) and read back by others through
//! `/lookup`. A contact's key is pinned (`share-pins.json`) the first time it's used; if the relay later hands
//! back another, nothing more goes to or comes from them until it's trusted, after comparing the safety code.
//!
//! No name, no key: nothing here is read or made until messaging has a name. Every relay call blocks, so they run
//! on worker threads (messaging.rs's), never the UI thread.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use qcalc_core::app::messaging as core;
use serde_json::{Value, json};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::share_crypt as crypt;

const KEY_FILE: &str = "share-key.bin";
const PINS_FILE: &str = "share-pins.json";
/// DPAPI's extra entropy: the key opens only for this purpose too
const ENTROPY: &[u8] = b"qcalc-share-key";

#[derive(Default)]
struct Keys {
    secret: Option<StaticSecret>,
    pins: Option<BTreeMap<String, String>>,
    /// who → the key the relay has now, while it isn't the pinned one
    changed: BTreeMap<String, String>,
    /// "name:pub" last published this run
    published: String,
}

fn keys() -> &'static Mutex<Keys> {
    static K: OnceLock<Mutex<Keys>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(Keys::default()))
}

fn with<R>(f: impl FnOnce(&mut Keys) -> R) -> R {
    let mut k = keys().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut k)
}

fn path(file: &str) -> Option<PathBuf> {
    crate::store::config_dir().map(|d| d.join(file))
}

// ---- DPAPI ----

fn blob(data: &[u8]) -> windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
    windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

fn take(out: windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: DPAPI's output buffer of cbData bytes, freed once copied
    unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            out.pbData.cast(),
        )));
        v
    }
}

/// `data` sealed for this Windows user (DPAPI).
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let entropy = blob(ENTROPY);
    // SAFETY: live input blobs for the call; the output is taken (and freed) only on success
    unsafe {
        CryptProtectData(
            &blob(data),
            windows::core::w!("Q Calc sharing key"),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .ok()?;
    }
    Some(take(out))
}

/// What `protect` sealed, for this user only.
pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let entropy = blob(ENTROPY);
    // SAFETY: as in protect
    unsafe {
        CryptUnprotectData(
            &blob(data),
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .ok()?;
    }
    Some(take(out))
}

// ---- this app's key ----

fn read_key() -> Option<StaticSecret> {
    let sealed = std::fs::read(path(KEY_FILE)?).ok()?;
    let raw: [u8; 32] = unprotect(&sealed)?.try_into().ok()?;
    Some(StaticSecret::from(raw))
}

fn write_key(secret: &StaticSecret) -> bool {
    let Some(p) = path(KEY_FILE) else {
        return false;
    };
    let Some(sealed) = protect(secret.as_bytes()) else {
        return false;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // written beside, then moved, so a crash never leaves half a key
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, &sealed).is_ok()
        && std::fs::rename(&tmp, &p).is_ok()
        && read_key().is_some_and(|k| k.as_bytes() == secret.as_bytes())
}

/// The private key: read once, or made and kept; None only if it can't be kept.
pub fn secret() -> Option<StaticSecret> {
    if let Some(s) = with(|k| k.secret.clone()) {
        return Some(s);
    }
    let s = match read_key() {
        Some(s) => s,
        None => {
            let s = crypt::new_secret();
            if !write_key(&s) {
                crate::qlog!("share keys: couldn't keep a new key");
                return None;
            }
            s
        }
    };
    with(|k| k.secret = Some(s.clone()));
    Some(s)
}

pub fn public_text() -> String {
    secret().map(|s| crypt::public_text(&s)).unwrap_or_default()
}

// ---- pins ----

fn pins_now(k: &mut Keys) -> &mut BTreeMap<String, String> {
    k.pins.get_or_insert_with(|| {
        path(PINS_FILE)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    })
}

fn pin(who: &str, key: &str) {
    let saved = with(|k| {
        let pins = pins_now(k);
        pins.insert(who.to_string(), key.to_string());
        serde_json::to_vec_pretty(pins).ok()
    });
    if let (Some(p), Some(b)) = (path(PINS_FILE), saved) {
        let _ = std::fs::write(p, b);
    }
}

fn pinned(who: &str) -> Option<String> {
    with(|k| pins_now(k).get(who).cloned())
}

pub fn changed(who: &str) -> bool {
    with(|k| k.changed.contains_key(who))
}

// ---- the relay ----

/// Publishes this app's public key with the name (once a run, and after a claim). Blocking: a worker thread.
pub fn publish(name: &str, relay_key: &str) {
    if name.is_empty() || relay_key.is_empty() {
        return;
    }
    let pub_text = public_text();
    let tag = format!("{name}:{pub_text}");
    if pub_text.is_empty() || with(|k| k.published == tag) {
        return;
    }
    let status = crate::relay::post(
        "/pubkey",
        &json!({ "name": name, "key": relay_key, "pub": pub_text }),
    );
    if status == "ok" {
        with(|k| k.published = tag);
    } else {
        crate::qlog!("share keys: publishing said {status}");
    }
}

/// Something sealed for this app didn't open: publish again even if this run already did.
pub fn republish(name: &str, relay_key: &str) {
    with(|k| k.published.clear());
    publish(name, relay_key);
}

/// What's known of a contact's key.
#[derive(Clone, Debug)]
pub enum Found {
    Key(PublicKey, String),
    /// not the pinned one: nothing goes until it's trusted
    Changed,
    /// they hold the name but have published no key (an older copy)
    None,
    Unknown,
    /// the relay couldn't say (its status)
    Offline(String),
}

fn lookup(name: &str, relay_key: &str, who: &str) -> (String, String) {
    if name.is_empty() || relay_key.is_empty() {
        return ("not-yours".into(), String::new());
    }
    let (status, v) = crate::relay::post_value(
        "/lookup",
        &json!({ "from": name, "key": relay_key, "name": who }),
    );
    (status, v["pub"].as_str().unwrap_or("").to_string())
}

fn found(who: &str, key: &str) -> Found {
    if key.is_empty() {
        return Found::None;
    }
    let pins = with(|k| serde_json::to_value(pins_now(k)).unwrap_or(Value::Null));
    match core::pin_state(&pins, who, key) {
        "new" => pin(who, key),
        "same" => {}
        "changed" => {
            with(|k| k.changed.insert(who.to_string(), key.to_string()));
            return Found::Changed;
        }
        _ => return Found::None,
    }
    with(|k| k.changed.remove(who));
    crypt::public_key(key).map_or(Found::None, |p| Found::Key(p, key.to_string()))
}

/// Before sending: the relay's key for `who`, checked against the pin (pinned the first time). Blocking.
pub fn for_sending(name: &str, relay_key: &str, who: &str) -> Found {
    publish(name, relay_key);
    match lookup(name, relay_key, who) {
        (s, key) if s == "found" => found(who, &key),
        (s, _) if s == "unknown" => Found::Unknown,
        (s, _) => Found::Offline(s),
    }
}

/// On receiving: the pinned key at once, or the relay's, pinned, the first time. Blocking.
pub fn for_receiving(name: &str, relay_key: &str, who: &str) -> Found {
    if changed(who) {
        return Found::Changed;
    }
    if let Some(p) = pinned(who)
        && let Some(key) = crypt::public_key(&p)
    {
        return Found::Key(key, p);
    }
    for_sending(name, relay_key, who)
}

/// Something from `who` didn't open with the pinned key: true when the relay now has another. Blocking.
pub fn recheck(name: &str, relay_key: &str, who: &str) -> bool {
    let (status, key) = lookup(name, relay_key, who);
    if status != "found" || key.is_empty() || pinned(who).is_none_or(|p| p == key) {
        return false;
    }
    with(|k| k.changed.insert(who.to_string(), key));
    true
}

/// `trust <name>`: the new key is pinned. True when there was one waiting.
pub fn trust(who: &str) -> bool {
    let Some(key) = with(|k| k.changed.remove(who)) else {
        return false;
    };
    pin(who, &key);
    true
}

/// The code both people compare out loud; "" until their key is known.
pub fn safety_code(who: &str) -> String {
    let theirs = with(|k| k.changed.get(who).cloned()).or_else(|| pinned(who));
    let Some(theirs) = theirs else {
        return String::new();
    };
    core::safety_code(&public_text(), &theirs).unwrap_or_default()
}

/// Forgets a contact's pin (Remove / Unblock in settings).
pub fn forget(who: &str) {
    let saved = with(|k| {
        k.changed.remove(who);
        let pins = pins_now(k);
        pins.remove(who)
            .map(|_| serde_json::to_vec_pretty(pins).ok())
    });
    if let (Some(p), Some(Some(b))) = (path(PINS_FILE), saved) {
        let _ = std::fs::write(p, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trips_and_hides_the_key() {
        let secret = [7u8; 32];
        let sealed = protect(&secret).unwrap();
        assert!(!sealed.windows(32).any(|w| w == secret));
        assert_eq!(unprotect(&sealed).unwrap(), secret);
        let mut bad = sealed.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(unprotect(&bad).is_none());
    }
}
