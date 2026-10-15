//! Copying text to the system clipboard through the terminal.
//!
//! OSC 52 is an escape sequence that asks the terminal itself to put text on
//! the clipboard. It needs no clipboard crate, no X11 or Wayland libraries,
//! and no display server on the machine the app runs on — which is what makes
//! it work over SSH, where the clipboard that matters is the one on the other
//! end. The cost is that a terminal can ignore it (and tmux does, unless
//! `set-clipboard` is on), so anything copied this way also has a command-line
//! route that prints it.

/// The sequence that puts `text` on the clipboard. `c` names the clipboard
/// proper rather than the X primary selection; BEL ends it, which more
/// terminals accept than the two-byte ST.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard, padded base64 — the one encoding OSC 52 takes. Written out here
/// because it is a dozen lines and the only use the app has for it.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_4648_test_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected, "{input:?}");
        }
    }

    #[test]
    fn text_beyond_ascii_is_encoded_byte_for_byte() {
        // `Flabébé` and `Farfetch’d` are both in an exported team.
        assert_eq!(base64("é".as_bytes()), "w6k=");
    }

    #[test]
    fn the_sequence_targets_the_clipboard_and_ends_with_bel() {
        assert_eq!(osc52("foo"), "\x1b]52;c;Zm9v\x07");
    }
}
