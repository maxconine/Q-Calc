//! The bench pipe's `share {…}` commands, for native/windows/tests/e2e/share-e2e.ps1 only (bench.rs `extra`; the
//! pipe exists only with `--bench-pipe` / `QCALC_BENCH=1`). They do what the bar's lines, cards and thread do, by
//! name, without the clipboard or a file dialog in the way, and report the state the suite checks: the link, the
//! cards, the transfers, the threads, the keys and pins.
//!
//! `vector-open` / `vector-seal` run the crypto on vectors made with the Mac's construction (tests/e2e/
//! share-vectors.mjs) in both directions, through the same code the app uses.

use serde_json::{Value, json};

use crate::app::App;
use crate::share_crypt as crypt;
use crate::{bg, messaging, share, share_keys, threads};

static DRAG: std::sync::Mutex<Value> = std::sync::Mutex::new(Value::Null);

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_string()
}

/// `args` is the JSON after `share `. Mutations run on the UI thread right after the reply (`bg::post`).
pub fn cmd(app: &App, args: &str) -> Value {
    let Ok(a) = serde_json::from_str::<Value>(args) else {
        return json!({ "error": "share: not JSON" });
    };
    let op = s(&a, "op");
    let later = |f: Box<dyn FnOnce(&mut App) + Send>| {
        bg::post(f);
        json!({ "ok": true })
    };
    match op.as_str() {
        "state" => json!({
            "messaging": messaging::debug_state_with(&app.model),
            "share": share::debug_state(),
            "threads": threads::debug_state(),
            "keys": share_keys::debug_state(),
            "hint": app.model.hint,
            "drag": DRAG.lock().map(|d| d.clone()).unwrap_or(Value::Null),
            "completion": app.model.line.completion,
            "slot": app.model.command.as_ref().map(|c| c.message.clone()),
            "text": app.model.field.text(),
            "visible": app.bar.visible,
        }),
        "name" => {
            let name = s(&a, "name");
            later(Box::new(move |_| messaging::set_name(&name)))
        }
        "allow" => {
            let who = s(&a, "who");
            // settings' Requests › Allow (the same as the card's)
            later(Box::new(move |app| messaging::answer(app, &who, true)))
        }
        "inbox" => {
            let pause = a["pause"].as_bool().unwrap_or(false);
            later(Box::new(move |_| messaging::inbox_pause(pause)))
        }
        "forget" => {
            let who = s(&a, "who");
            later(Box::new(move |_| messaging::forget(&who)))
        }
        "text" => {
            let (to, text) = (s(&a, "to"), s(&a, "text"));
            later(Box::new(move |app| messaging::send_line(app, to, text)))
        }
        "clip" => {
            let (to, text) = (s(&a, "to"), s(&a, "text"));
            later(Box::new(move |app| share::send_clip(app, text, &to)))
        }
        "paste" => {
            let to = s(&a, "to");
            later(Box::new(move |app| share::send_clipboard(app, &to)))
        }
        "files" => {
            let to = s(&a, "to");
            let paths: Vec<std::path::PathBuf> = a["paths"]
                .as_array()
                .map(|p| p.iter().filter_map(Value::as_str).map(Into::into).collect())
                .unwrap_or_default();
            later(Box::new(move |app| share::send_files(app, paths, &to)))
        }
        "drop" => {
            let paths: Vec<std::path::PathBuf> = a["paths"]
                .as_array()
                .map(|p| p.iter().filter_map(Value::as_str).map(Into::into).collect())
                .unwrap_or_default();
            later(Box::new(move |app| {
                share::drop_files(app, paths);
            }))
        }
        "save" | "retry" => {
            let id = s(&a, "id");
            let retry = op == "retry";
            later(Box::new(move |app| share::file_act(app, &id, retry)))
        }
        "trust" => {
            let who = s(&a, "who");
            later(Box::new(move |app| share::trust(app, &who)))
        }
        "open" => {
            let who = s(&a, "who");
            later(Box::new(move |app| threads::open(app, &who)))
        }
        "close" => later(Box::new(|app| {
            threads::close(&mut app.model);
        })),
        "line" => {
            let text = s(&a, "text");
            later(Box::new(move |app| {
                app.model.field.set_text(&text);
                app.text_changed();
            }))
        }
        // a drag over the thread (`at: "thread"`) or the line, its hint kept for `state`'s `drag`
        "drag" => {
            let paths: Vec<std::path::PathBuf> = a["paths"]
                .as_array()
                .map(|p| p.iter().filter_map(Value::as_str).map(Into::into).collect())
                .unwrap_or_default();
            let (thread, drop) = (
                s(&a, "at") == "thread",
                a["drop"].as_bool().unwrap_or(false),
            );
            later(Box::new(move |app| {
                let l = &app.bar.layout;
                let r = if thread {
                    l.threads.unwrap_or(l.composer)
                } else {
                    l.composer
                };
                let v = crate::share_drop::simulate(
                    app,
                    paths,
                    r.x + r.w / 2.0,
                    r.y + r.h - 20.0,
                    drop,
                );
                if let Ok(mut d) = DRAG.lock() {
                    *d = v;
                }
            }))
        }
        "vector-open" => vector_open(&a),
        "vector-seal" => vector_seal(&a),
        _ => json!({ "error": format!("share: unknown op {op}") }),
    }
}

fn secret_of(hex: &str) -> Option<x25519_dalek::StaticSecret> {
    let raw: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    let raw: [u8; 32] = raw.try_into().ok()?;
    Some(x25519_dalek::StaticSecret::from(raw))
}

/// A clip and a file made with the Mac's construction, opened here: the clip's text, the facts read back, and the
/// file saved (checked and marked) into `dir`.
fn vector_open(a: &Value) -> Value {
    let Some(me) = secret_of(&s(a, "secret")) else {
        return json!({ "error": "secret" });
    };
    let Some(from) = crypt::public_key(&s(a, "senderPub")) else {
        return json!({ "error": "senderPub" });
    };
    let my_pub = crypt::public_text(&me);
    let ctx = |kind: &str, id: &str| {
        qcalc_core::app::messaging::share_context(
            kind,
            &s(a, "from"),
            &s(a, "to"),
            id,
            &s(a, "senderPub"),
            &my_pub,
        )
        .unwrap_or_default()
    };
    let clip = &a["clip"];
    let clip_text = crypt::open(
        &s(clip, "sealed"),
        &s(clip, "epk"),
        &me,
        &from,
        &ctx("clip", &s(clip, "id")),
    )
    .map(|p| String::from_utf8_lossy(&p).into_owned());
    let offer = &a["offer"];
    let facts = crypt::open(
        &s(offer, "sealed"),
        &s(offer, "epk"),
        &me,
        &from,
        &ctx("file", &s(offer, "id")),
    )
    .ok()
    .and_then(|p| {
        qcalc_core::app::messaging::open_file_facts(
            &String::from_utf8_lossy(&p),
            offer["bytes"].as_u64().unwrap_or(0),
        )
    });
    let saved = facts.as_ref().and_then(|f| {
        let key = crypt::file_key(&f.key)?;
        crypt::safe_save(
            std::path::Path::new(&s(a, "sealedFile")),
            &key,
            &f.sha256,
            f.size,
            &f.name,
            std::path::Path::new(&s(a, "dir")),
        )
        .ok()
    });
    json!({
        "clip": clip_text.ok(),
        "facts": facts.as_ref().map(|f| json!({ "name": f.name, "size": f.size, "sha256": f.sha256 })),
        "saved": saved.as_ref().map(|p| p.display().to_string()),
        "marked": saved.as_ref().is_some_and(|p| crypt::has_mark_of_the_web(p)),
    })
}

/// A clip and a file sealed here for the vectors' recipient (`recipPub`), by a fresh key whose public half comes
/// back, for share-vectors.mjs to open with the Mac's construction.
fn vector_seal(a: &Value) -> Value {
    let me = crypt::new_secret();
    let my_pub = crypt::public_text(&me);
    let Some(to) = crypt::public_key(&s(a, "recipPub")) else {
        return json!({ "error": "recipPub" });
    };
    let (from_name, to_name) = (s(a, "from"), s(a, "to"));
    let ctx = |kind: &str, id: &str| {
        qcalc_core::app::messaging::share_context(
            kind,
            &from_name,
            &to_name,
            id,
            &my_pub,
            &s(a, "recipPub"),
        )
    };
    let clip_id = crypt::new_id();
    let Some(c) = ctx("clip", &clip_id) else {
        return json!({ "error": "context" });
    };
    let Ok((epk, sealed)) = crypt::seal(s(a, "clip").as_bytes(), &me, &to, &c) else {
        return json!({ "error": "seal" });
    };
    let plain = std::path::PathBuf::from(s(a, "plainFile"));
    let out = std::path::PathBuf::from(s(a, "sealedFile"));
    let key = crypt::new_file_key();
    let Ok(sha) = crypt::encrypt_file(&plain, &out, &key, |_| {}) else {
        return json!({ "error": "encrypt" });
    };
    let size = std::fs::metadata(&plain).map_or(0, |m| m.len());
    let facts = qcalc_core::app::messaging::seal_file_facts(
        &plain
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        "application/octet-stream",
        size,
        &sha,
        &crypt::encode(&key),
    )
    .unwrap_or_default();
    let file_id = crypt::new_id();
    let Some(fc) = ctx("file", &file_id) else {
        return json!({ "error": "context" });
    };
    let Ok((fepk, fsealed)) = crypt::seal(facts.as_bytes(), &me, &to, &fc) else {
        return json!({ "error": "seal" });
    };
    json!({
        "senderPub": my_pub,
        "clip": { "id": clip_id, "epk": epk, "sealed": sealed },
        "offer": { "id": file_id, "epk": fepk, "sealed": fsealed, "bytes": crypt::encrypted_length(size) },
    })
}
