//! Windows (MSVC) only: embeds the manifest (per-monitor-v2 DPI, common controls 6, UTF-8 code page) and, when the
//! SDK's `rc.exe` is around, the app icon, so Explorer and the Run dialog show Q Calc's glyph. Elsewhere it does
//! nothing, so the Mac's `cargo build` of the workspace is untouched.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=qcalc.manifest");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let manifest = dir.join("qcalc.manifest");
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
    // the core recurses deeply (ARCHITECTURE.md §5). The engine runs on its own 32 MB threads, but a stray call on
    // the main thread must not overflow either: 8 MB reserved (committed only as touched)
    println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    icon(&dir);
}

/// The icon resource, compiled with the SDK's rc.exe; skipped with a warning when there is none.
fn icon(dir: &Path) {
    let ico = dir.join("../../src-tauri/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found: qcalc.exe builds without its icon resource");
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let script = out.join("qcalc.rc");
    let ico_path = ico
        .canonicalize()
        .unwrap_or(ico)
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let ico_path = ico_path.trim_start_matches("\\\\\\\\?\\\\").to_string();
    // id 1 is the app icon (Explorer takes the first); window.rs/tray.rs load the same bytes from the binary
    if std::fs::write(&script, format!("1 ICON \"{ico_path}\"\n")).is_err() {
        return;
    }
    let res = out.join("qcalc.res");
    let ok = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res)
        .arg(&script)
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    } else {
        println!("cargo:warning=rc.exe failed: qcalc.exe builds without its icon resource");
    }
}

fn find_rc() -> Option<PathBuf> {
    let root = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("10."))
        })
        .collect();
    versions.sort();
    versions
        .into_iter()
        .rev()
        .map(|v| v.join("x64").join("rc.exe"))
        .chain([root.join("x64").join("rc.exe")])
        .find(|p| p.exists())
}
