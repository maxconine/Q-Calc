//! An image on the clipboard as a PNG file, for `paste to <name>` (the Mac's MessageFiles.swift sends a copied
//! image as `clipboard.png`). A browser's or the Snipping Tool's own "PNG" format is taken as it is; otherwise the
//! bitmap (CF_DIBV5 / CF_DIB, 24 or 32 bits) is written as a PNG here: no image library, the pixels as zlib's
//! stored blocks (lossless, uncompressed). Pure apart from reading the clipboard (UI thread); the encoding runs on
//! a worker.

/// What the clipboard had.
pub enum Copied {
    Png(Vec<u8>),
    Dib(Vec<u8>),
}

/// The clipboard's image, if any (UI thread; the bytes are copied out while it's open).
pub fn read(owner: windows::Win32::Foundation::HWND) -> Option<Copied> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    const CF_DIB: u32 = 8;
    const CF_DIBV5: u32 = 17;
    // SAFETY: the clipboard is opened, read and closed here; each handle is locked, copied and unlocked while open
    unsafe {
        let png = RegisterClipboardFormatW(windows::core::w!("PNG"));
        let format = [(png, true), (CF_DIBV5, false), (CF_DIB, false)]
            .into_iter()
            .find(|(f, _)| *f != 0 && IsClipboardFormatAvailable(*f).is_ok())?;
        OpenClipboard(Some(owner)).ok()?;
        let bytes = GetClipboardData(format.0).ok().and_then(|h| {
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                return None;
            }
            let v = std::slice::from_raw_parts(p, GlobalSize(g)).to_vec();
            let _ = GlobalUnlock(g);
            Some(v)
        });
        let _ = CloseClipboard();
        let bytes = bytes?;
        Some(if format.1 {
            Copied::Png(bytes)
        } else {
            Copied::Dib(bytes)
        })
    }
}

/// The PNG bytes for what was copied (a worker); None for a bitmap this can't read.
pub fn png(c: Copied) -> Option<Vec<u8>> {
    match c {
        Copied::Png(b) => b.starts_with(b"\x89PNG\r\n\x1a\n").then_some(b),
        Copied::Dib(b) => {
            let (w, h, rgba) = dib_to_rgba(&b)?;
            Some(encode(w, h, &rgba))
        }
    }
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// A packed DIB (header, masks, pixels) as top-down RGBA rows. 24- and 32-bit only.
pub fn dib_to_rgba(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let header = u32_at(b, 0)? as usize;
    let w = i32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    let h = i32::from_le_bytes(b.get(8..12)?.try_into().ok()?);
    let bits = u16::from_le_bytes(b.get(14..16)?.try_into().ok()?);
    let compression = u32_at(b, 16)?;
    if w <= 0
        || h == 0
        || !(bits == 24 || bits == 32)
        || header < 40
        || w > 32_768
        || h.abs() > 32_768
    {
        return None;
    }
    // BI_BITFIELDS with the 40-byte header puts three masks after it
    let masks_after = if compression == 3 && header == 40 {
        12
    } else {
        0
    };
    if compression != 0 && compression != 3 {
        return None;
    }
    let (rm, gm, bm, am) = if compression == 3 {
        // the masks follow the 40-byte header, or are its V4/V5 fields at the same place
        let at = 40;
        (
            u32_at(b, at)?,
            u32_at(b, at + 4)?,
            u32_at(b, at + 8)?,
            if header >= 56 { u32_at(b, 52)? } else { 0 },
        )
    } else {
        (0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0)
    };
    let start = header + masks_after;
    let (wu, hu) = (w as usize, h.unsigned_abs() as usize);
    let stride = (wu * usize::from(bits) / 8).div_ceil(4) * 4;
    let pixels = b.get(start..start + stride * hu)?;
    let shift = |m: u32| if m == 0 { 0 } else { m.trailing_zeros() };
    let take = |v: u32, m: u32| {
        if m == 0 {
            0
        } else {
            ((v & m) >> shift(m)) as u8
        }
    };
    let mut out = vec![0u8; wu * hu * 4];
    let mut any_alpha = false;
    for row in 0..hu {
        // bottom-up unless the height is negative
        let src = if h > 0 { hu - 1 - row } else { row };
        let line = &pixels[src * stride..src * stride + stride];
        for x in 0..wu {
            let o = (row * wu + x) * 4;
            if bits == 24 {
                let p = &line[x * 3..x * 3 + 3];
                out[o..o + 4].copy_from_slice(&[p[2], p[1], p[0], 255]);
            } else {
                let v = u32::from_le_bytes(line[x * 4..x * 4 + 4].try_into().ok()?);
                let a = if am != 0 {
                    take(v, am)
                } else {
                    (v >> 24) as u8
                };
                any_alpha |= a != 0;
                out[o..o + 4].copy_from_slice(&[take(v, rm), take(v, gm), take(v, bm), a]);
            }
        }
    }
    // most apps leave a 32-bit bitmap's alpha at 0: that's an opaque image, not an invisible one
    if bits == 32 && !any_alpha {
        for px in out.chunks_mut(4) {
            px[3] = 255;
        }
    }
    Some((wu as u32, hu as u32, out))
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for b in data {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// RGBA rows as a PNG (RGB when every pixel is opaque).
pub fn encode(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let opaque = rgba.chunks(4).all(|p| p[3] == 255);
    let bpp = if opaque { 3 } else { 4 };
    let mut raw = Vec::with_capacity((w as usize * bpp + 1) * h as usize);
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        for p in row.chunks(4) {
            raw.extend_from_slice(&p[..bpp]);
        }
    }
    // zlib: stored blocks, then Adler-32
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for x in &raw {
        a = (a + u32::from(*x)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, if opaque { 2 } else { 6 }, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dib(w: i32, h: i32, bits: u16, px: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&w.to_le_bytes());
        b[8..12].copy_from_slice(&h.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(px);
        b
    }

    #[test]
    fn a_bitmap_reads_bottom_up_with_padding() {
        // 1×2, 24-bit: each row 3 bytes + 1 pad; bottom row first (blue), then top (red)
        let b = dib(1, 2, 24, &[255, 0, 0, 0, 0, 0, 255, 0]);
        let (w, h, px) = dib_to_rgba(&b).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(px, [255, 0, 0, 255, 0, 0, 255, 255]);
        // 32-bit with no alpha anywhere is opaque
        let b = dib(1, -1, 32, &[1, 2, 3, 0]);
        assert_eq!(dib_to_rgba(&b).unwrap().2, [3, 2, 1, 255]);
        assert!(dib_to_rgba(&dib(1, 1, 8, &[0, 0, 0, 0])).is_none());
    }

    #[test]
    fn the_png_is_well_formed() {
        let png = encode(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        // the IHDR's CRC as zlib's crc32 has it
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert!(png.ends_with(&[0xae, 0x42, 0x60, 0x82]));
        // RGB: 1 filter byte + 6 bytes, stored
        let idat_len = u32::from_be_bytes(png[33..37].try_into().unwrap());
        assert_eq!(idat_len, 2 + 5 + 7 + 4);
    }
}
