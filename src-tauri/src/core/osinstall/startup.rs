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
//! follows that convention rather than inventing ART's own. Line endings can
//! be either `\n` or `\r\n` — a hand-edited file on this project's own
//! Windows host is as likely to carry Notepad's CRLF as an Amiga's bare LF,
//! and a marker line is still the same marker line either way.
//!
//! ## Edit in place (§39/§40)
//!
//! The same rule this project already applies to `FF.CFG`, `config.txt` and
//! `cmdline.txt`: everything outside the block(s) being touched — the
//! user's own comments, other components' blocks, blank lines, whatever
//! order it was all in — passes through **byte for byte**. [`merge_user_startup`]
//! never re-flows or re-joins the surrounding text; it slices the original
//! string around the block(s) it is replacing (or, when appending, copies it
//! forward unchanged and only ever adds bytes after it).
//!
//! ## Pairing: the closest opener wins, not the first
//!
//! A block is only "well-formed" — eligible to be replaced — when its own
//! `;BEGIN <id>` is paired with the `;END <id>` that actually closes it.
//! [`merge_user_startup`] finds every such pair by scanning markers in
//! order and letting each new opener replace whichever one was open before
//! it, so a closer always pairs with the **most recent** unclosed opener —
//! never an earlier, already-abandoned one. This matters for a very
//! concrete failure: a stray `;BEGIN <id>` with no closer of its own,
//! followed later by a genuine, complete block for the same id, used to get
//! matched first-opener-to-first-closer-after-it — which reaches *past* the
//! real block's own opener and swallows everything from the stray opener
//! through the real block's close, including the user's own text sitting
//! between them. That is exactly the file-eating outcome this module
//! exists to prevent, and it does not stop mattering after the first run:
//! the stray opener is still there on every run after, so the pairing has
//! to get this right every time, not just once.
//!
//! ## An opener with no closer at all is left alone
//!
//! When an opener never finds a closer anywhere in the file, it is not
//! "close enough" to a block — guessing where somebody's unterminated
//! section was meant to end is how a file gets eaten. [`merge_user_startup`]
//! treats that text as carrying no matched block for `component` at all: it
//! leaves every byte of it untouched and appends a fresh, well-formed block
//! after it, rather than trying to repair or reason about text that is
//! already broken.
//!
//! ## More than one well-formed block collapses into one
//!
//! If `component`'s block genuinely appears more than once — a leftover
//! from some earlier bug, say — every complete occurrence is found. The
//! first is replaced with the fresh block; every later one is dropped
//! entirely, so a duplicate never survives a merge and the file never grows
//! a second copy of the same component's block.
//!
//! ## Re-running touches only one component's own block
//!
//! Every other component's block — malformed or not — is left exactly as
//! it was found. `merge_user_startup` only ever looks for `;BEGIN
//! <component>` / `;END <component>` naming the one component it was called
//! for.

/// Whether `haystack[pos..]` begins with a line terminator, or `pos` is the
/// end of the string — the shapes a marker's own line can end in. `\n` and
/// `\r\n` both count (see the module doc comment on line endings); a bare
/// `\r` does not, since nothing on this project's own two platforms writes
/// old-style Mac line endings. Returns the terminator's byte length (`0` at
/// end of string) so a caller can skip past it in one step.
fn line_terminator_len(haystack: &str, pos: usize) -> Option<usize> {
    let bytes = haystack.as_bytes();
    if pos == bytes.len() {
        return Some(0);
    }
    if bytes[pos] == b'\n' {
        return Some(1);
    }
    if bytes[pos] == b'\r' && bytes.get(pos + 1) == Some(&b'\n') {
        return Some(2);
    }
    None
}

/// Every byte range of `marker`'s own **whole line** occurrences inside
/// `haystack` — not merely a substring somewhere inside a longer line (so
/// `;BEGIN alpha` never matches inside a line that actually reads `;BEGIN
/// alphabeta`) — in the order they appear.
///
/// Each returned range's end includes the line's own terminator, so a
/// caller can drop the whole line — content and terminator together — with
/// one slice.
fn marker_lines(haystack: &str, marker: &str) -> Vec<(usize, usize)> {
    let bytes = haystack.as_bytes();
    let mut out = Vec::new();
    for (start, _) in haystack.match_indices(marker) {
        let end = start + marker.len();
        let starts_line = start == 0 || bytes[start - 1] == b'\n';
        if !starts_line {
            continue;
        }
        if let Some(term_len) = line_terminator_len(haystack, end) {
            out.push((start, end + term_len));
        }
    }
    out
}

/// Every well-formed `;BEGIN <component>` / `;END <component>` pair in
/// `haystack`, in the order they appear — see the module doc comment's
/// "Pairing" section for why an opener only ever pairs with the closest
/// closer that follows it, never a more distant one.
fn matched_blocks(haystack: &str, begin_marker: &str, end_marker: &str) -> Vec<(usize, usize)> {
    enum Event {
        Begin(usize),
        End(usize),
    }

    let mut events: Vec<(usize, Event)> = Vec::new();
    for (start, _) in marker_lines(haystack, begin_marker) {
        events.push((start, Event::Begin(start)));
    }
    for (start, end) in marker_lines(haystack, end_marker) {
        events.push((start, Event::End(end)));
    }
    events.sort_by_key(|(pos, _)| *pos);

    let mut pairs = Vec::new();
    let mut open: Option<usize> = None;
    for (_, event) in events {
        match event {
            // A new opener always replaces whichever one was open before
            // it — an earlier opener that never closed is simply
            // abandoned, never paired with a later block's own close.
            Event::Begin(start) => open = Some(start),
            Event::End(end) => {
                if let Some(begin_start) = open.take() {
                    pairs.push((begin_start, end));
                }
                // A closer with nothing open is a stray `;END` with no
                // opener at all — not a shape this file's own convention
                // produces, and not this function's to repair; it is
                // simply not part of any pair, exactly like an unclosed
                // opener.
            }
        }
    }
    pairs
}

/// Merge one component's own lines into `S:User-Startup`, inside a block
/// marked `;BEGIN <component>` / `;END <component>` — the convention real
/// Amiga installers already use (see the module doc comment).
///
/// - `existing` is the file's current content, if it exists yet.
/// - Every well-formed block already in `existing` for `component` (see
///   [`matched_blocks`]) is replaced by exactly one fresh block: the first
///   occurrence takes the new content, and any later occurrence is dropped
///   — so a duplicate never survives a merge. Everything else in the file,
///   including any unclosed opener for this component or any other
///   component's block, is copied through unchanged.
/// - When no well-formed block exists for `component` at all, a new one is
///   appended after whatever was already there.
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
    let pairs = matched_blocks(existing, &begin_marker, &end_marker);

    if pairs.is_empty() {
        // No well-formed block for this component anywhere — append.
        // `existing` (broken, or merely absent this component) is copied
        // forward untouched; only a separating newline (when the text does
        // not already end in one) and the new block are ever added after
        // it.
        let mut out = existing.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&block);
        return out;
    }

    // One or more well-formed blocks already exist. The first is replaced
    // with the fresh block; every later one is dropped entirely (see the
    // module doc comment's "collapses into one" section). Everything
    // outside every matched pair is copied through untouched, in order.
    let mut out = String::with_capacity(existing.len() + block.len());
    let mut cursor = 0;
    for (i, &(start, end)) in pairs.iter().enumerate() {
        out.push_str(&existing[cursor..start]);
        if i == 0 {
            out.push_str(&block);
        }
        cursor = end;
    }
    out.push_str(&existing[cursor..]);
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

    // ---- fix round 1: review findings ----

    /// Requirement 3's own failure mode, arriving one run later — item 1 of
    /// the review. Before the pairing fix, `merge_user_startup` matched the
    /// *first* opener to the *first* closer after it, so a stray unclosed
    /// `;BEGIN alpha` sitting before a later, genuine `alpha` block got
    /// paired with that later block's own `;END alpha` — collapsing
    /// everything between them, including the user's own untouched comment
    /// line, into one fresh block. Merged twice, per the review's own
    /// instruction: the stray text has to survive not just the run that
    /// fixes this but every run after it, since it never becomes anything
    /// other than an unclosed opener no matter how many times this
    /// component is merged.
    #[test]
    fn a_stray_opener_before_a_well_formed_block_of_the_same_component_is_never_swallowed() {
        let stray_prefix = ";BEGIN alpha\nold\n; the END line is missing\n";
        let broken_then_real = format!("{stray_prefix};BEGIN alpha\nfirst\n;END alpha\n");

        let once = merge_user_startup(Some(&broken_then_real), "alpha", &["second".into()]);
        assert!(
            once.starts_with(stray_prefix),
            "the stray opener and the user's broken comment must survive \
             the very run that fixes the well-formed block after them"
        );
        assert!(once.contains(";BEGIN alpha\nsecond\n;END alpha\n"));
        assert!(
            !once.contains("first"),
            "the well-formed block's own old content is replaced"
        );

        let twice = merge_user_startup(Some(&once), "alpha", &["third".into()]);
        assert!(
            twice.starts_with(stray_prefix),
            "and it must still be there on the run after that"
        );
        assert!(twice.contains(";BEGIN alpha\nthird\n;END alpha\n"));
        assert!(!twice.contains("second"));
    }

    /// Item 2 of the review: every existing replace test asserts with
    /// `contains` or `starts_with`, none of which can fail if the replace
    /// branch adds a stray extra newline before the tail — a blank line
    /// injected into the user's file on every single run. One exact
    /// `assert_eq!` on a full replace closes it.
    #[test]
    fn replacing_an_existing_block_produces_the_exact_expected_bytes() {
        let existing = "; before\n;BEGIN amissl\nold\n;END amissl\n; after\n";
        let out = merge_user_startup(Some(existing), "amissl", &["new".into()]);
        assert_eq!(out, "; before\n;BEGIN amissl\nnew\n;END amissl\n; after\n");
    }

    /// Item 4 of the review: Notepad writes CRLF, this tree lives on
    /// Windows, and the file is documented to be one the user hand-edits.
    /// Before the fix, `bytes[end] == b'\n'` never matched a marker line
    /// ending in `\r\n`, so a CRLF file's markers were invisible and every
    /// re-run appended another block rather than replacing the existing
    /// one.
    #[test]
    fn crlf_marker_lines_are_recognised_so_a_second_run_replaces_not_duplicates() {
        let existing = ";BEGIN amissl\r\nold\r\n;END amissl\r\n";
        let out = merge_user_startup(Some(existing), "amissl", &["new".into()]);
        assert_eq!(
            out.matches(";BEGIN amissl").count(),
            1,
            "the CRLF block must be replaced, not left behind alongside a \
             second, freshly-appended one"
        );
        assert!(out.contains("new"));
        assert!(!out.contains("old"));
    }

    /// The "also fold in" item: two already-well-formed blocks for one
    /// component — a leftover duplicate from some earlier bug, say — used
    /// to leave the second one behind untouched once the first was
    /// replaced. `matched_blocks` finds every complete pair, not just the
    /// first, and `merge_user_startup` drops every one past the first, so
    /// the duplicate collapses into a single merged block rather than
    /// surviving alongside it.
    #[test]
    fn two_well_formed_blocks_for_one_component_collapse_into_one_merged_block() {
        let duplicated = ";BEGIN alpha\none\n;END alpha\n;BEGIN alpha\ntwo\n;END alpha\n";
        let out = merge_user_startup(Some(duplicated), "alpha", &["three".into()]);
        assert_eq!(
            out.matches(";BEGIN alpha").count(),
            1,
            "no leftover duplicate block"
        );
        assert_eq!(out, ";BEGIN alpha\nthree\n;END alpha\n");
    }
}
