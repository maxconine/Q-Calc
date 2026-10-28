//! Crash reports, kept on this computer and never sent by themselves.
//!
//! **Writing one.** A panic on the UI thread (the app can't go on after it: it unwinds out of a window procedure
//! and aborts) and an unhandled exception (access violation, stack overflow...) each write
//! `crash-<unix seconds>.txt` into `crashes\` beside the log (`%LOCALAPPDATA%\com.maxconine.qcalc\crashes`, or
//! `QCALC_DATA\crashes` for a test copy): the version, Windows' version, what happened, the stack (Rust's backtrace,
//! and the raw frames as `module+offset`, which the release's PDB resolves), and the log's last lines. An exception
//! also leaves a small minidump (`crash-<s>.dmp`, dbghelp's MiniDumpNormal with thread info) beside it. A panic on
//! another thread is only logged: the engine's are caught and it carries on.
//!
//! **The next launch** (after the first frame, bg.rs `start`) finds the newest report not yet offered and shows a
//! card (a task dialog, on a thread of its own): "Q Calc closed unexpectedly", **Report it** or Not now. Report it
//! opens a GitHub issue pre-filled with the version, Windows' version and a one-line summary (where it panicked,
//! or the exception's code and offset: never the panic's message, a path or anything typed), and opens the folder
//! with the report selected, for the person to attach. Either way it's marked offered (`.seen`) and never asked
//! again. The newest five reports are kept.
//!
//! **Report a Problem…** in the tray menu opens the same issue form (summary "reported from the tray") and a
//! folder, `report\` beside the log, holding a copy of the log and the newest crash report, if any.
//!
//! Nothing here runs on the launch path but `install` (a hook and a filter, no I/O). For tests, `QCALC_NO_OPEN`
//! logs what would open instead of opening it, and `QCALC_TEST_CRASH=panic|access` (honoured only with
//! `QCALC_DATA` set) crashes the copy on purpose once it's up.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::qlog;

/// The repository's new-issue form.
pub const ISSUES: &str = "https://github.com/maxconine/Q-Calc/issues/new";
/// Reports kept (oldest go first).
const KEEP: usize = 5;
/// The log lines a report carries.
const LOG_LINES: usize = 60;

static WROTE: AtomicBool = AtomicBool::new(false);

// ---- pure parts ----

/// What went wrong, in words that carry no personal data (a source file's name and line, a code and an offset).
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    Panic { file: String, line: u32 },
    Exception { code: u32, at: String },
}

impl What {
    pub fn summary(&self) -> String {
        match self {
            What::Panic { file, line } => format!("panic at {}:{line}", base_name(file)),
            What::Exception { code, at } => {
                let name = exception_name(*code);
                let at = if at.is_empty() {
                    String::new()
                } else {
                    format!(" at {at}")
                };
                match name {
                    Some(n) => format!("exception 0x{code:08X} ({n}){at}"),
                    None => format!("exception 0x{code:08X}{at}"),
                }
            }
        }
    }
}

/// A path's last part, either separator.
pub fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub fn exception_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0xC000_0005 => "access violation",
        0xC000_00FD => "stack overflow",
        0xC000_001D => "illegal instruction",
        0xC000_0094 => "integer divide by zero",
        0xC000_0374 => "heap corruption",
        0xC000_0409 => "fast fail",
        0xC000_0006 => "in-page error",
        0x8000_0003 => "breakpoint",
        _ => return None,
    })
}

/// "Windows 11 25H2 (10.0.26200.6899)": the product from the build number (11 from build 22000).
pub fn os_name(major: u32, minor: u32, build: u32, ubr: u32, display: &str) -> String {
    let product = if major >= 10 && build >= 22000 {
        "Windows 11"
    } else if major >= 10 {
        "Windows 10"
    } else {
        "Windows"
    };
    let display = display.trim();
    let release = if display.is_empty() {
        String::new()
    } else {
        format!(" {display}")
    };
    let ubr = if ubr > 0 {
        format!(".{ubr}")
    } else {
        String::new()
    };
    format!("{product}{release} ({major}.{minor}.{build}{ubr})")
}

/// Percent-encoding for a URL's query value (RFC 3986 unreserved kept).
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3 / 2);
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The pre-filled issue: title and body carry the version, Windows' version and the summary, nothing else.
/// `attach` names the file the person is asked to drag in.
pub fn issue_url(version: &str, os: &str, summary: &str, attach: Option<&str>) -> String {
    let title = format!("Windows native {version}: {summary}");
    let mut body = format!(
        "**Version:** {version} (native, {})\n**Windows:** {os}\n**What happened:** {summary}\n\n\
         **What were you doing just before?**\n\n\n",
        arch()
    );
    match attach {
        Some(file) => body.push_str(&format!(
            "Please attach `{file}` from the folder Q Calc opened (drag it into this box). It has the version, \
             the stack and the log's last lines; read it first if you like. Nothing was sent automatically.\n"
        )),
        None => body.push_str(
            "Please attach `qcalc.log` from the folder Q Calc opened (drag it into this box); read it first if \
             you like. Nothing was sent automatically.\n",
        ),
    }
    format!("{ISSUES}?title={}&body={}", encode(&title), encode(&body))
}

pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "x86"
    }
}

/// The report's text. `header` lines first (`key: value`, read back by `header`), then the rest.
pub struct Report<'a> {
    pub version: &'a str,
    pub os: &'a str,
    pub when: u64,
    pub what: &'a What,
    pub thread: &'a str,
    pub message: &'a str,
    pub stack: &'a str,
    pub minidump: Option<&'a str>,
    pub log_tail: &'a str,
}

impl Report<'_> {
    pub fn text(&self) -> String {
        let mut out = String::new();
        out.push_str("Q Calc crash report (kept on this computer; nothing was sent)\n");
        out.push_str(&format!("version: {} (native, {})\n", self.version, arch()));
        out.push_str(&format!("os: {}\n", self.os));
        out.push_str(&format!("when: {} (unix seconds)\n", self.when));
        out.push_str(&format!("summary: {}\n", self.what.summary()));
        out.push_str(&format!("thread: {}\n", self.thread));
        if let Some(d) = self.minidump {
            out.push_str(&format!("minidump: {d}\n"));
        }
        if !self.message.is_empty() {
            out.push_str("\nmessage:\n");
            out.push_str(self.message);
            out.push('\n');
        }
        out.push_str("\nstack:\n");
        out.push_str(self.stack);
        if !self.stack.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("\nlog (last {LOG_LINES} lines):\n"));
        out.push_str(self.log_tail);
        out
    }
}

/// A report's `key: value` header line.
pub fn header<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .take_while(|l| !l.is_empty())
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(": "))
        .map(str::trim)
}

/// The version without its "(native, x64)".
pub fn header_version(text: &str) -> Option<&str> {
    header(text, "version").map(|v| v.split(" (").next().unwrap_or(v))
}

/// The newest `crash-<n>.txt` among `names` without its `crash-<n>.seen`.
pub fn pending(names: &[String]) -> Option<String> {
    names
        .iter()
        .filter_map(|n| {
            let stem = n.strip_prefix("crash-")?.strip_suffix(".txt")?;
            let at: u64 = stem.parse().ok()?;
            (!names.iter().any(|m| *m == format!("crash-{stem}.seen"))).then_some((at, n))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, n)| n.clone())
}

/// The files of reports past the newest `keep` (their .txt, .dmp and .seen).
pub fn stale(names: &[String], keep: usize) -> Vec<String> {
    let mut stamps: Vec<u64> = names
        .iter()
        .filter_map(|n| n.strip_prefix("crash-")?.strip_suffix(".txt")?.parse().ok())
        .collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    let old: Vec<u64> = stamps.into_iter().skip(keep).collect();
    names
        .iter()
        .filter(|n| {
            n.strip_prefix("crash-")
                .and_then(|r| r.split('.').next())
                .and_then(|s| s.parse::<u64>().ok())
                .is_some_and(|at| old.contains(&at))
        })
        .cloned()
        .collect()
}

/// The last `n` lines of a log's text.
pub fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    let mut out = lines[start..].join("\n");
    out.push('\n');
    out
}

/// Where reports go: `QCALC_DATA\crashes` for a test copy, else `crashes` beside the log.
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("QCALC_DATA") {
        return Some(PathBuf::from(d).join("crashes"));
    }
    crate::log::dir().map(|d| d.join("crashes"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn log_tail() -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Some(path) = crate::log::path() else {
        return String::new();
    };
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(24 * 1024)));
    let mut bytes = Vec::new();
    let _ = f.read_to_end(&mut bytes);
    tail(&String::from_utf8_lossy(&bytes), LOG_LINES)
}

fn names_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Writes a report; the path, if it could.
fn write_report(
    what: &What,
    thread: &str,
    message: &str,
    stack: &str,
    dump: Option<&Path>,
) -> Option<PathBuf> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let when = unix_now();
    let path = dir.join(format!("crash-{when}.txt"));
    let os = os_version();
    let dump_name = dump
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let text = Report {
        version: crate::update::VERSION,
        os: &os,
        when,
        what,
        thread,
        message,
        stack,
        minidump: dump_name.as_deref(),
        log_tail: &log_tail(),
    }
    .text();
    std::fs::write(&path, text).ok()?;
    Some(path)
}

// ---- Windows ----

pub use win::*;

mod win {
    use super::*;
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HANDLE, HMODULE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW, GetProcAddress, LOAD_LIBRARY_FLAGS,
        LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
    };
    use windows::Win32::UI::Controls::{
        TASKDIALOG_BUTTON, TASKDIALOG_FLAGS, TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG,
        TDF_ALLOW_DIALOG_CANCELLATION, TDF_POSITION_RELATIVE_TO_WINDOW, TDN_CREATED,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HRESULT, HSTRING, PCWSTR, s, w};

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        record: *mut ExceptionRecord,
        address: *mut c_void,
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }

    /// dbghelp.h's MINIDUMP_EXCEPTION_INFORMATION (pshpack4)
    #[repr(C, packed(4))]
    struct DumpException {
        thread_id: u32,
        pointers: *mut ExceptionPointers,
        client_pointers: i32,
    }

    type Filter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        fn RtlCaptureStackBackTrace(
            skip: u32,
            count: u32,
            frames: *mut *mut c_void,
            hash: *mut u32,
        ) -> u16;
    }

    type MiniDumpWriteDump = unsafe extern "system" fn(
        HANDLE,
        u32,
        HANDLE,
        u32,
        *const DumpException,
        *const c_void,
        *const c_void,
    ) -> i32;

    type TaskDialogIndirect =
        unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut i32) -> HRESULT;

    /// MiniDumpNormal (0) | MiniDumpWithUnloadedModules | MiniDumpWithThreadInfo
    const DUMP_TYPE: u32 = 0x0020 | 0x1000;
    const EXCEPTION_EXECUTE_HANDLER: i32 = 1;

    /// Before anything else: the panic hook (after log.rs's, which it runs too) and the exception filter.
    pub fn install() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            // the UI thread can't go on after a panic (it unwinds out of a window procedure); elsewhere it's caught
            // (the engine) or ends only that thread, and the log has it
            if thread.name() == Some("main") && !WROTE.swap(true, Ordering::SeqCst) {
                let (file, line) = info
                    .location()
                    .map_or((String::new(), 0), |l| (l.file().to_string(), l.line()));
                let message = info
                    .payload()
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| info.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                let stack = format!(
                    "{}\nframes (module+offset):\n{}",
                    std::backtrace::Backtrace::force_capture(),
                    frames_text(&capture(1))
                );
                let what = What::Panic { file, line };
                match write_report(&what, "main", &message, &stack, None) {
                    Some(p) => qlog!("crash: {} written to {}", what.summary(), p.display()),
                    None => qlog!("crash: {} (couldn't write the report)", what.summary()),
                }
            }
            previous(info);
        }));
        // SAFETY: installs a function pointer; the previous filter (none, normally) is dropped
        unsafe {
            let _ = SetUnhandledExceptionFilter(Some(filter));
        }
    }

    fn capture(skip: u32) -> Vec<usize> {
        let mut frames = [std::ptr::null_mut::<c_void>(); 62];
        // SAFETY: the buffer holds the count asked for
        let n = unsafe {
            RtlCaptureStackBackTrace(
                skip + 1,
                frames.len() as u32,
                frames.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        frames[..usize::from(n)]
            .iter()
            .map(|p| *p as usize)
            .collect()
    }

    /// `qcalc.exe+0x1a2b3`: the module an address is in and the offset from its base.
    pub fn module_offset(addr: usize) -> String {
        let mut module = HMODULE::default();
        // SAFETY: FROM_ADDRESS reads the address as a pointer into a module; UNCHANGED_REFCOUNT takes no reference
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(addr as *const u16),
                &mut module,
            )
        }
        .is_ok();
        if !found {
            return format!("0x{addr:x}");
        }
        let mut buf = [0u16; 260];
        // SAFETY: a live buffer
        let n = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
        let name = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        format!("{}+0x{:x}", base_name(&name), addr - module.0 as usize)
    }

    fn frames_text(frames: &[usize]) -> String {
        frames
            .iter()
            .enumerate()
            .map(|(i, a)| format!("  {i:2}: {}\n", module_offset(*a)))
            .collect()
    }

    struct Fault {
        code: u32,
        address: usize,
        thread: u32,
        pointers: usize,
        frames: Vec<usize>,
    }

    unsafe extern "system" fn filter(pointers: *mut ExceptionPointers) -> i32 {
        if WROTE.swap(true, Ordering::SeqCst) {
            return EXCEPTION_EXECUTE_HANDLER;
        }
        // SAFETY: Windows passes live exception pointers to the filter
        let (code, address) = unsafe {
            let r = (*pointers).record;
            if r.is_null() {
                (0, 0)
            } else {
                ((*r).code, (*r).address as usize)
            }
        };
        let fault = Fault {
            code,
            address,
            // SAFETY: plain query
            thread: unsafe { GetCurrentThreadId() },
            pointers: pointers as usize,
            frames: capture(0),
        };
        // on a thread of its own: a stack overflow leaves this one next to no stack
        let writer = std::thread::Builder::new()
            .name("qcalc-crash".into())
            .stack_size(1 << 20)
            .spawn(move || write_fault(&fault));
        if let Ok(w) = writer {
            let _ = w.join();
        }
        // ends the process here, quietly: the report is ours, and nothing goes anywhere by itself
        EXCEPTION_EXECUTE_HANDLER
    }

    fn write_fault(f: &Fault) {
        let what = What::Exception {
            code: f.code,
            at: if f.address == 0 {
                String::new()
            } else {
                module_offset(f.address)
            },
        };
        let Some(dir) = dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let dump = dir.join(format!("crash-{}.dmp", unix_now()));
        let dumped = minidump(&dump, f.thread, f.pointers);
        let stack = format!(
            "frames from the filter (module+offset):\n{}",
            frames_text(&f.frames)
        );
        match write_report(
            &what,
            &format!("id {}", f.thread),
            "",
            &stack,
            dumped.then_some(dump.as_path()),
        ) {
            Some(p) => qlog!("crash: {} written to {}", what.summary(), p.display()),
            None => qlog!("crash: {} (couldn't write the report)", what.summary()),
        }
    }

    fn minidump(path: &Path, thread: u32, pointers: usize) -> bool {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: dbghelp from System32 only; the function's signature is dbghelp.h's
        let write: MiniDumpWriteDump = unsafe {
            let Ok(lib) = LoadLibraryExW(w!("dbghelp.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
            else {
                return false;
            };
            let Some(f) = GetProcAddress(lib, s!("MiniDumpWriteDump")) else {
                return false;
            };
            std::mem::transmute::<unsafe extern "system" fn() -> isize, MiniDumpWriteDump>(f)
        };
        let Ok(file) = std::fs::File::create(path) else {
            return false;
        };
        let info = DumpException {
            thread_id: thread,
            pointers: pointers as *mut ExceptionPointers,
            client_pointers: 0,
        };
        // SAFETY: the file handle and the exception info outlive the call; the faulting thread waits on this one
        let ok = unsafe {
            write(
                GetCurrentProcess(),
                GetCurrentProcessId(),
                HANDLE(file.as_raw_handle()),
                DUMP_TYPE,
                &info,
                std::ptr::null(),
                std::ptr::null(),
            )
        } != 0;
        drop(file);
        if !ok {
            let _ = std::fs::remove_file(path);
        }
        ok
    }

    /// "Windows 11 25H2 (10.0.26200.6899)", read from the registry's CurrentVersion (no API lies about it).
    pub fn os_version() -> String {
        use windows::Win32::System::Registry::{
            HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
        };
        let key = HSTRING::from(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
        let dword = |name: &str| -> u32 {
            let mut v = 0u32;
            let mut size = 4u32;
            // SAFETY: a live 4-byte out value
            let r = unsafe {
                RegGetValueW(
                    HKEY_LOCAL_MACHINE,
                    &key,
                    &HSTRING::from(name),
                    RRF_RT_REG_DWORD,
                    None,
                    Some((&mut v as *mut u32).cast()),
                    Some(&mut size),
                )
            };
            if r.is_ok() { v } else { 0 }
        };
        let text = |name: &str| -> String {
            let mut buf = [0u16; 64];
            let mut size = (buf.len() * 2) as u32;
            // SAFETY: a live buffer of the size passed
            let r = unsafe {
                RegGetValueW(
                    HKEY_LOCAL_MACHINE,
                    &key,
                    &HSTRING::from(name),
                    RRF_RT_REG_SZ,
                    None,
                    Some(buf.as_mut_ptr().cast()),
                    Some(&mut size),
                )
            };
            if r.is_err() {
                return String::new();
            }
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..n])
        };
        let build: u32 = text("CurrentBuildNumber").parse().unwrap_or(0);
        os_name(
            dword("CurrentMajorVersionNumber"),
            dword("CurrentMinorVersionNumber"),
            build,
            dword("UBR"),
            &text("DisplayVersion"),
        )
    }

    // ---- opening things (or, for a test, saying so) ----

    fn quiet() -> bool {
        std::env::var_os("QCALC_NO_OPEN").is_some()
    }

    pub fn open_url(url: &str) {
        if quiet() {
            qlog!("report: would open {url}");
            return;
        }
        // SAFETY: plain strings in
        let r = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(url),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if (r.0 as isize) <= 32 {
            qlog!("report: couldn't open the browser ({})", r.0 as isize);
        }
    }

    /// Explorer at `path`'s folder with it selected.
    pub fn show_file(path: &Path) {
        if quiet() {
            qlog!("report: would show {}", path.display());
            return;
        }
        let windir =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let arg = format!("/select,\"{}\"", path.display());
        use std::os::windows::process::CommandExt;
        if let Err(e) = std::process::Command::new(windir.join("explorer.exe"))
            .raw_arg(arg)
            .spawn()
        {
            qlog!("report: couldn't open the folder: {e}");
        }
    }

    // ---- the next launch ----

    /// After the first frame: a test crash, else the card for a report not yet offered, and old reports pruned.
    pub fn start() {
        if std::env::var_os("QCALC_DATA").is_some()
            && let Ok(kind) = std::env::var("QCALC_TEST_CRASH")
        {
            test_crash(&kind);
        }
        let Some(dir) = dir() else { return };
        if !dir.is_dir() {
            return;
        }
        crate::bg::spawn("qcalc-crash-card", move || {
            let names = names_in(&dir);
            for old in stale(&names, KEEP) {
                let _ = std::fs::remove_file(dir.join(old));
            }
            let names = names_in(&dir);
            let Some(name) = pending(&names) else { return };
            let path = dir.join(&name);
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // offered once, whatever the answer
            let _ = std::fs::write(path.with_extension("seen"), b"");
            let summary = header(&text, "summary").unwrap_or("a crash").to_string();
            let version = header_version(&text)
                .unwrap_or(crate::update::VERSION)
                .to_string();
            let os = header(&text, "os").map_or_else(os_version, String::from);
            qlog!("crash: offering to report {name} ({summary})");
            if card(&summary, &path) {
                qlog!("crash: report it");
                open_url(&issue_url(&version, &os, &summary, Some(&name)));
                show_file(&path);
            }
        });
    }

    fn test_crash(kind: &str) {
        qlog!("crash: test crash ({kind})");
        match kind {
            "panic" => panic!("a test crash (QCALC_TEST_CRASH)"),
            "access" => {
                // SAFETY: deliberately not: an access violation for the test
                unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u8>(), 1) }
            }
            _ => {}
        }
    }

    /// The card: true for Report it. A task dialog (comctl32 v6, loaded only now).
    fn card(summary: &str, path: &Path) -> bool {
        const REPORT: i32 = 100;
        const LATER: i32 = 101;
        if let Ok(answer) = std::env::var("QCALC_CRASH_CARD")
            && std::env::var_os("QCALC_DATA").is_some()
        {
            // a test answers for the person
            qlog!("crash: card shown ({summary}), answered {answer}");
            return answer == "report";
        }
        // SAFETY: comctl32 by name, so side-by-side redirection gives the manifest's v6 (only it has task dialogs;
        // a System32-only search would find v5)
        let show: TaskDialogIndirect = unsafe {
            let Ok(lib) = LoadLibraryExW(w!("comctl32.dll"), None, LOAD_LIBRARY_FLAGS(0)) else {
                qlog!("crash: comctl32 didn't load; no card");
                return false;
            };
            let Some(f) = GetProcAddress(lib, s!("TaskDialogIndirect")) else {
                qlog!("crash: no TaskDialogIndirect (comctl32 v5?); no card");
                return false;
            };
            std::mem::transmute::<unsafe extern "system" fn() -> isize, TaskDialogIndirect>(f)
        };
        let content = HSTRING::from(format!(
            "Q Calc stopped last time ({summary}). A report was saved on this computer; nothing has been sent.\n\n\
             Report it opens a GitHub issue with the version and that one line filled in, and the folder with the \
             report, for you to attach."
        ));
        let expanded = HSTRING::from(path.display().to_string());
        let (report, later) = (HSTRING::from("Report it"), HSTRING::from("Not now"));
        let buttons = [
            TASKDIALOG_BUTTON {
                nButtonID: REPORT,
                pszButtonText: PCWSTR(report.as_ptr()),
            },
            TASKDIALOG_BUTTON {
                nButtonID: LATER,
                pszButtonText: PCWSTR(later.as_ptr()),
            },
        ];
        let mut config = TASKDIALOGCONFIG {
            cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
            hwndParent: HWND::default(),
            dwFlags: TASKDIALOG_FLAGS(
                TDF_ALLOW_DIALOG_CANCELLATION.0 | TDF_POSITION_RELATIVE_TO_WINDOW.0,
            ),
            pszWindowTitle: w!("Q Calc"),
            pszMainInstruction: w!("Q Calc closed unexpectedly"),
            pszContent: PCWSTR(content.as_ptr()),
            cButtons: buttons.len() as u32,
            pButtons: buttons.as_ptr(),
            nDefaultButton: REPORT,
            pszExpandedInformation: PCWSTR(expanded.as_ptr()),
            pszCollapsedControlText: w!("Where the report is"),
            ..Default::default()
        };
        config.pfCallback = Some(on_top);
        // TD_WARNING_ICON: MAKEINTRESOURCE(-1)
        config.Anonymous1.pszMainIcon = PCWSTR(0xFFFF as *const u16);
        let mut picked = 0i32;
        // SAFETY: the config and everything it points at live across the (modal) call
        let r = unsafe {
            show(
                &config,
                &mut picked,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if let Err(e) = r.ok() {
            qlog!("crash: the card couldn't show: {e}");
        }
        r.is_ok() && picked == REPORT
    }

    /// The card has no owner (the bar may be away): on top, so it isn't lost behind what's open.
    unsafe extern "system" fn on_top(
        hwnd: HWND,
        msg: TASKDIALOG_NOTIFICATIONS,
        _: WPARAM,
        _: LPARAM,
        _: isize,
    ) -> HRESULT {
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos,
        };
        if msg == TDN_CREATED {
            // SAFETY: the dialog's own window, on its thread
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
                let _ = SetForegroundWindow(hwnd);
            }
        }
        HRESULT(0)
    }

    // ---- the tray's Report a Problem… ----

    /// A folder (`report\` beside the log) with a copy of the log and the newest crash report, shown in Explorer,
    /// and the issue form.
    pub fn report_problem() {
        crate::bg::spawn("qcalc-report", || {
            let base = match std::env::var_os("QCALC_DATA") {
                Some(d) => Some(PathBuf::from(d)),
                None => crate::log::dir(),
            };
            let Some(folder) = base.map(|b| b.join("report")) else {
                return;
            };
            let _ = std::fs::remove_dir_all(&folder);
            if let Err(e) = std::fs::create_dir_all(&folder) {
                qlog!("report: couldn't make {}: {e}", folder.display());
                return;
            }
            let log = folder.join("qcalc.log");
            if let Some(src) = crate::log::path() {
                let _ = std::fs::copy(src, &log);
            }
            let mut attach = "qcalc.log".to_string();
            if let Some(dir) = dir() {
                let names = names_in(&dir);
                let newest = names
                    .iter()
                    .filter(|n| n.starts_with("crash-") && n.ends_with(".txt"))
                    .max_by_key(|n| {
                        n.trim_start_matches("crash-")
                            .trim_end_matches(".txt")
                            .parse::<u64>()
                            .unwrap_or(0)
                    });
                if let Some(n) = newest
                    && std::fs::copy(dir.join(n), folder.join(n)).is_ok()
                {
                    attach = format!("qcalc.log` and `{n}");
                }
            }
            let url = issue_url(
                crate::update::VERSION,
                &os_version(),
                "reported from the tray",
                Some(&attach),
            );
            qlog!("report: a problem, from the tray");
            open_url(&url);
            show_file(&log);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_carry_no_paths_or_messages() {
        let p = What::Panic {
            file: r"C:\Users\someone\src\qcalc\native\windows\src\app.rs".into(),
            line: 412,
        };
        assert_eq!(p.summary(), "panic at app.rs:412");
        assert_eq!(
            What::Panic {
                file: "/home/x/.cargo/registry/src/foo-1.0/src/lib.rs".into(),
                line: 3
            }
            .summary(),
            "panic at lib.rs:3"
        );
        let e = What::Exception {
            code: 0xC000_0005,
            at: "qcalc.exe+0x1a2b".into(),
        };
        assert_eq!(
            e.summary(),
            "exception 0xC0000005 (access violation) at qcalc.exe+0x1a2b"
        );
        assert_eq!(
            What::Exception {
                code: 0xE06D_7363,
                at: String::new()
            }
            .summary(),
            "exception 0xE06D7363"
        );
    }

    #[test]
    fn the_os_is_named_by_its_build() {
        assert_eq!(
            os_name(10, 0, 26200, 6899, "25H2"),
            "Windows 11 25H2 (10.0.26200.6899)"
        );
        assert_eq!(os_name(10, 0, 19045, 0, ""), "Windows 10 (10.0.19045)");
    }

    #[test]
    fn the_issue_url_is_short_and_says_only_what_it_should() {
        let url = issue_url(
            "2.1.0-beta.1",
            "Windows 11 25H2 (10.0.26200.6899)",
            "panic at app.rs:412",
            Some("crash-1793000000.txt"),
        );
        assert!(url.starts_with("https://github.com/maxconine/Q-Calc/issues/new?title=Windows%20native%202.1.0-beta.1%3A%20panic%20at%20app.rs%3A412&body="));
        // well inside the 8 KB browsers and GitHub take
        assert!(url.len() < 1500, "{}", url.len());
        assert!(url.contains("crash-1793000000.txt"));
        // nothing but ASCII that needs no further escaping
        assert!(url.bytes().all(|b| b.is_ascii_graphic()));
        let decoded = url.replace("%20", " ");
        for leak in ["Users", "C%3A", "%5C", "\\", "AppData"] {
            assert!(!decoded.contains(leak), "{leak}");
        }
        let tray = issue_url(
            "2.0.15",
            "Windows 10 (10.0.19045)",
            "reported from the tray",
            None,
        );
        assert!(tray.contains("qcalc.log"));
    }

    #[test]
    fn encode_keeps_unreserved_only() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode("A-z_0.9~"), "A-z_0.9~");
        assert_eq!(encode("**x**\n"), "%2A%2Ax%2A%2A%0A");
    }

    #[test]
    fn a_report_reads_back_its_header() {
        let what = What::Exception {
            code: 0xC000_00FD,
            at: "qcalc.exe+0x10".into(),
        };
        let text = Report {
            version: "2.0.15",
            os: "Windows 11 (10.0.26200)",
            when: 1_793_000_000,
            what: &what,
            thread: "id 4",
            message: "",
            stack: "  0: qcalc.exe+0x10",
            minidump: Some("crash-1793000000.dmp"),
            log_tail: "1 launch\n",
        }
        .text();
        assert_eq!(
            header(&text, "summary"),
            Some("exception 0xC00000FD (stack overflow) at qcalc.exe+0x10")
        );
        assert_eq!(header_version(&text), Some("2.0.15"));
        assert_eq!(header(&text, "os"), Some("Windows 11 (10.0.26200)"));
        assert_eq!(header(&text, "minidump"), Some("crash-1793000000.dmp"));
        // the header ends at the first blank line: the log can't be read as one
        let fake = format!("{text}\nsummary: not this\n");
        assert_eq!(header(&fake, "summary"), header(&text, "summary"));
        assert!(text.contains("stack:\n  0: qcalc.exe+0x10\n"));
        assert!(text.ends_with("1 launch\n"));
    }

    #[test]
    fn the_newest_unoffered_report_is_pending_and_old_ones_go() {
        let names: Vec<String> = [
            "crash-100.txt",
            "crash-100.seen",
            "crash-300.txt",
            "crash-300.dmp",
            "crash-200.txt",
            "crash-x.txt",
            "notes.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(pending(&names).as_deref(), Some("crash-300.txt"));
        let mut seen = names.clone();
        seen.push("crash-300.seen".into());
        assert_eq!(pending(&seen).as_deref(), Some("crash-200.txt"));
        seen.push("crash-200.seen".into());
        assert_eq!(pending(&seen), None);
        let mut gone = stale(&names, 1);
        gone.sort();
        assert_eq!(gone, ["crash-100.seen", "crash-100.txt", "crash-200.txt"]);
        assert!(stale(&names, 5).is_empty());
    }

    #[test]
    fn the_log_tail_is_its_last_lines() {
        assert_eq!(tail("a\nb\nc\n", 2), "b\nc\n");
        assert_eq!(tail("a", 5), "a\n");
        assert_eq!(base_name(r"a\b/c.rs"), "c.rs");
    }
}
