//! Text hygiene for repository metadata.
//!
//! Everything the engine reads from a repository is untrusted input (§56): the
//! index and the readmes are prose written by thousands of people over three
//! decades, and a hostile upload is cheap. Two rules apply everywhere:
//!
//! - **Bound before you keep.** No field grows to whatever the input says.
//! - **Strip control characters.** An ANSI escape run in a package description
//!   would repaint the terminal of anyone who exports the catalog, and a NUL or
//!   a newline smuggled into a one-line field lets one record forge another.
//!   This is also the first line of defence for §45.5.7: text that reaches the
//!   AI layer later cannot carry its own formatting.

/// The longest prefix of `s` that is at most `max` bytes and ends on a
/// character boundary.
///
/// `str::truncate`-style slicing panics mid-character; input this module sees
/// is routinely multi-byte, so the boundary walk is not optional.
pub(super) fn truncate_at_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Normalise a free-text field: drop control characters, collapse runs of
/// whitespace, trim, and bound the result to `max` bytes.
pub(super) fn sanitise_text(raw: &str, max: usize) -> String {
    // Bound the *input* first: a 10 MB "one-line description" must not be
    // walked character by character just to throw it away.
    let bounded = truncate_at_char_boundary(raw, max.saturating_mul(4).max(max));

    let mut out = String::with_capacity(bounded.len().min(max));
    let mut pending_space = false;

    for ch in bounded.chars() {
        if ch.is_control() || ch == '\u{7f}' {
            // Control characters become a single space rather than vanishing,
            // so "a\u{1b}[31mb" cannot silently become the word "ab".
            pending_space = !out.is_empty();
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            if out.len() + 1 >= max {
                break;
            }
            out.push(' ');
            pending_space = false;
        }
        if out.len() + ch.len_utf8() > max {
            break;
        }
        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_lands_on_a_character_boundary() {
        // "ü" is two bytes; cutting at 2 must not split it.
        let s = "aüb";
        assert_eq!(truncate_at_char_boundary(s, 2), "a");
        assert_eq!(truncate_at_char_boundary(s, 3), "aü");
        assert_eq!(truncate_at_char_boundary(s, 99), "aüb");
    }

    #[test]
    fn control_characters_become_spaces_not_nothing() {
        assert_eq!(sanitise_text("a\u{1b}[31mb", 64), "a [31mb");
        assert_eq!(sanitise_text("one\ttwo", 64), "one two");
        assert_eq!(sanitise_text("nul\0byte", 64), "nul byte");
    }

    /// A newline smuggled into a one-line field would let one catalog record
    /// forge another.
    #[test]
    fn newlines_cannot_survive_a_one_line_field() {
        let out = sanitise_text("Real description\nutil/fake/Evil.lha 1K 0+Fake", 256);
        assert!(!out.contains('\n'));
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(sanitise_text("   a    b   ", 64), "a b");
        assert_eq!(sanitise_text("     ", 64), "");
    }

    #[test]
    fn output_is_bounded() {
        let out = sanitise_text(&"x".repeat(10_000), 32);
        assert!(out.len() <= 32, "got {} bytes", out.len());
    }

    /// The bound is on bytes, and must still leave valid UTF-8 behind.
    #[test]
    fn a_multibyte_field_is_bounded_without_splitting_a_character() {
        let out = sanitise_text(&"ü".repeat(1000), 31);
        assert!(out.len() <= 31);
        assert_eq!(out.chars().count(), 15);
    }
}
