//! Turning a title into a lookup key, and the two rules that use it.
//!
//! Matching is deliberately strict (spec §3). Online artwork is provenance rank
//! 2 — below the WHDLoad slave (3) and the user's own edit (4) — so it fills
//! gaps and never overwrites a stated fact. A silently accepted wrong guess
//! would make that ranking meaningless, so there is no edit distance here and
//! no token overlap: a title either matches by one of two written rules or it
//! has no artwork.

use std::collections::BTreeMap;

/// Articles dropped from the front of a title, longest first so `an ` is tested
/// before `a `.
const ARTICLES: [&str; 3] = ["the ", "an ", "a "];

/// What separates a title from its subtitle. Spaces on both sides are required:
/// `'Nam 1965-1975` is one name, not a title and a subtitle.
const SUBTITLE_SEPARATOR: &str = " - ";

/// Fold a title into its lookup key: lower case, single spaces, no leading
/// article.
pub fn normalise(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let mut folded = String::with_capacity(lowered.len());
    let mut last_was_space = true;
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                folded.push(' ');
            }
            last_was_space = true;
        } else {
            folded.push(ch);
            last_was_space = false;
        }
    }
    let trimmed = folded.trim_end();

    for article in ARTICLES {
        if let Some(rest) = trimmed.strip_prefix(article) {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

/// Find `title` in an index keyed by [`normalise`]d name.
///
/// Rule 1 is whole-title equality. Rule 2 — used only when rule 1 finds nothing
/// — is equality against the part before the first ` - `, which is what connects
/// `1869` to `1869 - Erlebte Geschichte Teil I`.
///
/// Rule 2 can match more than one candidate. `BTreeMap` iterates in sorted key
/// order and the first is taken, so the answer is the same on every run.
pub fn lookup<'a>(index: &'a BTreeMap<String, String>, title: &str) -> Option<&'a str> {
    let key = normalise(title);
    if key.is_empty() {
        return None;
    }

    if let Some(hit) = index.get(&key) {
        return Some(hit.as_str());
    }

    let prefix = format!("{key}{SUBTITLE_SEPARATOR}");
    index
        .range(prefix.clone()..)
        .take_while(|(candidate, _)| candidate.starts_with(&prefix))
        .map(|(_, path)| path.as_str())
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn index(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(name, path)| (normalise(name), (*path).to_string()))
            .collect()
    }

    #[test]
    fn normalisation_folds_case_and_collapses_space() {
        assert_eq!(normalise("  Turrican   II  "), "turrican ii");
    }

    #[test]
    fn normalisation_drops_a_leading_article() {
        assert_eq!(normalise("The Settlers"), "settlers");
        assert_eq!(normalise("A Prehistoric Tale"), "prehistoric tale");
    }

    /// "Theme Park" starts with "The" but the article is "The ", with a space.
    #[test]
    fn a_word_beginning_with_the_is_not_an_article() {
        assert_eq!(normalise("Theme Park"), "theme park");
    }

    #[test]
    fn rule_one_matches_the_whole_title() {
        let idx = index(&[("Turrican II", "Named_Boxarts/Turrican II.png")]);
        assert_eq!(
            lookup(&idx, "Turrican II"),
            Some("Named_Boxarts/Turrican II.png")
        );
    }

    /// The real case this rule exists for: the user's catalogue holds `1869`,
    /// libretro holds `1869 - Erlebte Geschichte Teil I`.
    #[test]
    fn rule_two_matches_the_head_before_a_dash() {
        let idx = index(&[(
            "1869 - Erlebte Geschichte Teil I",
            "Named_Boxarts/1869 - Erlebte Geschichte Teil I.png",
        )]);
        assert_eq!(
            lookup(&idx, "1869"),
            Some("Named_Boxarts/1869 - Erlebte Geschichte Teil I.png")
        );
    }

    /// Two releases of the same title, German and English. Both are legitimate;
    /// the choice must be the same on every run or two scans disagree.
    #[test]
    fn a_head_matching_two_candidates_picks_the_same_one_every_time() {
        let idx = index(&[
            ("1869 - History Experience Part I", "en.png"),
            ("1869 - Erlebte Geschichte Teil I", "de.png"),
        ]);
        let first = lookup(&idx, "1869").unwrap().to_string();
        for _ in 0..8 {
            assert_eq!(lookup(&idx, "1869").unwrap(), first);
        }
        // Sorted order, so the German title wins — stated so a change is visible.
        assert_eq!(first, "de.png");
    }

    /// Rule 1 must win outright. A title that exists whole is never resolved
    /// through the looser rule.
    #[test]
    fn whole_title_beats_head_match() {
        let idx = index(&[
            ("1869", "exact.png"),
            ("1869 - Erlebte Geschichte Teil I", "subtitled.png"),
        ]);
        assert_eq!(lookup(&idx, "1869"), Some("exact.png"));
    }

    /// The strictness the design chose: no edit distance, no token overlap.
    #[test]
    fn a_near_miss_does_not_match() {
        let idx = index(&[("Turrican II", "t2.png")]);
        assert_eq!(lookup(&idx, "Turrican"), None);
        assert_eq!(lookup(&idx, "Turricn II"), None);
    }

    #[test]
    fn an_empty_title_matches_nothing() {
        let idx = index(&[("Turrican II", "t2.png")]);
        assert_eq!(lookup(&idx, "   "), None);
    }

    /// A hyphen without spaces around it is part of the name, not a subtitle
    /// separator — `1000cc Turbo` and `'Nam 1965-1975` are both real entries.
    #[test]
    fn a_hyphen_without_spaces_does_not_split_a_title() {
        let idx = index(&[("'Nam 1965-1975", "nam.png")]);
        assert_eq!(lookup(&idx, "'Nam 1965"), None);
        assert_eq!(lookup(&idx, "'Nam 1965-1975"), Some("nam.png"));
    }
}
