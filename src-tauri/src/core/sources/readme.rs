//! Aminet readme field extraction (§41.5.2).
//!
//! Aminet readmes conventionally begin with labelled fields:
//!
//! ```text
//! Short:        Portable SSL/TLS library
//! Author:       Jens Maus
//! Uploader:     jens maus de
//! Type:         util/libs
//! Version:      5.5
//! Requires:     AmigaOS 3.0, MUI 3.8
//! Distribution: Aminet
//! ```
//!
//! They are also hand-written, thirty years deep, and frequently creative:
//! fields repeat, wrap, arrive in any case, and sometimes are not fields at all
//! but a header drawn in ASCII art. So the parser is deliberately narrow — it
//! recognises a fixed set of labels and ignores everything else — and every
//! value it keeps is [sanitised](super::text::sanitise_text) and bounded.
//!
//! **Extraction is not interpretation.** A `Version:` field here is a string,
//! not a version number; it is turned into a [`Claim`](super::Claim) with
//! `Medium` confidence at best, and the full readme is always available to the
//! user verbatim. ART never strips a package's own licence text (§41.5.5).

use super::text::{sanitise_text, truncate_at_char_boundary};

/// How much of a readme is scanned for fields.
///
/// The header is the first few hundred bytes by convention. Scanning a whole
/// 10 MB readme to find fields that are not there is wasted work, and reading
/// unbounded input is how the rest of this codebase gets into trouble.
const MAX_SCAN_BYTES: usize = 64 * 1024;

/// The most bytes kept for any single field value.
const MAX_FIELD_BYTES: usize = 256;

/// The most `Requires:` entries kept from one readme.
const MAX_REQUIRES: usize = 32;

/// The most continuation lines folded into one field.
const MAX_CONTINUATION_LINES: usize = 8;

/// Fields recovered from a readme header. Every value is already sanitised and
/// bounded; absent means "not found", never "empty".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ReadmeFields {
    pub short: Option<String>,
    pub author: Option<String>,
    pub uploader: Option<String>,
    /// The `Type:` field. Named `kind` because `type` is a Rust keyword.
    pub kind: Option<String>,
    pub version: Option<String>,
    pub requires: Vec<String>,
    pub distribution: Option<String>,
    /// Names of fields that appeared more than once with *different* values.
    ///
    /// The first value wins, but the disagreement is remembered so the caller
    /// can demote its confidence rather than pretending the readme was clear.
    pub ambiguous: Vec<String>,
}

impl ReadmeFields {
    /// Whether `field` was seen more than once with conflicting values.
    pub fn is_ambiguous(&self, field: &str) -> bool {
        self.ambiguous.iter().any(|f| f == field)
    }

    fn note_ambiguous(&mut self, field: &str) {
        if !self.is_ambiguous(field) {
            self.ambiguous.push(field.to_string());
        }
    }
}

/// Parse readme bytes. Invalid UTF-8 is replaced, never rejected — a readme
/// from 1994 is as likely to be Latin-1 as anything else, and a mangled
/// character is no reason to lose the whole file.
pub fn parse_readme_bytes(bytes: &[u8]) -> ReadmeFields {
    let bounded = &bytes[..bytes.len().min(MAX_SCAN_BYTES)];
    parse_readme(&String::from_utf8_lossy(bounded))
}

/// Parse a readme's header fields.
pub fn parse_readme(text: &str) -> ReadmeFields {
    let head = truncate_at_char_boundary(text, MAX_SCAN_BYTES);
    let mut fields = ReadmeFields::default();

    let lines: Vec<&str> = head.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let Some((label, first_value)) = split_field(lines[i]) else {
            i += 1;
            continue;
        };

        // Fold continuation lines: indented, non-empty, and not themselves a
        // new field.
        let mut value = first_value.to_string();
        let mut folded = 0;
        let mut j = i + 1;
        while j < lines.len() && folded < MAX_CONTINUATION_LINES {
            let next = lines[j];
            let is_continuation = next.starts_with([' ', '\t'])
                && !next.trim().is_empty()
                && split_field(next).is_none();
            if !is_continuation {
                break;
            }
            value.push(' ');
            value.push_str(next);
            folded += 1;
            j += 1;
        }
        i = j;

        store(&mut fields, &label, &value);
    }

    fields
}

/// Split `Label: value` when the line looks like a field header.
///
/// Narrow on purpose: the label must start at column 0, be alphabetic (plus
/// spaces, for `Short description:`-style labels), and be short. This is what
/// keeps a line of prose containing a colon — or a URL — from being read as a
/// field.
fn split_field(line: &str) -> Option<(String, &str)> {
    if line.starts_with([' ', '\t']) {
        return None;
    }
    let colon = line.find(':')?;
    let label = &line[..colon];
    if label.is_empty() || label.len() > 24 {
        return None;
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-')
    {
        return None;
    }
    Some((label.trim().to_ascii_lowercase(), &line[colon + 1..]))
}

/// Record one recognised field, keeping the first value and noting conflicts.
fn store(fields: &mut ReadmeFields, label: &str, raw_value: &str) {
    let value = sanitise_text(raw_value, MAX_FIELD_BYTES);
    if value.is_empty() {
        return;
    }

    // `Requires:` accumulates; every other field is single-valued.
    if label == "requires" {
        let mut parts: Vec<&str> = value.split([',', ';']).collect();

        // A value that hit the length bound may end half-way through an entry,
        // and half a dependency name is worse than a missing one. ASCII values
        // land exactly on the bound; the slack covers multi-byte tails.
        let was_truncated = value.len() >= MAX_FIELD_BYTES.saturating_sub(4);
        if was_truncated && parts.len() > 1 {
            parts.pop();
        }

        for part in parts {
            let item = part.trim();
            if item.is_empty() || fields.requires.len() >= MAX_REQUIRES {
                continue;
            }
            let item = item.to_string();
            if !fields.requires.contains(&item) {
                fields.requires.push(item);
            }
        }
        return;
    }

    let conflict = {
        let slot = match label {
            "short" => &mut fields.short,
            "author" => &mut fields.author,
            "uploader" => &mut fields.uploader,
            "type" => &mut fields.kind,
            "version" => &mut fields.version,
            "distribution" => &mut fields.distribution,
            _ => return,
        };

        match slot {
            Some(existing) => existing != &value,
            None => {
                *slot = Some(value);
                false
            }
        }
    };

    if conflict {
        fields.note_ambiguous(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPICAL: &str = "\
Short:        Portable SSL/TLS library
Author:       Jens Maus
Uploader:     jens maus de
Type:         util/libs
Version:      5.5
Requires:     AmigaOS 3.0, MUI 3.8
Distribution: Aminet

This is the body of the readme, which is not parsed for fields.
Homepage: https://example.invalid/amissl
";

    #[test]
    fn reads_the_conventional_header() {
        let f = parse_readme(TYPICAL);
        assert_eq!(f.short.as_deref(), Some("Portable SSL/TLS library"));
        assert_eq!(f.author.as_deref(), Some("Jens Maus"));
        assert_eq!(f.kind.as_deref(), Some("util/libs"));
        assert_eq!(f.version.as_deref(), Some("5.5"));
        assert_eq!(f.distribution.as_deref(), Some("Aminet"));
        assert_eq!(f.requires, vec!["AmigaOS 3.0", "MUI 3.8"]);
        assert!(f.ambiguous.is_empty());
    }

    #[test]
    fn labels_are_case_insensitive() {
        let f = parse_readme("VERSION: 2.1\nauthor: Somebody\n");
        assert_eq!(f.version.as_deref(), Some("2.1"));
        assert_eq!(f.author.as_deref(), Some("Somebody"));
    }

    #[test]
    fn missing_fields_are_none_not_empty() {
        let f = parse_readme("just some prose, no fields at all\n");
        assert_eq!(f, ReadmeFields::default());
    }

    /// A `Version:` line inside the body — or two conflicting headers — must not
    /// be presented as a clean answer.
    #[test]
    fn a_repeated_conflicting_field_is_marked_ambiguous() {
        let f = parse_readme("Version: 1.0\nAuthor: A\nVersion: 2.0\n");
        assert_eq!(f.version.as_deref(), Some("1.0"), "the first value wins");
        assert!(f.is_ambiguous("version"));
    }

    /// Repeating a field with the *same* value is just formatting, not a
    /// disagreement.
    #[test]
    fn a_repeated_identical_field_is_not_ambiguous() {
        let f = parse_readme("Version: 1.0\nVersion: 1.0\n");
        assert!(!f.is_ambiguous("version"));
    }

    #[test]
    fn continuation_lines_are_folded_into_the_value() {
        let f = parse_readme(
            "Short:  A very long description\n        that wraps onto a second line\nAuthor: X\n",
        );
        assert_eq!(
            f.short.as_deref(),
            Some("A very long description that wraps onto a second line")
        );
        assert_eq!(f.author.as_deref(), Some("X"));
    }

    /// Prose containing a colon is not a field. Without this, every readme
    /// mentioning a URL would grow a bogus "https" field.
    #[test]
    fn prose_and_urls_are_not_mistaken_for_fields() {
        let f =
            parse_readme("See https://example.invalid/x for details\nNote: this line is fine\n");
        // "See https" is not a valid label (contains '/' and is long), and
        // "Note" is not a recognised field.
        assert_eq!(f, ReadmeFields::default());
    }

    #[test]
    fn indented_lines_are_never_new_fields() {
        let f = parse_readme("   Version: 9.9\n");
        assert_eq!(f.version, None);
    }

    /// §45.5.7 groundwork: nothing from a readme carries its own formatting.
    #[test]
    fn control_characters_and_ansi_escapes_are_stripped() {
        let f = parse_readme("Version: 1.\u{1b}[31m0\u{0}\n");
        let v = f.version.unwrap();
        assert!(!v.contains('\u{1b}'), "escape survived: {v:?}");
        assert!(!v.contains('\0'));
    }

    #[test]
    fn field_values_are_bounded() {
        let f = parse_readme(&format!("Author: {}\n", "A".repeat(100_000)));
        assert!(f.author.unwrap().len() <= MAX_FIELD_BYTES);
    }

    #[test]
    fn requires_is_split_and_deduplicated() {
        let f = parse_readme("Requires: MUI 3.8, AmigaOS 3.0; MUI 3.8\n");
        assert_eq!(f.requires, vec!["MUI 3.8", "AmigaOS 3.0"]);
    }

    #[test]
    fn requires_is_bounded_by_count() {
        let many = (0..100)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let f = parse_readme(&format!("Requires: {many}\n"));
        assert_eq!(f.requires.len(), MAX_REQUIRES);
    }

    /// The length bound cuts the value wherever it falls. A dependency called
    /// "dep" that is really half of "dep3" would be a fabricated fact, so the
    /// possibly-truncated tail is dropped.
    #[test]
    fn a_truncated_requires_list_does_not_invent_a_partial_entry() {
        let many = (0..200)
            .map(|i| format!("dep{}", i % 5))
            .collect::<Vec<_>>()
            .join(", ");
        let f = parse_readme(&format!("Requires: {many}\n"));

        assert_eq!(f.requires, vec!["dep0", "dep1", "dep2", "dep3", "dep4"]);
    }

    #[test]
    fn a_ten_megabyte_readme_is_not_walked_end_to_end() {
        let mut text = String::from("Version: 3.0\n");
        text.push_str(&"filler line\n".repeat(900_000));
        text.push_str("Version: 99.0\n");

        let f = parse_readme(&text);
        assert_eq!(f.version.as_deref(), Some("3.0"));
        assert!(
            !f.is_ambiguous("version"),
            "the trailing field is past the scan window and must not be seen"
        );
    }

    #[test]
    fn invalid_utf8_is_replaced_not_rejected() {
        // Latin-1 "Jörg" — 0xF6 is not valid UTF-8.
        let bytes = b"Author: J\xf6rg\nVersion: 1.0\n";
        let f = parse_readme_bytes(bytes);
        assert_eq!(f.version.as_deref(), Some("1.0"));
        assert!(f.author.is_some());
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let f = parse_readme("Version: 1.2\r\nAuthor: X\r\n");
        assert_eq!(f.version.as_deref(), Some("1.2"));
        assert_eq!(f.author.as_deref(), Some("X"));
    }
}
