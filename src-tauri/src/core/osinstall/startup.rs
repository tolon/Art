//! `S:User-Startup`, edited in place, never regenerated.
//!
//! `S:Startup-Sequence` is not ART's to write — the release ships one that
//! already handles both floppy and hard-disk cases, and it arrives like any
//! other file, through a [`super::PathRule`]. `User-Startup` is different:
//! it is where *components* add their own lines, and — because the user is
//! meant to hand-edit it afterwards — a file `apply` (later) touches once
//! per install and never gets to regenerate.
//!
//! ## The block markers are not ART's invention
//!
//! `;BEGIN <component-id>` … `;END <component-id>` is the convention real
//! Amiga installers already use to mark a section they own inside a shared
//! startup file, so a later run of the same installer — or ART's own
//! re-install — can find and replace only its own lines. [`merge_user_startup`]
//! follows that convention rather than inventing ART's own.
//!
//! ## Edit in place (§39/§40)
//!
//! The same rule this project already applies to `FF.CFG`, `config.txt` and
//! `cmdline.txt`: everything outside the one block being written — the
//! user's own comments, other components' blocks, blank lines, whatever
//! order it was all in — passes through **byte for byte**. [`merge_user_startup`]
//! never re-flows or re-joins the surrounding text; it slices the original
//! string on either side of the block it is replacing (or, when appending,
//! copies it forward unchanged and only ever adds bytes after it).
//!
//! ## An unterminated block is left alone
//!
//! A `;BEGIN <id>` with no matching `;END <id>` is not "close enough" to a
//! block — guessing where somebody's unterminated section was meant to end
//! is how a file gets eaten. When that happens, [`merge_user_startup`]
//! treats the existing text as carrying no block for `component` at all: it
//! leaves every byte of it untouched and appends a fresh, well-formed block
//! after it, rather than trying to repair or reason about text that is
//! already broken.
//!
//! ## Re-running touches only one component's own block
//!
//! Every other component's block — malformed or not — is left exactly as
//! it was found. `merge_user_startup` only ever looks for `;BEGIN
//! <component>` / `;END <component>` naming the one component it was called
//! for.

/// The byte range of `marker`'s own line inside `haystack`, if `marker`
/// occurs there as a **whole line** — not merely as a substring somewhere
/// inside a longer line (so `;BEGIN alpha` never matches inside a line that
/// actually reads `;BEGIN alphabeta`).
///
/// The returned end includes the line's own trailing `\n` when there is
/// one, so callers can drop the whole line — content and terminator
/// together — with one slice. When the marker is the last line of
/// `haystack` and has no trailing newline, the end is `haystack.len()`.
fn find_marker_line(haystack: &str, marker: &str) -> Option<(usize, usize)> {
    let bytes = haystack.as_bytes();
    for (start, _) in haystack.match_indices(marker) {
        let end = start + marker.len();
        let starts_line = start == 0 || bytes[start - 1] == b'\n';
        let ends_line = end == bytes.len() || bytes[end] == b'\n';
        if starts_line && ends_line {
            let end_inclusive = if end < bytes.len() { end + 1 } else { end };
            return Some((start, end_inclusive));
        }
    }
    None
}

/// Merge one component's own lines into `S:User-Startup`, inside a block
/// marked `;BEGIN <component>` / `;END <component>` — the convention real
/// Amiga installers already use (see the module doc comment).
///
/// - `existing` is the file's current content, if it exists yet.
/// - If `existing` already carries a **well-formed** block for `component`
///   (an opening marker with a matching closing marker somewhere after it),
///   that block — and only that block — is replaced with the new one.
///   Everything else in the file is copied through unchanged.
/// - Otherwise a new block is appended after whatever was already there.
///   This is also what happens when `component`'s own opening marker exists
///   with no matching closing marker: the broken text is left exactly as it
///   is, and a fresh block is appended rather than guessing where the
///   broken one was meant to end.
pub fn merge_user_startup(existing: Option<&str>, component: &str, lines: &[String]) -> String {
    let begin_marker = format!(";BEGIN {component}");
    let end_marker = format!(";END {component}");

    let mut block = String::new();
    block.push_str(&begin_marker);
    block.push('\n');
    for line in lines {
        block.push_str(line);
        block.push('\n');
    }
    block.push_str(&end_marker);
    block.push('\n');

    let existing = existing.unwrap_or("");

    if let Some((begin_start, _)) = find_marker_line(existing, &begin_marker) {
        if let Some((_, end_end)) = find_marker_line(&existing[begin_start..], &end_marker) {
            let end_end = begin_start + end_end;
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..begin_start]);
            out.push_str(&block);
            out.push_str(&existing[end_end..]);
            return out;
        }
    }

    // No well-formed block for this component — append. `existing` (broken
    // or merely absent-for-this-component) is copied forward untouched;
    // only a separating newline (when the text does not already end in one)
    // and the new block are ever added after it.
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&block);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_added_with_the_convention_real_installers_use() {
        let out = merge_user_startup(None, "amissl", &["Assign AmiSSL: SYS:Libs/AmiSSL".into()]);
        assert_eq!(
            out,
            ";BEGIN amissl\nAssign AmiSSL: SYS:Libs/AmiSSL\n;END amissl\n"
        );
    }

    /// §39/§40: a hand-tuned file is edited, never regenerated.
    #[test]
    fn everything_the_user_wrote_survives_verbatim() {
        let existing = "; my own line\nAssign WORK: DH1:\n";
        let out = merge_user_startup(
            Some(existing),
            "amissl",
            &["Assign AmiSSL: SYS:Libs/AmiSSL".into()],
        );
        assert!(
            out.starts_with(existing),
            "the user's own lines come first and unchanged"
        );
    }

    #[test]
    fn re_running_replaces_only_this_components_own_block() {
        let first = merge_user_startup(None, "amissl", &["one".into()]);
        let with_user = format!("{first}; something the user added later\n");
        let second = merge_user_startup(Some(&with_user), "amissl", &["two".into()]);

        assert!(second.contains("two"));
        assert!(!second.contains("one"));
        assert!(second.contains("; something the user added later"));
    }

    #[test]
    fn another_components_block_is_left_alone() {
        let a = merge_user_startup(None, "alpha", &["a".into()]);
        let both = merge_user_startup(Some(&a), "beta", &["b".into()]);
        let again = merge_user_startup(Some(&both), "beta", &["b2".into()]);

        assert!(again.contains(";BEGIN alpha\na\n;END alpha"));
        assert!(again.contains("b2"));
    }

    #[test]
    fn an_unterminated_block_is_left_alone_rather_than_swallowed() {
        let broken = ";BEGIN alpha\na\n; the END line is missing\n";
        let out = merge_user_startup(Some(broken), "beta", &["b".into()]);
        assert!(
            out.starts_with(broken),
            "ART does not repair a file it did not break"
        );
    }

    // ---- coverage beyond the brief's own five ----

    /// The brief's five tests never exercise a component's own block being
    /// unterminated *and then merged again for that same component* — every
    /// existing unterminated-block test names a different component
    /// (`"beta"` merging while `"alpha"` is broken). This is the case the
    /// module doc comment calls out by name: re-running for the very
    /// component whose own opening marker has no matching close must still
    /// leave the broken text alone and append, rather than "finding" the
    /// stray opening marker and doing something between replace and append.
    #[test]
    fn an_unterminated_block_for_the_same_component_is_still_left_alone() {
        let broken = ";BEGIN alpha\nold\n; the END line is missing\n";
        let out = merge_user_startup(Some(broken), "alpha", &["new".into()]);
        assert!(
            out.starts_with(broken),
            "the broken block must not be edited, only appended after"
        );
        assert!(out.contains(";BEGIN alpha\nnew\n;END alpha\n"));
    }

    /// Byte-exact preservation, not merely line-equal: a file whose last
    /// line has no trailing newline must keep it that way in the untouched
    /// prefix — a version that reconstructed the file by joining lines with
    /// `"\n"` uniformly would silently add one. This is the falsification
    /// for "verbatim": that bug would still pass `everything_the_user_
    /// wrote_survives_verbatim` above, because that test's own fixture
    /// already ends in `\n`.
    #[test]
    fn a_missing_trailing_newline_in_the_untouched_prefix_is_not_added() {
        let existing = "; no trailing newline on this file";
        let out = merge_user_startup(Some(existing), "amissl", &["x".into()]);
        assert!(
            out.starts_with(existing),
            "the original bytes, including the missing final newline, must \
             appear first and unchanged"
        );
        assert_eq!(
            &out[existing.len()..existing.len() + 1],
            "\n",
            "exactly one separating newline is added before the new block, \
             not folded into the existing text"
        );
    }

    /// Multiple components already merged, replacing one in the middle —
    /// the shape `apply` will actually produce once more than one component
    /// carries `user_startup` lines. Falsification: a version keyed only on
    /// finding *a* `;BEGIN`/`;END` pair (rather than the one naming this
    /// component) would replace the wrong block here.
    #[test]
    fn replacing_the_middle_block_of_three_leaves_the_other_two_untouched() {
        let one = merge_user_startup(None, "alpha", &["a".into()]);
        let two = merge_user_startup(Some(&one), "beta", &["b".into()]);
        let three = merge_user_startup(Some(&two), "gamma", &["c".into()]);

        let updated = merge_user_startup(Some(&three), "beta", &["b2".into()]);

        assert!(updated.contains(";BEGIN alpha\na\n;END alpha"));
        assert!(updated.contains(";BEGIN beta\nb2\n;END beta"));
        assert!(!updated.contains("\nb\n"), "beta's old line must be gone");
        assert!(updated.contains(";BEGIN gamma\nc\n;END gamma"));
    }
}
