//! End-to-end sealing for clips and files, byte for byte as the Mac's ShareCrypt.swift and FileCrypt.swift (native
//! only; native/PRIVACY.md), so a Mac and a Windows user can send each other clips and files. Audited crates:
//! x25519-dalek, hkdf + sha2, aes-gcm. Pure apart from the files; nothing here runs on the UI thread.
//!
//! - **Seal** (`seal` / `open`): the sender draws an ephemeral X25519 key; the one-time key is HKDF-SHA256 over two
//!   agreements, the ephemeral key's with the recipient's then the sender's own with the recipient's, salt
//!   `qcalc-share-v1`, info the core's `share_context` (which is also the GCM associated data). The box is
//!   nonce (12) ‖ ciphertext ‖ tag (16), base64url without padding. Only the recipient opens it, only the sender
//!   could have made it, and a box moved to another id, name or kind doesn't open.
//! - **Files** (`encrypt_file` / `decrypt_file`): a fresh AES-256 key per file (sealed in the offer's facts), the
//!   plain bytes in 1 MiB chunks each sealed on its own: nonce 4 zero bytes ‖ the index (8, big-endian), associated
//!   data the index (8, big-endian) ‖ 1 on the last chunk else 0, so chunks can't be reordered, dropped or cut.
//! - **Saving** (`safe_save`): the download decrypted beside Downloads into a hidden partial file, checked (length,
//!   size, SHA-256), marked with the Mark of the Web (`Zone.Identifier`, internet zone) so Windows asks before
//!   anything of it runs, then moved under a name nothing has (`notes 2.pdf`). Anything that fails leaves nothing
//!   behind. Nothing here opens, previews or runs a file.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use qcalc_core::app::messaging as core;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

pub const VERSION: &str = core::SHARE_VERSION;
pub const CHUNK: usize = core::CHUNK_BYTES as usize;
pub const TAG: usize = core::TAG_BYTES as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// a key that isn't one, or agrees on zero
    BadKey,
    /// a box that isn't theirs, for us, for this context
    Sealed,
    Unreadable,
    Corrupt,
    Disk,
    Size,
}

impl Failure {
    /// The core's word for it (`file_error_text`).
    pub fn status(self) -> &'static str {
        match self {
            Failure::BadKey | Failure::Sealed => "sealed",
            Failure::Unreadable => "unreadable",
            Failure::Corrupt | Failure::Size => "corrupt",
            Failure::Disk => "disk",
        }
    }
}

// ---- encoding ----

/// base64url without padding.
pub fn encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = c.iter().fold(0u32, |n, b| (n << 8) | u32::from(*b)) << (8 * (3 - c.len()));
        for k in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    out
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    core::b64url_decode(text)
}

/// Random bytes from the system.
pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    // ProcessPrng never fails once loaded; a key is never made from anything weaker (callers run off the UI
    // thread, where a panic is logged and ends only that job)
    getrandom::getrandom(&mut b).expect("the system's random numbers");
    b
}

/// A share's id: 16 random bytes as hex, drawn by the sender so the seal can be bound to it.
pub fn new_id() -> String {
    random::<16>().iter().map(|b| format!("{b:02x}")).collect()
}

/// A new private key.
pub fn new_secret() -> StaticSecret {
    StaticSecret::from(random::<32>())
}

/// A public key as published: 43 characters of base64url.
pub fn public_text(secret: &StaticSecret) -> String {
    encode(PublicKey::from(secret).as_bytes())
}

/// A published key read back: 43 characters, 32 bytes, not all zero.
pub fn public_key(text: &str) -> Option<PublicKey> {
    let clean = core::clean_public_key(&serde_json::Value::String(text.into()))?;
    let raw: [u8; 32] = decode(&clean)?.try_into().ok()?;
    Some(PublicKey::from(raw))
}

// ---- sealing ----

fn agree(mine: &StaticSecret, theirs: &PublicKey) -> Result<[u8; 32], Failure> {
    let s = mine.diffie_hellman(theirs).to_bytes();
    // a low-order point agrees on zero: never a key
    if s.iter().all(|b| *b == 0) {
        return Err(Failure::BadKey);
    }
    Ok(s)
}

fn wrap_key(ephemeral: &[u8; 32], fixed: &[u8; 32], context: &str) -> [u8; 32] {
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(ephemeral);
    ikm[32..].copy_from_slice(fixed);
    let mut key = [0u8; 32];
    // 32 bytes is far under HKDF-SHA256's limit
    let _ =
        Hkdf::<Sha256>::new(Some(VERSION.as_bytes()), &ikm).expand(context.as_bytes(), &mut key);
    key
}

/// `plain` sealed for `to` by `me`: (the ephemeral public key, the box), both base64url. `context` is the core's
/// `share_context` for this share.
pub fn seal(
    plain: &[u8],
    me: &StaticSecret,
    to: &PublicKey,
    context: &str,
) -> Result<(String, String), Failure> {
    seal_with(plain, me, to, context, &new_secret(), random::<12>())
}

/// `seal` with the ephemeral key and nonce given (the tests' fixed vectors).
pub fn seal_with(
    plain: &[u8],
    me: &StaticSecret,
    to: &PublicKey,
    context: &str,
    ephemeral: &StaticSecret,
    nonce: [u8; 12],
) -> Result<(String, String), Failure> {
    let key = wrap_key(&agree(ephemeral, to)?, &agree(me, to)?, context);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Failure::Sealed)?;
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| Failure::Sealed)?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ct);
    Ok((public_text(ephemeral), encode(&combined)))
}

/// A box sealed for `me` by `from`, opened; `Sealed` when it isn't theirs, for us, for this context.
pub fn open(
    sealed: &str,
    epk: &str,
    me: &StaticSecret,
    from: &PublicKey,
    context: &str,
) -> Result<Vec<u8>, Failure> {
    let ephemeral = public_key(epk).ok_or(Failure::Sealed)?;
    let combined = decode(sealed).ok_or(Failure::Sealed)?;
    if combined.len() < 12 + TAG {
        return Err(Failure::Sealed);
    }
    let key = wrap_key(&agree(me, &ephemeral)?, &agree(me, from)?, context);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Failure::Sealed)?;
    cipher
        .decrypt(
            Nonce::from_slice(&combined[..12]),
            Payload {
                msg: &combined[12..],
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| Failure::Sealed)
}

// ---- files ----

pub fn encrypted_length(size: u64) -> u64 {
    core::encrypted_len(size)
}

/// A file's own key, and as its facts carry it.
pub fn new_file_key() -> [u8; 32] {
    random::<32>()
}

pub fn file_key(text: &str) -> Option<[u8; 32]> {
    decode(text)?.try_into().ok()
}

fn chunk_nonce(i: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&i.to_be_bytes());
    n
}

fn chunk_aad(i: u64, last: bool) -> [u8; 9] {
    let mut a = [0u8; 9];
    a[..8].copy_from_slice(&i.to_be_bytes());
    a[8] = u8::from(last);
    a
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

/// The plain bytes' SHA-256 as hex, read once (what an offer's facts carry, before the file is sealed).
pub fn hash_file(source: &Path) -> Result<String, Failure> {
    let mut input = File::open(source).map_err(|_| Failure::Unreadable)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let got = read_full(&mut input, &mut buf).map_err(|_| Failure::Unreadable)?;
        if got == 0 {
            break;
        }
        hash.update(&buf[..got]);
    }
    Ok(hex(&hash.finalize()))
}

/// `source` sealed into `dest`; the plain bytes' SHA-256 as hex. `progress` hears how many sealed bytes are in
/// `dest` so far, after each chunk (an upload reads up to there while the rest is sealed).
pub fn encrypt_file(
    source: &Path,
    dest: &Path,
    key: &[u8; 32],
    mut progress: impl FnMut(u64),
) -> Result<String, Failure> {
    let mut input = File::open(source).map_err(|_| Failure::Unreadable)?;
    let total = input.metadata().map_err(|_| Failure::Unreadable)?.len();
    let mut output = File::create(dest).map_err(|_| Failure::Disk)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Failure::Disk)?;
    let chunks = total.div_ceil(CHUNK as u64).max(1);
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut read: u64 = 0;
    let mut sealed: u64 = 0;
    for i in 0..chunks {
        let want = (CHUNK as u64).min(total - read) as usize;
        let got = read_full(&mut input, &mut buf[..want]).map_err(|_| Failure::Unreadable)?;
        if got != want {
            return Err(Failure::Unreadable);
        }
        read += got as u64;
        hash.update(&buf[..got]);
        let aad = chunk_aad(i, i == chunks - 1);
        let ct = cipher
            .encrypt(
                Nonce::from_slice(&chunk_nonce(i)),
                Payload {
                    msg: &buf[..got],
                    aad: &aad,
                },
            )
            .map_err(|_| Failure::Disk)?;
        output.write_all(&ct).map_err(|_| Failure::Disk)?;
        sealed += ct.len() as u64;
        progress(sealed);
    }
    output.flush().map_err(|_| Failure::Disk)?;
    Ok(hex(&hash.finalize()))
}

/// `source` opened into `dest`, checked against `sha256` and the plain `size` the sender sealed (the download
/// must be exactly its encrypted length); `Corrupt` when anything doesn't match.
pub fn decrypt_file(
    source: &Path,
    dest: &Path,
    key: &[u8; 32],
    sha256: &str,
    size: u64,
) -> Result<(), Failure> {
    let mut input = File::open(source).map_err(|_| Failure::Corrupt)?;
    let total = input.metadata().map_err(|_| Failure::Corrupt)?.len();
    if total != encrypted_length(size) {
        return Err(Failure::Corrupt);
    }
    let mut output = File::create(dest).map_err(|_| Failure::Disk)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| Failure::Corrupt)?;
    let sealed = (CHUNK + TAG) as u64;
    let chunks = total.div_ceil(sealed).max(1);
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; CHUNK + TAG];
    let mut written: u64 = 0;
    for i in 0..chunks {
        let got = read_full(&mut input, &mut buf).map_err(|_| Failure::Corrupt)?;
        if got < TAG {
            return Err(Failure::Corrupt);
        }
        let aad = chunk_aad(i, i == chunks - 1);
        let plain = cipher
            .decrypt(
                Nonce::from_slice(&chunk_nonce(i)),
                Payload {
                    msg: &buf[..got],
                    aad: &aad,
                },
            )
            .map_err(|_| Failure::Corrupt)?;
        hash.update(&plain);
        written += plain.len() as u64;
        if written > size {
            return Err(Failure::Corrupt);
        }
        output.write_all(&plain).map_err(|_| Failure::Disk)?;
    }
    output.flush().map_err(|_| Failure::Disk)?;
    if written != size || hex(&hash.finalize()) != sha256 {
        return Err(Failure::Corrupt);
    }
    Ok(())
}

/// A download opened as its parts land (a streamed file, relay/src/files.js `streamPartBytes`): each part is whole
/// sealed chunks, opened in order into `out` with the hash kept running; `finish` checks the size and SHA-256 the
/// sender sealed. Anything that doesn't open is `Corrupt`.
pub struct StreamOpen {
    cipher: Aes256Gcm,
    hash: Sha256,
    chunks: u64,
    next: u64,
    written: u64,
    size: u64,
}

impl StreamOpen {
    pub fn new(key: &[u8; 32], size: u64) -> Result<StreamOpen, Failure> {
        Ok(StreamOpen {
            cipher: Aes256Gcm::new_from_slice(key).map_err(|_| Failure::Corrupt)?,
            hash: Sha256::new(),
            chunks: size.div_ceil(CHUNK as u64).max(1),
            next: 0,
            written: 0,
            size,
        })
    }

    /// The next part's sealed bytes, opened and written to `out`.
    pub fn part(&mut self, sealed: &[u8], out: &mut impl Write) -> Result<(), Failure> {
        for piece in sealed.chunks(CHUNK + TAG) {
            if piece.len() < TAG || self.next >= self.chunks {
                return Err(Failure::Corrupt);
            }
            let last = self.next == self.chunks - 1;
            // only the last chunk may be short
            if !last && piece.len() != CHUNK + TAG {
                return Err(Failure::Corrupt);
            }
            let plain = self
                .cipher
                .decrypt(
                    Nonce::from_slice(&chunk_nonce(self.next)),
                    Payload {
                        msg: piece,
                        aad: &chunk_aad(self.next, last),
                    },
                )
                .map_err(|_| Failure::Corrupt)?;
            self.written += plain.len() as u64;
            if self.written > self.size {
                return Err(Failure::Corrupt);
            }
            self.hash.update(&plain);
            out.write_all(&plain).map_err(|_| Failure::Disk)?;
            self.next += 1;
        }
        Ok(())
    }

    /// Every chunk came, the size is the sender's and so is the hash.
    pub fn finish(self, sha256: &str) -> Result<(), Failure> {
        if self.next != self.chunks
            || self.written != self.size
            || hex(&self.hash.finalize()) != sha256
        {
            return Err(Failure::Corrupt);
        }
        Ok(())
    }
}

// ---- saving ----

/// The Mark of the Web: Windows (SmartScreen, Office's Protected View) treats the file as downloaded from the
/// internet and asks before anything of it runs. False if it couldn't be written.
pub fn mark_of_the_web(path: &Path) -> bool {
    let mut ads = path.as_os_str().to_owned();
    ads.push(":Zone.Identifier");
    std::fs::write(&ads, "[ZoneTransfer]\r\nZoneId=3\r\n").is_ok()
}

pub fn has_mark_of_the_web(path: &Path) -> bool {
    let mut ads = path.as_os_str().to_owned();
    ads.push(":Zone.Identifier");
    std::fs::read_to_string(&ads).is_ok_and(|s| s.contains("ZoneId=3"))
}

/// Moves `from` to `to` only if nothing is there (never replaces, so a race loses only a try).
fn move_new(from: &Path, to: &Path) -> bool {
    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::HSTRING;
    // SAFETY: two live wide strings; no flags, so an existing file is never replaced
    unsafe {
        MoveFileExW(
            &HSTRING::from(from.as_os_str()),
            &HSTRING::from(to.as_os_str()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .is_ok()
}

/// The user's Downloads folder.
pub fn downloads() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: a known folder id; the returned string is freed once read
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// `sealed` (the encrypted download) checked and saved into `dir` as `name` (cleaned again here), or the next
/// free numbered name. `size` is the plain size the sender sealed: the download must be exactly its encrypted
/// length and decrypt to exactly that many bytes with that SHA-256.
pub fn safe_save(
    sealed: &Path,
    key: &[u8; 32],
    sha256: &str,
    size: u64,
    name: &str,
    dir: &Path,
) -> Result<PathBuf, Failure> {
    if size > core::FILE_MAX
        || std::fs::metadata(sealed).map_or(true, |m| m.len() != encrypted_length(size))
    {
        return Err(Failure::Size);
    }
    // hidden, beside where it lands (same volume, so the move is a rename and the mark goes with it)
    let partial = partial_in(dir);
    let done = (|| {
        decrypt_file(sealed, &partial, key, sha256, size).map_err(|e| {
            if e == Failure::Disk {
                Failure::Disk
            } else {
                Failure::Corrupt
            }
        })?;
        place(&partial, name, dir)
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    done
}

const PARTIAL_PREFIX: &str = ".qcalc-";
const PARTIAL_EXT: &str = ".partial";

/// Where a download is opened before it's checked: hidden, in the folder it lands in, never a name a saved file
/// could have.
pub fn partial_in(dir: &Path) -> PathBuf {
    dir.join(format!("{PARTIAL_PREFIX}{}{PARTIAL_EXT}", new_id()))
}

/// A partial file made hidden (it never shows in Explorer while it's being written).
pub fn create_partial(path: &Path) -> Result<File, Failure> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 2;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN)
        .open(path)
        .map_err(|_| Failure::Disk)
}

/// Partial files left in `dir` by a download that never finished (the app quit or crashed mid-way), older than
/// `age`, go.
pub fn sweep_partials(dir: &Path, age: std::time::Duration) {
    let Ok(list) = std::fs::read_dir(dir) else {
        return;
    };
    for e in list.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !(name.starts_with(PARTIAL_PREFIX) && name.ends_with(PARTIAL_EXT)) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d >= age);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// A checked partial file marked with the Mark of the Web, made visible and moved into `dir` as `name` (cleaned
/// again here), or the next free numbered name: it appears there whole, at once. The partial stays on failure (the
/// caller removes it).
pub fn place(partial: &Path, name: &str, dir: &Path) -> Result<PathBuf, Failure> {
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, SetFileAttributesW};
    let name = core::clean_filename(name);
    if !mark_of_the_web(partial) {
        return Err(Failure::Disk);
    }
    // SAFETY: a live wide string
    let _ = unsafe {
        SetFileAttributesW(
            &windows::core::HSTRING::from(partial.as_os_str()),
            FILE_ATTRIBUTE_NORMAL,
        )
    };
    for n in 1..=999u32 {
        let candidate = core::numbered_name(&name, n);
        if candidate.contains(['/', '\\', ':']) || candidate.starts_with('.') {
            continue;
        }
        let dest = dir.join(&candidate);
        if dest.parent() != Some(dir) || dest.exists() {
            continue;
        }
        if move_new(partial, &dest) {
            return Ok(dest);
        }
    }
    Err(Failure::Disk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill<const N: usize>(f: impl Fn(usize) -> usize) -> [u8; N] {
        std::array::from_fn(|i| (f(i) & 0xff) as u8)
    }

    // computed independently with Node's crypto (OpenSSL): scratch vector.js in the port's notes, the same
    // construction as CryptoKit's (X25519, HKDF-SHA256, AES-256-GCM combined box)
    const SENDER_PUB: &str = "B6N8vBQgk8i3VdwbEOhstCY3StFqqFPtC9_AsrhtHHw";
    const RECIP_PUB: &str = "w87qbcKPiR2Ks5-A4WqAKReahrIc4eVp5Tkrr4s1IEk";
    const EPH_PUB: &str = "w02ryB0kPasy5jLyyG-nAQZoZW_DYDlQXihRqoHlbT8";
    const SEALED: &str = "EBESExQVFhcYGRobcxfREbMA9t6R3zA3JeQfVCA-FVLM4QqV6Mx3KIDhTLdQOzkjkYLT4nI";
    const PLAIN: &str = "hello from a mac · 2+2=4";

    fn keys() -> (StaticSecret, StaticSecret, StaticSecret) {
        (
            StaticSecret::from(fill::<32>(|i| i + 1)),
            StaticSecret::from(fill::<32>(|i| 0x40 + i * 3)),
            StaticSecret::from(fill::<32>(|i| 0xa0 ^ (i * 7))),
        )
    }

    fn context() -> String {
        core::share_context(
            "clip",
            "max",
            "alex",
            "0123456789abcdef0123456789abcdef",
            SENDER_PUB,
            RECIP_PUB,
        )
        .unwrap()
    }

    #[test]
    fn the_seal_matches_an_independent_implementation() {
        let (sender, recip, eph) = keys();
        assert_eq!(public_text(&sender), SENDER_PUB);
        assert_eq!(public_text(&recip), RECIP_PUB);
        let (epk, sealed) = seal_with(
            PLAIN.as_bytes(),
            &sender,
            &public_key(RECIP_PUB).unwrap(),
            &context(),
            &eph,
            fill::<12>(|i| 0x10 + i),
        )
        .unwrap();
        assert_eq!(epk, EPH_PUB);
        assert_eq!(sealed, SEALED);
        // and the box made elsewhere opens here
        let plain = open(
            SEALED,
            EPH_PUB,
            &recip,
            &public_key(SENDER_PUB).unwrap(),
            &context(),
        )
        .unwrap();
        assert_eq!(plain, PLAIN.as_bytes());
    }

    #[test]
    fn a_box_opens_only_for_its_recipient_sender_and_context() {
        let (sender, recip, _) = keys();
        let other = new_secret();
        let to = PublicKey::from(&recip);
        let from = PublicKey::from(&sender);
        let (epk, sealed) = seal(b"notes", &sender, &to, &context()).unwrap();
        assert_eq!(
            open(&sealed, &epk, &recip, &from, &context()).unwrap(),
            b"notes"
        );
        assert_eq!(
            open(&sealed, &epk, &other, &from, &context()),
            Err(Failure::Sealed)
        );
        assert_eq!(
            open(&sealed, &epk, &recip, &PublicKey::from(&other), &context()),
            Err(Failure::Sealed)
        );
        let moved = context().replace("clip", "file");
        assert_eq!(
            open(&sealed, &epk, &recip, &from, &moved),
            Err(Failure::Sealed)
        );
        assert!(!sealed.contains("notes"));
        assert!(public_key(&"A".repeat(43)).is_none());
        assert_eq!(new_id().len(), 32);
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn base64url_round_trips() {
        for n in 0..40 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(decode(&encode(&data)).unwrap(), data);
        }
        assert_eq!(encode(b"\xfb\xff"), "-_8");
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qcalc-share-test-{tag}-{}", new_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn file_chunks_match_an_independent_implementation() {
        let d = scratch("vec");
        let key = file_key("d3h5ent8fX5_gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZY").unwrap();
        for (plain, want) in [
            (&b"abcde"[..], "7174c01b3aad60990715cdef37effd8f52fdefbaa5"),
            (&b""[..], "d3ad0e996e8133733836f3ed1369382f"),
        ] {
            let (src, out) = (d.join("plain"), d.join("sealed"));
            std::fs::write(&src, plain).unwrap();
            let sha = encrypt_file(&src, &out, &key, |_| {}).unwrap();
            assert_eq!(hex(&std::fs::read(&out).unwrap()), want);
            assert_eq!(hex(&Sha256::digest(plain)), sha);
        }
        assert_eq!(
            hex(&Sha256::digest(b"abcde")),
            "36bbe50ed96841d10443bcb670d6554f0a34b761be67ec9c4a8ad2c0c44ca42c"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_saves_checked_marked_and_never_over_another() {
        let d = scratch("save");
        let key = new_file_key();
        // two chunks and a bit
        let plain: Vec<u8> = (0..CHUNK * 2 + 3).map(|i| (i % 251) as u8).collect();
        let (src, sealed) = (d.join("in.bin"), d.join("sealed.bin"));
        std::fs::write(&src, &plain).unwrap();
        let sha = encrypt_file(&src, &sealed, &key, |_| {}).unwrap();
        assert_eq!(
            std::fs::metadata(&sealed).unwrap().len(),
            encrypted_length(plain.len() as u64)
        );
        let size = plain.len() as u64;
        let first = safe_save(&sealed, &key, &sha, size, "notes.pdf", &d).unwrap();
        assert_eq!(first.file_name().unwrap(), "notes.pdf");
        assert_eq!(std::fs::read(&first).unwrap(), plain);
        assert!(has_mark_of_the_web(&first));
        let second = safe_save(&sealed, &key, &sha, size, "notes.pdf", &d).unwrap();
        assert_eq!(second.file_name().unwrap(), "notes 2.pdf");
        // a path in the name never escapes the folder
        let third = safe_save(&sealed, &key, &sha, size, "..\\..\\evil.exe", &d).unwrap();
        assert_eq!(third.parent().unwrap(), d.as_path());
        // a wrong hash, a cut download or a flipped byte leave nothing behind
        let before = std::fs::read_dir(&d).unwrap().count();
        assert_eq!(
            safe_save(&sealed, &key, &"0".repeat(64), size, "x.bin", &d),
            Err(Failure::Corrupt)
        );
        let mut bytes = std::fs::read(&sealed).unwrap();
        bytes[10] ^= 1;
        std::fs::write(&sealed, &bytes).unwrap();
        assert_eq!(
            safe_save(&sealed, &key, &sha, size, "x.bin", &d),
            Err(Failure::Corrupt)
        );
        bytes.pop();
        std::fs::write(&sealed, &bytes).unwrap();
        assert_eq!(
            safe_save(&sealed, &key, &sha, size, "x.bin", &d),
            Err(Failure::Size)
        );
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), before);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_streamed_download_opens_part_by_part_and_only_whole() {
        let d = scratch("stream");
        let key = new_file_key();
        let plain: Vec<u8> = (0..CHUNK * 3 + 7).map(|i| (i % 253) as u8).collect();
        let (src, sealed) = (d.join("in.bin"), d.join("sealed.bin"));
        std::fs::write(&src, &plain).unwrap();
        let mut seen = Vec::new();
        let sha = encrypt_file(&src, &sealed, &key, |n| seen.push(n)).unwrap();
        assert_eq!(sha, hash_file(&src).unwrap());
        // each chunk's end, the last the whole length
        assert_eq!(seen.len(), 4);
        assert_eq!(*seen.last().unwrap(), encrypted_length(plain.len() as u64));
        let bytes = std::fs::read(&sealed).unwrap();
        let size = plain.len() as u64;
        // parts of two sealed chunks, as the relay cuts them
        let parts: Vec<&[u8]> = bytes.chunks(2 * (CHUNK + TAG)).collect();
        let mut out = Vec::new();
        let mut open = StreamOpen::new(&key, size).unwrap();
        for p in &parts {
            open.part(p, &mut out).unwrap();
        }
        open.finish(&sha).unwrap();
        assert_eq!(out, plain);
        // a part missing, out of order, cut, or a wrong hash never finishes
        let mut open = StreamOpen::new(&key, size).unwrap();
        open.part(parts[0], &mut Vec::new()).unwrap();
        assert_eq!(open.finish(&sha), Err(Failure::Corrupt));
        let mut open = StreamOpen::new(&key, size).unwrap();
        assert_eq!(open.part(parts[1], &mut Vec::new()), Err(Failure::Corrupt));
        let mut open = StreamOpen::new(&key, size).unwrap();
        assert_eq!(
            open.part(&parts[0][..CHUNK], &mut Vec::new()),
            Err(Failure::Corrupt)
        );
        let mut open = StreamOpen::new(&key, size).unwrap();
        for p in &parts {
            open.part(p, &mut Vec::new()).unwrap();
        }
        assert_eq!(open.finish(&"0".repeat(64)), Err(Failure::Corrupt));
        // a partial is hidden until it's placed, and placing it marks it and shows it
        let partial = partial_in(&d);
        create_partial(&partial).unwrap().write_all(b"x").unwrap();
        let placed = place(&partial, "done.txt", &d).unwrap();
        assert_eq!(placed.file_name().unwrap(), "done.txt");
        assert!(has_mark_of_the_web(&placed));
        assert!(!partial.exists());
        let stale = partial_in(&d);
        drop(create_partial(&stale).unwrap());
        sweep_partials(&d, std::time::Duration::from_secs(3600));
        assert!(stale.exists());
        sweep_partials(&d, std::time::Duration::ZERO);
        assert!(!stale.exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
