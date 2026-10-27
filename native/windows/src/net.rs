//! HTTPS for the background jobs (the ECB rates, the update feed and download, the usage counts), through WinHTTP:
//! part of Windows, so no TLS stack is linked in and the binary stays small; it follows redirects (GitHub's
//! release downloads) and the system's proxy settings. Blocking calls, made only on background threads.

use std::io::Write;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_DECOMPRESSION, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WinHttpSetTimeouts,
};
use windows::core::{HSTRING, PCWSTR};

/// An https URL's host and path ("/a/b?c"); anything else is refused.
pub fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let ok = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    ok.then_some((host, path))
}

struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a handle WinHTTP gave us, closed once
            let _ = unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn open(what: &str) -> Result<Handle, String> {
    let _ = what;
    // SAFETY: plain session creation; the strings outlive the call
    let session = Handle(unsafe {
        WinHttpOpen(
            &HSTRING::from(format!("QCalc/{}", crate::update::VERSION)),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(last_error("open"));
    }
    // SAFETY: a live session
    unsafe {
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 30_000);
        // gzip / deflate, decoded by WinHTTP (WINHTTP_DECOMPRESSION_FLAG_ALL)
        let flags = 3u32.to_le_bytes();
        let _ = WinHttpSetOption(
            Some(session.0),
            WINHTTP_OPTION_DECOMPRESSION,
            Some(&flags),
        );
    }
    Ok(session)
}

fn last_error(what: &str) -> String {
    format!("{what}: {}", windows::core::Error::from_win32().message())
}

/// One request; `sink` gets the body in pieces and may stop it by returning false. The status code back.
fn request(
    verb: &str,
    url: &str,
    body: Option<(&str, &[u8])>,
    mut sink: impl FnMut(&[u8]) -> bool,
) -> Result<u32, String> {
    let (host, path) = split_url(url).ok_or_else(|| format!("not an https URL: {url}"))?;
    let session = open(url)?;
    // SAFETY: live handles; strings outlive each call; buffers are sized locals
    unsafe {
        let connect = Handle(WinHttpConnect(
            session.0,
            &HSTRING::from(host),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return Err(last_error("connect"));
        }
        let req = Handle(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from(verb),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if req.0.is_null() {
            return Err(last_error("request"));
        }
        let (headers, data): (Vec<u16>, &[u8]) = match body {
            Some((kind, data)) => (format!("Content-Type: {kind}\r\n").encode_utf16().collect(), data),
            None => (Vec::new(), &[]),
        };
        let len = data.len() as u32;
        WinHttpSendRequest(
            req.0,
            (!headers.is_empty()).then_some(&headers[..]),
            (!data.is_empty()).then_some(data.as_ptr().cast()),
            len,
            len,
            0,
        )
        .map_err(|e| format!("send: {}", e.message()))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut())
            .map_err(|e| format!("response: {}", e.message()))?;
        let mut status = 0u32;
        let mut size = 4u32;
        let mut index = 0u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            &mut index,
        )
        .map_err(|e| format!("status: {}", e.message()))?;
        if !(200..300).contains(&status) {
            return Ok(status);
        }
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(req.0, buf.as_mut_ptr().cast(), buf.len() as u32, &mut read)
                .map_err(|e| format!("read: {}", e.message()))?;
            if read == 0 || !sink(&buf[..read as usize]) {
                break;
            }
        }
        Ok(status)
    }
}

/// GET into memory, at most `limit` bytes (more is an error).
pub fn get(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut over = false;
    let status = request("GET", url, None, |piece| {
        if out.len() + piece.len() > limit {
            over = true;
            return false;
        }
        out.extend_from_slice(piece);
        true
    })?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    if over {
        return Err(format!("more than {limit} bytes"));
    }
    Ok(out)
}

/// GET into `file` through `each` (a running hash), at most `limit` bytes.
pub fn download(
    url: &str,
    file: &mut std::fs::File,
    limit: u64,
    mut each: impl FnMut(&[u8]),
) -> Result<u64, String> {
    let mut total = 0u64;
    let mut failed: Option<String> = None;
    let status = request("GET", url, None, |piece| {
        total += piece.len() as u64;
        if total > limit {
            failed = Some(format!("more than {limit} bytes"));
            return false;
        }
        each(piece);
        if let Err(e) = file.write_all(piece) {
            failed = Some(e.to_string());
            return false;
        }
        true
    })?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    match failed {
        Some(e) => Err(e),
        None => Ok(total),
    }
}

/// POST `body`; the status code.
pub fn post(url: &str, content_type: &str, body: &[u8]) -> Result<u32, String> {
    request("POST", url, Some((content_type, body)), |_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_urls() {
        assert_eq!(
            split_url("https://www.ecb.europa.eu/stats/x.xml"),
            Some(("www.ecb.europa.eu", "/stats/x.xml"))
        );
        assert_eq!(split_url("https://example.com"), Some(("example.com", "/")));
        assert_eq!(split_url("http://example.com/x"), None);
        assert_eq!(split_url("https://ex ample.com/x"), None);
        assert_eq!(split_url("https://user@host/x"), None);
        assert_eq!(split_url("https:///x"), None);
    }
}
