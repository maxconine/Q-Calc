//! The dimmed answer that outlives a keystroke mid-edit (`src/lib/steady.ts`). The timing is the UI's; whether the
//! new text is an edit of the old is decided here.

/// How long a dimmed answer outlives the last keystroke before the slot goes honestly blank.
pub const STEADY_MS: u64 = 1800;
/// The fade of the dimmed answer.
pub const STEADY_FADE_MS: u64 = 260;

/// True when `next` reads as `prev` mid-edit (typed on, backspaced, changed in the middle), not new input.
pub fn is_edit_of(next: &str, prev: &str) -> bool {
    use crate::app::js::trim;
    if trim(next).is_empty() || trim(prev).is_empty() || next == prev {
        return false;
    }
    let a: Vec<u16> = next.encode_utf16().collect();
    let b: Vec<u16> = prev.encode_utf16().collect();
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head && tail < b.len() - head && a[a.len() - 1 - tail] == b[b.len() - 1 - tail] {
        tail += 1;
    }
    head + tail >= b.len().div_ceil(2)
}
