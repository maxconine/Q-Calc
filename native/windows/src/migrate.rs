//! Moving in from the Tauri app, once. Its settings, shortcut, onboarding and usage counts are in `settings.json`,
//! which both apps share. Its history is not: the page kept it in the web view's localStorage, which WebView2
//! stores as a LevelDB under `%LOCALAPPDATA%\com.maxconine.qcalc\EBWebView\Default\Local Storage\leveldb`.
//!
//! `history` reads the `qcalc-history` key out of that LevelDB (and the older `instant-solver-history`) with the
//! small reader below and saves it as the engine's `history.json`, before the tape is first loaded. It runs on the
//! engine thread (Lane::Misc, queued ahead of `history::load`) and only while there is no `history.json` yet, so
//! once there is one (migrated, or the native app's own) it costs a single file check.
//!
//! The reader: the write-ahead logs (`*.log`: 32 KB blocks of records making write batches) and the tables
//! (`*.ldb`/`*.sst`: blocks found through the index block, Snappy-compressed or not, keys prefix-shared). The
//! newest write wins by sequence number, and a deletion counts. Chromium's localStorage keys are
//! `_<origin>\0\x01<key>`; values are `\x01` + Latin-1 or `\0` + UTF-16LE. Nothing here writes to the LevelDB.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::qlog;

/// A key's newest write: its sequence number and value (None: deleted).
type Newest = HashMap<Vec<u8>, (u64, Option<Vec<u8>>)>;

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i)?;
        *i += 1;
        out |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

fn slice<'a>(b: &'a [u8], i: &mut usize, n: usize) -> Option<&'a [u8]> {
    let s = b.get(*i..i.checked_add(n)?)?;
    *i += n;
    Some(s)
}

fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn note(newest: &mut Newest, key: &[u8], seq: u64, value: Option<&[u8]>) {
    let slot = newest.entry(key.to_vec()).or_insert((0, None));
    if seq >= slot.0 {
        *slot = (seq, value.map(<[u8]>::to_vec));
    }
}

/// Snappy's raw format (the block compression LevelDB uses).
pub fn snappy(src: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    let len = usize::try_from(varint(src, &mut i)?).ok()?;
    if len > 64 << 20 {
        return None;
    }
    let mut out = Vec::with_capacity(len);
    while i < src.len() {
        let tag = src[i];
        i += 1;
        let (offset, n) = match tag & 3 {
            0 => {
                let mut n = usize::from(tag >> 2) + 1;
                if n > 60 {
                    let extra = n - 60;
                    let mut v = 0usize;
                    for k in 0..extra {
                        v |= usize::from(*src.get(i + k)?) << (8 * k);
                    }
                    i += extra;
                    n = v + 1;
                }
                out.extend_from_slice(slice(src, &mut i, n)?);
                continue;
            }
            1 => {
                let n = usize::from((tag >> 2) & 7) + 4;
                let off = (usize::from(tag >> 5) << 8) | usize::from(*src.get(i)?);
                i += 1;
                (off, n)
            }
            2 => {
                let off = usize::from(u16::from_le_bytes(src.get(i..i + 2)?.try_into().ok()?));
                i += 2;
                (off, usize::from(tag >> 2) + 1)
            }
            _ => {
                let off = u32le(src, i)? as usize;
                i += 4;
                (off, usize::from(tag >> 2) + 1)
            }
        };
        if offset == 0 || offset > out.len() {
            return None;
        }
        let start = out.len() - offset;
        for k in 0..n {
            out.push(out[start + k]);
        }
    }
    (out.len() == len).then_some(out)
}

/// One write batch: sequence, count, then (tag, key, value?) records.
fn batch(b: &[u8], newest: &mut Newest) -> Option<()> {
    let seq = u64::from_le_bytes(b.get(0..8)?.try_into().ok()?);
    let count = u32le(b, 8)?;
    let mut i = 12;
    for k in 0..u64::from(count) {
        let tag = *b.get(i)?;
        i += 1;
        let klen = usize::try_from(varint(b, &mut i)?).ok()?;
        let key = slice(b, &mut i, klen)?;
        match tag {
            1 => {
                let vlen = usize::try_from(varint(b, &mut i)?).ok()?;
                let value = slice(b, &mut i, vlen)?;
                note(newest, key, seq + k, Some(value));
            }
            0 => note(newest, key, seq + k, None),
            _ => return None,
        }
    }
    Some(())
}

/// A write-ahead log: 32 KB blocks of 7-byte-headed records (full, first, middle, last) making batches.
pub fn read_log(data: &[u8], newest: &mut Newest) {
    const BLOCK: usize = 32 * 1024;
    let mut pending: Vec<u8> = Vec::new();
    let mut at = 0;
    while at + 7 <= data.len() {
        let left_in_block = BLOCK - at % BLOCK;
        if left_in_block < 7 {
            at += left_in_block;
            continue;
        }
        let len = usize::from(u16::from_le_bytes([data[at + 4], data[at + 5]]));
        let kind = data[at + 6];
        let Some(body) = data.get(at + 7..at + 7 + len) else {
            break;
        };
        at += 7 + len;
        match kind {
            1 => {
                let _ = batch(body, newest);
                pending.clear();
            }
            2 => pending = body.to_vec(),
            3 => pending.extend_from_slice(body),
            4 => {
                pending.extend_from_slice(body);
                let _ = batch(&pending, newest);
                pending.clear();
            }
            // zero padding at a block's end
            _ => {}
        }
    }
}

/// A table block's bytes, uncompressed, without its trailer.
fn block(file: &[u8], offset: u64, size: u64) -> Option<Vec<u8>> {
    let (o, s) = (usize::try_from(offset).ok()?, usize::try_from(size).ok()?);
    let raw = file.get(o..o.checked_add(s)?)?;
    match *file.get(o + s)? {
        0 => Some(raw.to_vec()),
        1 => snappy(raw),
        _ => None,
    }
}

/// A block's entries (key, value), prefix-shared keys spelled out.
fn entries(b: &[u8]) -> Option<Vec<(Vec<u8>, &[u8])>> {
    let restarts = u32le(b, b.len().checked_sub(4)?)? as usize;
    let end = b.len().checked_sub(4 + 4 * restarts)?;
    let mut out: Vec<(Vec<u8>, &[u8])> = Vec::new();
    let mut i = 0;
    let mut last: Vec<u8> = Vec::new();
    while i < end {
        let shared = usize::try_from(varint(b, &mut i)?).ok()?;
        let unshared = usize::try_from(varint(b, &mut i)?).ok()?;
        let vlen = usize::try_from(varint(b, &mut i)?).ok()?;
        let mut key = last.get(..shared)?.to_vec();
        key.extend_from_slice(slice(b, &mut i, unshared)?);
        let value = slice(b, &mut i, vlen)?;
        last.clone_from(&key);
        out.push((key, value));
    }
    Some(out)
}

/// A table file: the footer's index block, each data block it points at, internal keys `user key + 8-byte
/// trailer (seq << 8 | type)`.
pub fn read_table(file: &[u8], newest: &mut Newest) -> Option<()> {
    let footer = file.get(file.len().checked_sub(48)?..)?;
    if footer[40..48] != 0xdb47_7524_8b80_fb57u64.to_le_bytes() {
        return None;
    }
    let mut i = 0;
    let _meta = (varint(footer, &mut i)?, varint(footer, &mut i)?);
    let index = block(file, varint(footer, &mut i)?, varint(footer, &mut i)?)?;
    for (_, handle) in entries(&index)? {
        let mut j = 0;
        let data = block(file, varint(handle, &mut j)?, varint(handle, &mut j)?)?;
        for (ikey, value) in entries(&data)? {
            let Some(split) = ikey.len().checked_sub(8) else {
                continue;
            };
            let trailer = u64::from_le_bytes(ikey[split..].try_into().ok()?);
            let (seq, kind) = (trailer >> 8, trailer & 0xff);
            note(newest, &ikey[..split], seq, (kind == 1).then_some(value));
        }
    }
    Some(())
}

/// Every key's newest value in a LevelDB folder.
pub fn read_db(dir: &Path) -> Newest {
    let mut newest = Newest::new();
    let Ok(list) = std::fs::read_dir(dir) else {
        return newest;
    };
    for entry in list.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        match ext.as_str() {
            "log" => read_log(&data, &mut newest),
            "ldb" | "sst" if read_table(&data, &mut newest).is_none() => {
                qlog!("migrate: couldn't read {}", path.display());
            }
            _ => {}
        }
    }
    newest
}

/// A localStorage value as text: `\x01` + Latin-1, or `\0` + UTF-16LE.
pub fn storage_text(value: &[u8]) -> Option<String> {
    match value.split_first()? {
        (1, rest) => Some(rest.iter().map(|&b| char::from(b)).collect()),
        (0, rest) if rest.len() % 2 == 0 => {
            let units: Vec<u16> = rest
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

/// The page's value for `key` at any origin (the Tauri page was `http://tauri.localhost`).
pub fn local_storage(newest: &Newest, key: &str) -> Option<String> {
    let mut suffix = vec![0u8, 1u8];
    suffix.extend_from_slice(key.as_bytes());
    newest
        .iter()
        .filter(|(k, _)| k.first() == Some(&b'_') && k.ends_with(&suffix))
        .max_by_key(|(_, (seq, _))| *seq)
        .and_then(|(_, (_, v))| v.as_deref())
        .and_then(storage_text)
}

/// Where the Tauri app's web view kept localStorage.
pub fn tauri_leveldb() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join(r"com.maxconine.qcalc\EBWebView\Default\Local Storage\leveldb"))
}

/// The Tauri page's history JSON, if it left one.
pub fn tauri_history(dir: &Path) -> Option<String> {
    let db = read_db(dir);
    ["qcalc-history", "instant-solver-history"]
        .iter()
        .find_map(|k| local_storage(&db, k))
        .filter(|t| t.trim_start().starts_with('['))
}

/// Written once the move has been made (or found nothing), so later launches cost one file check.
pub const MARKER: &str = "tauri-migrated.json";

/// What the Tauri page left in its localStorage that the native app keeps in files of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moves {
    /// the page's `qcalc-history` (or `instant-solver-history`) JSON, for the engine's `history.json`
    pub history: Option<String>,
    /// `qcalc-messaging` (`{name, key, accepted, blocked, recent}`), for `messaging.json`, in the same shape
    pub messaging: Option<String>,
    /// the game saves and the room nickname, as `games.json` (`{gameSaves, nickname}`, games/store.rs)
    pub games: Option<String>,
}

/// The moves a Tauri LevelDB holds.
pub fn plan(db: &Newest) -> Moves {
    let history = ["qcalc-history", "instant-solver-history"]
        .iter()
        .find_map(|k| local_storage(db, k))
        .filter(|t| t.trim_start().starts_with('['));
    // a name without its key can't be had back from the relay, so the pair moves together, and lists alone too
    let messaging = local_storage(db, "qcalc-messaging").filter(|t| {
        serde_json::from_str::<serde_json::Value>(t).is_ok_and(|v| {
            v.is_object()
                && (v["name"].as_str().is_some_and(|n| !n.is_empty())
                    || ["accepted", "blocked", "recent"]
                        .iter()
                        .any(|k| v[*k].as_array().is_some_and(|a| !a.is_empty())))
        })
    });
    let mut saves = serde_json::Map::new();
    for key in qcalc_core::games::saves::GAME_SAVE_KEYS {
        if let Some(v) = local_storage(db, key)
            && v.encode_utf16().count() <= qcalc_core::games::saves::GAME_SAVE_MAX
        {
            saves.insert(key.to_string(), serde_json::Value::String(v));
        }
    }
    let nickname = local_storage(db, "qcalc-nickname")
        .map(|n| qcalc_core::games::nickname::clean_nickname(&n))
        .unwrap_or_default();
    let games = (!saves.is_empty() || !nickname.is_empty()).then(|| {
        serde_json::to_string_pretty(
            &serde_json::json!({ "gameSaves": saves, "nickname": nickname }),
        )
        .unwrap_or_default()
    });
    Moves {
        history,
        messaging,
        games,
    }
}

/// Queued on the engine thread before the tape loads (Misc runs first): the Tauri app's history, messaging name
/// and lists, and game saves, each into its file only if that file isn't there yet, then the marker.
pub fn history(engine: &crate::engine::EngineThread) {
    // QCALC_DATA points tests and the bench at a folder of their own; they never take a real user's history
    if std::env::var_os("QCALC_DATA").is_some() && std::env::var_os("QCALC_MIGRATE").is_none() {
        return;
    }
    let Some(data) = crate::store::config_dir() else {
        return;
    };
    let Some(dir) = std::env::var_os("QCALC_MIGRATE")
        .map(PathBuf::from)
        .or_else(tauri_leveldb)
    else {
        return;
    };
    if data.join(MARKER).exists() || !dir.is_dir() {
        return;
    }
    engine.submit(crate::engine::Lane::Misc, move |cx| {
        let t = std::time::Instant::now();
        let moves = plan(&read_db(&dir));
        let mut rows_moved = 0usize;
        if let Some(json) = moves.history
            && !data.join("history.json").exists()
        {
            let rows = cx.engine.history_from_json(json);
            rows_moved = rows.len();
            if let Err(e) = cx.engine.save_history(rows) {
                qlog!("migrate: couldn't save the history: {e}");
                // tried again next launch
                return None;
            }
        }
        let put = |file: &str, text: Option<String>| -> bool {
            let path = data.join(file);
            match text {
                Some(text) if !path.exists() => match crate::store::write_atomic(&path, &text) {
                    Ok(()) => true,
                    Err(e) => {
                        qlog!("migrate: couldn't write {file}: {e}");
                        false
                    }
                },
                _ => false,
            }
        };
        let messaging = put("messaging.json", moves.messaging);
        let games = put("games.json", moves.games);
        let note = serde_json::json!({
            "at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            "historyRows": rows_moved, "messaging": messaging, "games": games,
        });
        let _ = crate::store::write_atomic(&data.join(MARKER), &note.to_string());
        qlog!(
            "migrate: from the Tauri app: {rows_moved} history rows, messaging {messaging}, games {games} in {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        None
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn batch_bytes(seq: u64, ops: &[(&[u8], Option<&[u8]>)]) -> Vec<u8> {
        let mut b = seq.to_le_bytes().to_vec();
        b.extend_from_slice(&(ops.len() as u32).to_le_bytes());
        for (k, v) in ops {
            b.push(u8::from(v.is_some()));
            put_varint(&mut b, k.len() as u64);
            b.extend_from_slice(k);
            if let Some(v) = v {
                put_varint(&mut b, v.len() as u64);
                b.extend_from_slice(v);
            }
        }
        b
    }

    fn record(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut r = vec![0, 0, 0, 0];
        r.extend_from_slice(&(body.len() as u16).to_le_bytes());
        r.push(kind);
        r.extend_from_slice(body);
        r
    }

    fn key(name: &str) -> Vec<u8> {
        let mut k = b"_http://tauri.localhost\0\x01".to_vec();
        k.extend_from_slice(name.as_bytes());
        k
    }

    fn utf16(s: &str) -> Vec<u8> {
        let mut v = vec![0u8];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn the_log_keeps_the_newest_write_and_deletions() {
        let hist = key("qcalc-history");
        let mut log = record(1, &batch_bytes(5, &[(&hist, Some(b"\x01[]"))]));
        // a later batch split over first/last records
        let b = batch_bytes(9, &[(&hist, Some(&utf16(r#"[{"expr":"2×π"}]"#)))]);
        log.extend(record(2, &b[..10]));
        log.extend(record(4, &b[10..]));
        let mut db = Newest::new();
        read_log(&log, &mut db);
        assert_eq!(
            local_storage(&db, "qcalc-history").as_deref(),
            Some(r#"[{"expr":"2×π"}]"#)
        );
        let mut later = log.clone();
        later.extend(record(1, &batch_bytes(12, &[(&hist, None)])));
        let mut db = Newest::new();
        read_log(&later, &mut db);
        assert_eq!(local_storage(&db, "qcalc-history"), None);
    }

    #[test]
    fn snappy_literals_and_copies() {
        // "abcabcabcabcX": literal "abc", copy offset 3 length 9, literal "X"
        let src = [
            13u8,
            2 << 2,
            b'a',
            b'b',
            b'c',
            1 | ((9 - 4) << 2),
            3,
            0,
            b'X',
        ];
        assert_eq!(snappy(&src).unwrap(), b"abcabcabcabcX");
        assert!(snappy(&[5u8, 0, b'a']).is_none());
    }

    #[test]
    fn a_table_file_reads_through_its_index() {
        // one data block holding two internal keys, an index block pointing at it, then the footer
        let mut data = Vec::new();
        let entry = |out: &mut Vec<u8>, shared: usize, k: &[u8], v: &[u8]| {
            put_varint(out, shared as u64);
            put_varint(out, (k.len() - shared) as u64);
            put_varint(out, v.len() as u64);
            out.extend_from_slice(&k[shared..]);
            out.extend_from_slice(v);
        };
        let ikey = |user: &[u8], seq: u64, kind: u64| {
            let mut k = user.to_vec();
            k.extend_from_slice(&((seq << 8) | kind).to_le_bytes());
            k
        };
        let a = ikey(&key("qcalc-draft"), 3, 1);
        let h = ikey(&key("qcalc-history"), 4, 1);
        let shared = a.iter().zip(&h).take_while(|(x, y)| x == y).count();
        entry(&mut data, 0, &a, b"\x01{}");
        entry(&mut data, shared, &h, b"\x01[1]");
        data.extend_from_slice(&0u32.to_le_bytes()[..0]);
        data.extend_from_slice(&0u32.to_le_bytes()); // restart at 0
        data.extend_from_slice(&1u32.to_le_bytes());
        let mut file = data.clone();
        file.extend_from_slice(&[0, 0, 0, 0, 0]); // trailer: uncompressed, crc
        let mut handle = Vec::new();
        put_varint(&mut handle, 0);
        put_varint(&mut handle, data.len() as u64);
        let index_at = file.len();
        let mut index = Vec::new();
        entry(&mut index, 0, &h, &handle);
        index.extend_from_slice(&0u32.to_le_bytes());
        index.extend_from_slice(&1u32.to_le_bytes());
        file.extend_from_slice(&index);
        file.extend_from_slice(&[0, 0, 0, 0, 0]);
        let mut footer = Vec::new();
        put_varint(&mut footer, 0);
        put_varint(&mut footer, 0);
        put_varint(&mut footer, index_at as u64);
        put_varint(&mut footer, index.len() as u64);
        footer.resize(40, 0);
        footer.extend_from_slice(&0xdb47_7524_8b80_fb57u64.to_le_bytes());
        file.extend_from_slice(&footer);
        let mut db = Newest::new();
        assert!(read_table(&file, &mut db).is_some());
        assert_eq!(local_storage(&db, "qcalc-history").as_deref(), Some("[1]"));
        // a newer log write beats the table's
        read_log(
            &record(
                1,
                &batch_bytes(7, &[(&key("qcalc-history"), Some(b"\x01[2]"))]),
            ),
            &mut db,
        );
        assert_eq!(local_storage(&db, "qcalc-history").as_deref(), Some("[2]"));
    }

    fn fixture(rest: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(r"tests\fixtures\tauri")
            .join(rest)
    }

    /// The Tauri app's real on-disk shapes (tests/fixtures/tauri/make.ps1: Chromium's own LevelDB, a table and a
    /// newer log over it; the Tauri store's settings.json).
    #[test]
    fn the_tauri_fixture_moves_whole() {
        let db = read_db(&fixture("leveldb"));
        let moves = plan(&db);
        let history: serde_json::Value =
            serde_json::from_str(moves.history.as_deref().unwrap()).unwrap();
        let exprs: Vec<&str> = history
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["expr"].as_str().unwrap())
            .collect();
        // the log's newer tape, not the table's older one; oldest first, as the page keeps it
        assert_eq!(exprs, ["12 kg to lb", "r = 5 m", "f(x) = x^2 + 1", "2×π"]);
        assert_eq!(history[2]["fnBody"], "x^2 + 1");
        let m: serde_json::Value =
            serde_json::from_str(moves.messaging.as_deref().unwrap()).unwrap();
        assert_eq!(
            (m["name"].as_str(), m["key"].as_str().map(str::len)),
            (Some("ada_l"), Some(64))
        );
        // what the native app makes of it, through the core's own rules
        let native = qcalc_core::app::messaging::sanitize_messaging(&m);
        assert_eq!(native.name, "ada_l");
        assert_eq!(native.accepted, ["grace_h"]);
        assert_eq!(native.blocked, ["spam_bot"]);
        // games.json's shape (games/store.rs `GameStore::from_json`)
        let games: serde_json::Value =
            serde_json::from_str(moves.games.as_deref().unwrap()).unwrap();
        assert_eq!(games["nickname"], "teal otter");
        assert_eq!(games["gameSaves"]["qcalc-crack"], "7");
        assert!(games["gameSaves"]["qcalc-equal"].is_string());
        // deleted in the second run: gone, though the table still holds it
        assert_eq!(local_storage(&db, "qcalc-filler"), None);
        // the page's draft and settings mirror aren't moved (settings.json is shared and counts)
        assert!(local_storage(&db, "qcalc-draft").is_some());
        assert_eq!(tauri_history(&fixture("leveldb")), moves.history);
    }

    #[test]
    fn the_tauri_settings_file_reads_as_saved() {
        let text = std::fs::read_to_string(fixture("settings.json")).unwrap();
        let s = crate::store::Store::from_json(&text);
        assert_eq!(s.settings.angle_mode, "rad");
        assert!(s.settings.fraction_mode && s.settings.space_thousands && !s.settings.share_usage);
        assert_eq!((s.settings.sig_figs, s.settings.draft_seconds), (9, 120));
        assert_eq!(s.settings.theme, "dark");
        assert_eq!(s.settings.history_show, "always");
        assert_eq!(
            s.settings.default_units.get("length").map(String::as_str),
            Some("ft")
        );
        assert_eq!(
            s.settings.keybinds.get("angle").map(String::as_str),
            Some("ctrl+alt+r")
        );
        assert_eq!(s.hotkey, "ctrl+alt+shift+f10");
        assert_eq!(s.positions.get(r"\\.\DISPLAY1"), Some(&[412.0, 180.0]));
        assert_eq!((s.spot_seconds, s.width, s.auto_update), (0, 900.0, false));
        assert!(s.first_run_done && s.onboarding.done);
        assert_eq!(s.onboarding.commits, 117);
        // kept for the Tauri app, written back as it was
        assert_eq!(s.sign_in, Some(true));
        assert!(
            serde_json::to_string(&s)
                .unwrap()
                .contains("\"signIn\":true")
        );
    }

    #[test]
    fn nothing_to_move_is_no_moves() {
        assert_eq!(plan(&Newest::new()), Moves::default());
        let mut db = Newest::new();
        // a messaging entry with neither a name nor lists isn't worth a file
        note(
            &mut db,
            &key("qcalc-messaging"),
            1,
            Some(b"\x01{\"name\":\"\",\"accepted\":[]}"),
        );
        note(&mut db, &key("qcalc-history"), 2, Some(b"\x01{}"));
        assert_eq!(plan(&db), Moves::default());
    }

    #[test]
    fn storage_text_reads_both_encodings() {
        assert_eq!(storage_text(b"\x01[]").as_deref(), Some("[]"));
        assert_eq!(storage_text(&utf16("√2")).as_deref(), Some("√2"));
        assert_eq!(storage_text(b"\x02x"), None);
    }
}
