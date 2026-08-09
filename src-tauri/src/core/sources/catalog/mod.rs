//! The local catalog: search, resolve, and where it is kept (§41.5.2).
//!
//! Everything the user does in Aminet Studio — search, browse, compare
//! versions — is served **from the local catalog**, never from a live query.
//! That is what makes the module retro-friendly in the way §41.5.2 asks for:
//! sync once at a friend's house, browse at home with the modem unplugged.
//!
//! ## Trait plus logic
//!
//! [`CatalogStore`] is deliberately dumb — it fetches and stores rows. The
//! judgement lives in this module as free functions ([`search_over`],
//! [`resolve`]) so every implementation ranks and resolves identically and one
//! set of tests covers all of them. `core/oplog` splits the same way.
//!
//! ## Never silently pick
//!
//! §41.5.2 is explicit: "latest stable version" is the newest catalog entry,
//! cross-checked against the readme `Version:` field, and **if they disagree,
//! show both**. [`Resolution`] therefore carries the disagreement rather than
//! hiding it, and [`resolve`] has no tie-breaking policy that would let ART
//! quietly prefer one story over the other.

pub mod jsonl;

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{PackageMeta, PackageRef};
use crate::core::error::CoreResult;

/// The most search terms honoured in one query.
const MAX_TERMS: usize = 8;

/// The longest single search term.
const MAX_TERM_BYTES: usize = 64;

/// The default number of hits returned when a query does not say.
pub const DEFAULT_SEARCH_LIMIT: usize = 100;

/// The most hits any single query can return.
const MAX_SEARCH_LIMIT: usize = 1000;

/// The most version segments compared before ART stops caring.
const MAX_VERSION_SEGMENTS: usize = 8;

/// The most file extensions a query may filter on.
const MAX_EXTENSIONS: usize = 12;

/// How results are ordered.
///
/// `Relevance` is the default because it is the only order that uses the search
/// text. The rest ignore it entirely: a user who asks for "largest" means
/// largest, and quietly re-ranking that by keyword match would be ART deciding
/// it knew better.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Relevance,
    /// Most recently uploaded first. Unknown ages sort last.
    Newest,
    Oldest,
    Largest,
    Smallest,
    /// By file name, case-insensitive.
    Name,
}

/// Narrowing beyond the search text (§41.5.6's browse and search surface).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFilters {
    /// Match the file name only, not the description.
    ///
    /// Aminet descriptions are one line of prose, so a common word matches
    /// hundreds of unrelated packages. This is the switch that makes a search
    /// for "amiga" usable.
    pub name_only: bool,
    /// Keep entries no older than this many weeks.
    ///
    /// Aminet's age column saturates at 999, so anything at the cap is excluded
    /// by any bound below it — which is correct: ART does not know how much
    /// older those are.
    pub max_age_weeks: Option<u32>,
    pub min_size_bytes: Option<u64>,
    pub max_size_bytes: Option<u64>,
    /// Lowercased extensions without the dot: `["lha", "lzh"]`. Empty means any.
    pub extensions: Vec<String>,
}

impl SearchFilters {
    /// Whether an entry survives the filters. The search text is handled
    /// separately, by scoring.
    fn keeps(&self, entry: &PackageMeta) -> bool {
        if let Some(max_age) = self.max_age_weeks {
            match entry.age_weeks {
                Some(age) if age <= max_age => {}
                // An entry with no age is not evidence of freshness, so an
                // age bound excludes it rather than letting it through.
                _ => return false,
            }
        }
        if let Some(min) = self.min_size_bytes {
            if entry.size_bytes < min {
                return false;
            }
        }
        if let Some(max) = self.max_size_bytes {
            if entry.size_bytes > max {
                return false;
            }
        }
        if !self.extensions.is_empty() {
            let extension = entry
                .name
                .rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase())
                .unwrap_or_default();
            if !self.extensions.iter().take(MAX_EXTENSIONS).any(|wanted| {
                wanted
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(&extension)
            }) {
                return false;
            }
        }
        true
    }
}

/// What the user is looking for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceQuery {
    /// Free text. Whitespace-separated terms must **all** match.
    pub text: String,
    /// Restrict to a directory and everything under it: `"util/libs"`.
    pub directory: Option<String>,
    /// 0 means [`DEFAULT_SEARCH_LIMIT`].
    pub limit: usize,
    pub sort: SortOrder,
    pub filters: SearchFilters,
}

impl SourceQuery {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn in_directory(mut self, directory: impl Into<String>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn sorted_by(mut self, sort: SortOrder) -> Self {
        self.sort = sort;
        self
    }

    pub fn filtered(mut self, filters: SearchFilters) -> Self {
        self.filters = filters;
        self
    }

    fn effective_limit(&self) -> usize {
        match self.limit {
            0 => DEFAULT_SEARCH_LIMIT,
            n => n.min(MAX_SEARCH_LIMIT),
        }
    }

    /// The lowercased terms this query actually searches for.
    ///
    /// Bounded in both count and length: a query is user input, and an
    /// unbounded term list turns a linear scan over ninety thousand rows into
    /// something quadratic.
    fn terms(&self) -> Vec<String> {
        self.text
            .split_whitespace()
            .take(MAX_TERMS)
            .map(|t| {
                let lower = t.to_lowercase();
                super::text::truncate_at_char_boundary(&lower, MAX_TERM_BYTES).to_string()
            })
            .filter(|t| !t.is_empty())
            .collect()
    }
}

/// How much the catalog holds. Shown in Settings and in Power User Mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogStats {
    pub total: usize,
    /// Per-provider counts, sorted by provider id.
    pub providers: Vec<(String, usize)>,
}

/// Where a synced catalog lives.
///
/// Implementations do storage; ranking and resolution stay in this module.
pub trait CatalogStore: Send + Sync {
    /// Replace everything ART holds for one provider.
    ///
    /// Wholesale replacement, not a merge: a package removed from Aminet must
    /// disappear from the catalog too, and a merge would leave ART offering
    /// downloads that no longer exist.
    fn replace_provider(&self, provider: &str, entries: Vec<PackageMeta>) -> CoreResult<()>;

    fn get(&self, reference: &PackageRef) -> CoreResult<Option<PackageMeta>>;

    /// Every entry directly inside one directory of one provider.
    fn in_directory(&self, provider: &str, directory: &str) -> CoreResult<Vec<PackageMeta>>;

    fn search(&self, query: &SourceQuery) -> CoreResult<Vec<PackageMeta>>;

    fn stats(&self) -> CoreResult<CatalogStats>;
}

/// Rank `entries` against `query`, best first.
///
/// The shared implementation every [`CatalogStore`] should call, so results do
/// not depend on which store happens to be configured.
pub fn search_over<'a>(
    entries: impl Iterator<Item = &'a PackageMeta>,
    query: &SourceQuery,
) -> Vec<PackageMeta> {
    let terms = query.terms();
    let directory = query.directory.as_deref().map(str::to_lowercase);

    let mut hits: Vec<(u32, &PackageMeta)> = Vec::new();

    for entry in entries {
        if let Some(ref dir) = directory {
            if !directory_contains(dir, &entry.directory) {
                continue;
            }
        }
        if !query.filters.keeps(entry) {
            continue;
        }
        match score(entry, &terms, query.filters.name_only) {
            Some(points) => hits.push((points, entry)),
            None => continue,
        }
    }

    // Every order ends on the repository path, so a query run twice returns the
    // same list — a table that reshuffles its ties on every keystroke is worse
    // than one that is merely imperfectly ranked.
    hits.sort_by(|a, b| {
        let tie = |x: &(u32, &PackageMeta), y: &(u32, &PackageMeta)| {
            x.1.reference.path.cmp(&y.1.reference.path)
        };
        match query.sort {
            SortOrder::Relevance => {
                b.0.cmp(&a.0)
                    // Newer first. An unknown age sorts last rather than first: not
                    // knowing when something was uploaded is not evidence it is fresh.
                    .then_with(|| age_rank(a.1).cmp(&age_rank(b.1)))
                    .then_with(|| tie(a, b))
            }
            SortOrder::Newest => age_rank(a.1).cmp(&age_rank(b.1)).then_with(|| tie(a, b)),
            // Oldest-first still puts unknown ages last: they are unknown, not
            // ancient.
            SortOrder::Oldest => match (a.1.age_weeks, b.1.age_weeks) {
                (Some(x), Some(y)) => y.cmp(&x).then_with(|| tie(a, b)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => tie(a, b),
            },
            SortOrder::Largest => b.1.size_bytes.cmp(&a.1.size_bytes).then_with(|| tie(a, b)),
            SortOrder::Smallest => a.1.size_bytes.cmp(&b.1.size_bytes).then_with(|| tie(a, b)),
            SortOrder::Name => {
                a.1.name
                    .to_lowercase()
                    .cmp(&b.1.name.to_lowercase())
                    .then_with(|| tie(a, b))
            }
        }
    });

    hits.into_iter()
        .take(query.effective_limit())
        .map(|(_, entry)| entry.clone())
        .collect()
}

/// Whether `entry_dir` is `dir` or lives under it.
fn directory_contains(dir: &str, entry_dir: &str) -> bool {
    let entry = entry_dir.to_lowercase();
    let dir = dir.trim_end_matches('/');
    entry == dir || entry.starts_with(&format!("{dir}/"))
}

fn age_rank(entry: &PackageMeta) -> u64 {
    entry.age_weeks.map_or(u64::MAX, u64::from)
}

/// Score one entry, or `None` when a term does not match at all.
///
/// Every term must match somewhere — an AND search. Searching "amissl os4"
/// and being shown every package mentioning OS4 would make the box useless.
fn score(entry: &PackageMeta, terms: &[String], name_only: bool) -> Option<u32> {
    if terms.is_empty() {
        return Some(0);
    }

    let name = entry.name.to_lowercase();
    let path = entry.reference.path.to_lowercase();
    let short = entry.short.to_lowercase();
    let stem = package_stem(&entry.name);

    let mut total = 0u32;
    for term in terms {
        let mut points = 0u32;
        if stem == *term {
            points = points.max(100);
        }
        if name.starts_with(term) {
            points = points.max(50);
        }
        if name.contains(term) {
            points = points.max(30);
        }
        if !name_only {
            if short.contains(term) {
                points = points.max(15);
            }
            if path.contains(term) {
                points = points.max(5);
            }
        }

        if points == 0 {
            return None;
        }
        total = total.saturating_add(points);
    }

    Some(total)
}

/// The package name with its extension and version suffix removed, lowercased.
///
/// `AmiSSL-5.5.lha` → `amissl`, `AmiTCP-SDK-4.3.lha` → `amitcp-sdk`. This is
/// how the different versions of one package are recognised as siblings:
/// Aminet has no package identity beyond the file name.
pub fn package_stem(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _ext)| stem);
    let bytes = stem.as_bytes();

    for i in 1..bytes.len() {
        let is_separator = matches!(bytes[i - 1], b'-' | b'_');
        let starts_version = bytes[i].is_ascii_digit()
            || (matches!(bytes[i], b'v' | b'V')
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit));
        if is_separator && starts_version {
            return stem[..i - 1].to_lowercase();
        }
    }

    stem.to_lowercase()
}

/// The catalog and the readme telling different stories about which release is
/// newest. Surfaced, never resolved (§41.5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionDisagreement {
    /// The entry the catalog says is newest.
    pub newest: PackageRef,
    /// Its version, when one is known.
    pub newest_version: Option<String>,
    /// The entry carrying the highest version string.
    pub highest: PackageRef,
    pub highest_version: String,
}

/// The answer to "which one should I install?", with its caveats intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub best: PackageMeta,
    /// Present when the newest catalog entry is not the one with the highest
    /// version. The UI must show both; ART does not choose.
    pub disagreement: Option<VersionDisagreement>,
    /// Other releases of the same package, newest first.
    pub alternatives: Vec<PackageMeta>,
}

/// Pick the release of `stem` to offer, from entries in one directory.
///
/// Newest by catalog age wins, because that is the only ordering Aminet itself
/// asserts. Where a readme disagrees, the disagreement travels with the answer.
pub fn resolve(candidates: &[PackageMeta], stem: &str) -> Option<Resolution> {
    let stem = stem.to_lowercase();
    let mut matching: Vec<&PackageMeta> = candidates
        .iter()
        .filter(|entry| package_stem(&entry.name) == stem)
        .collect();

    if matching.is_empty() {
        return None;
    }

    matching.sort_by(|a, b| {
        age_rank(a)
            .cmp(&age_rank(b))
            .then_with(|| a.reference.path.cmp(&b.reference.path))
    });

    let best = matching[0];

    // The highest version string among the candidates, when versions are known.
    let highest = matching
        .iter()
        .filter(|entry| entry.version.is_some())
        .max_by(|a, b| {
            let (a_version, b_version) = (
                a.version.as_ref().map(|c| c.value.as_str()).unwrap_or(""),
                b.version.as_ref().map(|c| c.value.as_str()).unwrap_or(""),
            );
            compare_versions(a_version, b_version)
        })
        .copied();

    let disagreement = highest.and_then(|highest| {
        if highest.reference == best.reference {
            return None;
        }
        let highest_version = highest.version.as_ref()?.value.clone();
        let best_version = best.version.as_ref().map(|c| c.value.clone());

        // Only a disagreement if the *older* entry claims the higher version.
        let higher = match best_version {
            Some(ref best_value) => {
                compare_versions(&highest_version, best_value) == Ordering::Greater
            }
            None => true,
        };

        higher.then(|| VersionDisagreement {
            newest: best.reference.clone(),
            newest_version: best_version,
            highest: highest.reference.clone(),
            highest_version,
        })
    });

    Some(Resolution {
        best: best.clone(),
        disagreement,
        alternatives: matching[1..].iter().map(|e| (*e).clone()).collect(),
    })
}

/// Compare two version strings the way a human reads them: `1.10` is newer
/// than `1.9`, and `2.0` is newer than `1.99`.
///
/// Deliberately not a semver parser. Amiga version strings are `5.5`, `4.3`,
/// `1.2b`, `2.0beta3` — a strict parser would reject most of them, and
/// rejecting is worse than ordering approximately and saying so: the result
/// only ever feeds a `Low`/`Medium`-confidence claim.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let mut left = version_segments(a);
    let mut right = version_segments(b);

    let len = left.len().max(right.len());
    left.resize(len, VersionSegment::Number(0));
    right.resize(len, VersionSegment::Number(0));

    for (l, r) in left.iter().zip(right.iter()) {
        match (l, r) {
            (VersionSegment::Number(l), VersionSegment::Number(r)) => match l.cmp(r) {
                Ordering::Equal => continue,
                other => return other,
            },
            // A plain number outranks a qualified one: 2.0 is newer than
            // 2.0beta.
            (VersionSegment::Number(_), VersionSegment::Text(_)) => return Ordering::Greater,
            (VersionSegment::Text(_), VersionSegment::Number(_)) => return Ordering::Less,
            (VersionSegment::Text(l), VersionSegment::Text(r)) => match l.cmp(r) {
                Ordering::Equal => continue,
                other => return other,
            },
        }
    }

    Ordering::Equal
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionSegment {
    Number(u64),
    Text(String),
}

fn version_segments(version: &str) -> Vec<VersionSegment> {
    version
        .split(['.', '-', '_'])
        .take(MAX_VERSION_SEGMENTS)
        .map(|segment| match segment.parse::<u64>() {
            Ok(n) => VersionSegment::Number(n),
            Err(_) => VersionSegment::Text(segment.to_lowercase()),
        })
        .collect()
}

/// A catalog held entirely in memory.
///
/// Used by tests, and by [`jsonl::JsonlCatalogStore`] as its index — a linear scan
/// over ninety thousand rows is single-digit milliseconds, which is well
/// inside what a search box needs.
#[derive(Debug, Default)]
pub struct MemoryCatalogStore {
    entries: std::sync::RwLock<Vec<PackageMeta>>,
}

impl MemoryCatalogStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_entries(entries: Vec<PackageMeta>) -> Self {
        Self {
            entries: std::sync::RwLock::new(entries),
        }
    }

    /// Every entry, cloned. For persistence, not for querying.
    pub fn snapshot(&self) -> Vec<PackageMeta> {
        self.entries.read().expect("catalog lock").clone()
    }
}

impl CatalogStore for MemoryCatalogStore {
    fn replace_provider(&self, provider: &str, entries: Vec<PackageMeta>) -> CoreResult<()> {
        let mut held = self.entries.write().expect("catalog lock");
        held.retain(|entry| entry.reference.provider != provider);
        held.extend(entries);
        Ok(())
    }

    fn get(&self, reference: &PackageRef) -> CoreResult<Option<PackageMeta>> {
        let held = self.entries.read().expect("catalog lock");
        Ok(held.iter().find(|e| &e.reference == reference).cloned())
    }

    fn in_directory(&self, provider: &str, directory: &str) -> CoreResult<Vec<PackageMeta>> {
        let held = self.entries.read().expect("catalog lock");
        Ok(held
            .iter()
            .filter(|e| e.reference.provider == provider && e.directory == directory)
            .cloned()
            .collect())
    }

    fn search(&self, query: &SourceQuery) -> CoreResult<Vec<PackageMeta>> {
        let held = self.entries.read().expect("catalog lock");
        Ok(search_over(held.iter(), query))
    }

    fn stats(&self) -> CoreResult<CatalogStats> {
        let held = self.entries.read().expect("catalog lock");
        let mut providers: Vec<(String, usize)> = Vec::new();

        for entry in held.iter() {
            match providers
                .iter_mut()
                .find(|(name, _)| name == &entry.reference.provider)
            {
                Some((_, count)) => *count += 1,
                None => providers.push((entry.reference.provider.clone(), 1)),
            }
        }
        providers.sort();

        Ok(CatalogStats {
            total: held.len(),
            providers,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::sources::{index, Claim, PROVIDER_AMINET};

    /// A small catalog built from index text, so the tests exercise the real
    /// parser rather than hand-built structs.
    pub(crate) fn sample_catalog() -> Vec<PackageMeta> {
        let (entries, report) = index::parse_index(
            "\
AmiSSL-4.12.lha                util/libs  900K 180 Portable SSL/TLS library
AmiSSL-5.5.lha                 util/libs 1234K   4 Portable SSL/TLS library
MUI38usr.lha                   util/libs 1100K 900 MagicUserInterface for users
AmiTCP-SDK-4.3.lha             comm/tcp  1200K 600 SDK for AmiTCP
Nettle.lha                     game/misc  100K  12 A small puzzle game
",
            PROVIDER_AMINET,
        );
        assert_eq!(report.skipped, 0);
        entries
    }

    fn store() -> MemoryCatalogStore {
        MemoryCatalogStore::from_entries(sample_catalog())
    }

    // ---- search ----

    #[test]
    fn a_name_match_outranks_a_description_match() {
        let hits = store().search(&SourceQuery::text("nettle")).unwrap();
        assert_eq!(hits[0].name, "Nettle.lha");
    }

    #[test]
    fn search_is_case_insensitive() {
        let hits = store().search(&SourceQuery::text("AMISSL")).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn the_newest_release_of_a_package_is_listed_first() {
        let hits = store().search(&SourceQuery::text("amissl")).unwrap();
        assert_eq!(hits[0].name, "AmiSSL-5.5.lha", "4 weeks beats 180");
    }

    /// Searching two words must narrow, not widen.
    #[test]
    fn every_term_has_to_match() {
        let store = store();
        assert_eq!(
            store
                .search(&SourceQuery::text("amissl ssl"))
                .unwrap()
                .len(),
            2
        );
        assert!(store
            .search(&SourceQuery::text("amissl nettle"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_directory_filter_includes_subdirectories_and_excludes_the_rest() {
        let store = store();
        assert_eq!(
            store
                .search(&SourceQuery::text("").in_directory("util"))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            store
                .search(&SourceQuery::text("").in_directory("game/misc"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn results_are_bounded() {
        let many: Vec<PackageMeta> = (0..5000)
            .map(|i| {
                let (entries, _) = index::parse_index(
                    &format!("Thing{i}.lha util/x 1K 0 thing\n"),
                    PROVIDER_AMINET,
                );
                entries.into_iter().next().unwrap()
            })
            .collect();
        let store = MemoryCatalogStore::from_entries(many);

        assert_eq!(
            store.search(&SourceQuery::text("thing")).unwrap().len(),
            DEFAULT_SEARCH_LIMIT
        );
        assert_eq!(
            store
                .search(&SourceQuery::text("thing").limit(usize::MAX))
                .unwrap()
                .len(),
            MAX_SEARCH_LIMIT
        );
    }

    /// A query is user input; an unbounded term list would turn every search
    /// into a scan per word.
    #[test]
    fn a_pathological_query_is_bounded_not_rejected() {
        let query = SourceQuery::text("a ".repeat(10_000));
        assert_eq!(query.terms().len(), MAX_TERMS);
        assert!(store().search(&query).is_ok());
    }

    // ---- sorting ----

    fn names(hits: &[PackageMeta]) -> Vec<&str> {
        hits.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn sorting_by_size_ignores_relevance() {
        let hits = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Largest))
            .unwrap();
        assert_eq!(
            names(&hits),
            vec![
                "AmiSSL-5.5.lha",     // 1234K
                "AmiTCP-SDK-4.3.lha", // 1200K
                "MUI38usr.lha",       // 1100K
                "AmiSSL-4.12.lha",    // 900K
                "Nettle.lha",         // 100K
            ]
        );

        let smallest = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Smallest))
            .unwrap();
        assert_eq!(smallest[0].name, "Nettle.lha");
    }

    #[test]
    fn sorting_by_date_runs_both_ways() {
        let newest = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Newest))
            .unwrap();
        assert_eq!(newest[0].name, "AmiSSL-5.5.lha", "4 weeks");
        assert_eq!(newest[1].name, "Nettle.lha", "12 weeks");

        let oldest = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Oldest))
            .unwrap();
        assert_eq!(oldest[0].name, "MUI38usr.lha", "900 weeks");
    }

    /// An entry with no age is unknown, not ancient. It belongs at the end of
    /// *both* date orders, because ART cannot place it in either.
    #[test]
    fn an_unknown_age_sorts_last_whichever_way_the_dates_run() {
        let mut entries = sample_catalog();
        entries[0].age_weeks = None;
        let undated = entries[0].name.clone();
        let store = MemoryCatalogStore::from_entries(entries);

        for order in [SortOrder::Newest, SortOrder::Oldest] {
            let hits = store
                .search(&SourceQuery::text("").sorted_by(order))
                .unwrap();
            assert_eq!(
                hits.last().unwrap().name,
                undated,
                "{order:?} put an unknown age somewhere other than last"
            );
        }
    }

    #[test]
    fn sorting_by_name_is_case_insensitive() {
        let hits = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Name))
            .unwrap();
        assert_eq!(hits[0].name, "AmiSSL-4.12.lha");
        assert_eq!(hits.last().unwrap().name, "Nettle.lha");
    }

    /// A list that reshuffles its ties between two identical queries is worse
    /// than one that is merely imperfectly ranked.
    #[test]
    fn every_order_is_stable_across_identical_queries() {
        for order in [
            SortOrder::Relevance,
            SortOrder::Newest,
            SortOrder::Oldest,
            SortOrder::Largest,
            SortOrder::Smallest,
            SortOrder::Name,
        ] {
            let query = SourceQuery::text("").sorted_by(order);
            let first = store().search(&query).unwrap();
            let second = store().search(&query).unwrap();
            assert_eq!(names(&first), names(&second), "{order:?} is not stable");
        }
    }

    /// The limit has to apply *after* sorting, or "the 3 largest" would be
    /// "3 arbitrary ones, sorted".
    #[test]
    fn a_limited_search_keeps_the_extremes_not_the_first_matches() {
        let hits = store()
            .search(&SourceQuery::text("").sorted_by(SortOrder::Largest).limit(2))
            .unwrap();
        assert_eq!(names(&hits), vec!["AmiSSL-5.5.lha", "AmiTCP-SDK-4.3.lha"]);
    }

    // ---- filters ----

    /// Aminet descriptions are a line of prose, so a common word matches
    /// hundreds of unrelated packages. This is what makes such a search usable.
    #[test]
    fn name_only_search_ignores_the_description() {
        let store = store();
        let anywhere = store.search(&SourceQuery::text("ssl")).unwrap();
        assert_eq!(anywhere.len(), 2, "matches the AmiSSL descriptions too");

        let name_only = store
            .search(&SourceQuery::text("nettle").filtered(SearchFilters {
                name_only: true,
                ..SearchFilters::default()
            }))
            .unwrap();
        assert_eq!(names(&name_only), vec!["Nettle.lha"]);

        // "puzzle" appears only in a description, so name-only finds nothing.
        let none = store
            .search(&SourceQuery::text("puzzle").filtered(SearchFilters {
                name_only: true,
                ..SearchFilters::default()
            }))
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn an_age_bound_keeps_only_what_is_new_enough() {
        let hits = store()
            .search(&SourceQuery::text("").filtered(SearchFilters {
                max_age_weeks: Some(20),
                ..SearchFilters::default()
            }))
            .unwrap();
        assert_eq!(names(&hits), vec!["AmiSSL-5.5.lha", "Nettle.lha"]);
    }

    /// Aminet's age saturates at 999, so a capped entry is "this old or older".
    /// Any bound below the cap must exclude it — ART does not know how much
    /// older it really is.
    #[test]
    fn an_age_bound_excludes_capped_and_unknown_ages() {
        let mut entries = sample_catalog();
        entries[0].age_weeks = Some(crate::core::sources::index::AGE_CAP_WEEKS);
        entries[1].age_weeks = None;
        let store = MemoryCatalogStore::from_entries(entries);

        let hits = store
            .search(&SourceQuery::text("").filtered(SearchFilters {
                max_age_weeks: Some(998),
                ..SearchFilters::default()
            }))
            .unwrap();

        assert!(!hits.iter().any(|h| h.age_weeks.is_none()));
        assert!(!hits
            .iter()
            .any(|h| h.age_weeks == Some(crate::core::sources::index::AGE_CAP_WEEKS)));
    }

    #[test]
    fn a_size_range_is_inclusive_at_both_ends() {
        let hits = store()
            .search(&SourceQuery::text("").filtered(SearchFilters {
                min_size_bytes: Some(900 * 1024),
                max_size_bytes: Some(1200 * 1024),
                ..SearchFilters::default()
            }))
            .unwrap();

        let mut found = names(&hits);
        found.sort_unstable();
        assert_eq!(
            found,
            vec!["AmiSSL-4.12.lha", "AmiTCP-SDK-4.3.lha", "MUI38usr.lha"]
        );
    }

    #[test]
    fn an_extension_filter_accepts_a_leading_dot_and_any_case() {
        let mut entries = sample_catalog();
        entries[4].name = "Nettle.adf".into();
        let store = MemoryCatalogStore::from_entries(entries);

        for wanted in ["adf", ".adf", "ADF"] {
            let hits = store
                .search(&SourceQuery::text("").filtered(SearchFilters {
                    extensions: vec![wanted.into()],
                    ..SearchFilters::default()
                }))
                .unwrap();
            assert_eq!(names(&hits), vec!["Nettle.adf"], "failed on {wanted:?}");
        }
    }

    #[test]
    fn filters_combine_with_the_search_text() {
        let hits = store()
            .search(&SourceQuery::text("amissl").filtered(SearchFilters {
                max_age_weeks: Some(20),
                ..SearchFilters::default()
            }))
            .unwrap();
        assert_eq!(names(&hits), vec!["AmiSSL-5.5.lha"]);
    }

    #[test]
    fn no_filters_keeps_everything() {
        let hits = store().search(&SourceQuery::text("")).unwrap();
        assert_eq!(hits.len(), 5);
    }

    // ---- package identity ----

    #[test]
    fn a_package_stem_drops_the_version_suffix() {
        assert_eq!(package_stem("AmiSSL-5.5.lha"), "amissl");
        assert_eq!(package_stem("AmiTCP-SDK-4.3.lha"), "amitcp-sdk");
        assert_eq!(package_stem("Thing-v2.0.lha"), "thing");
        assert_eq!(package_stem("Nettle.lha"), "nettle");
        // No separator before the digits: part of the name, not a version.
        assert_eq!(package_stem("MUI38usr.lha"), "mui38usr");
    }

    // ---- resolution ----

    #[test]
    fn resolution_offers_the_newest_and_keeps_the_rest() {
        let candidates = store().in_directory(PROVIDER_AMINET, "util/libs").unwrap();
        let resolved = resolve(&candidates, "amissl").expect("a resolution");

        assert_eq!(resolved.best.name, "AmiSSL-5.5.lha");
        assert_eq!(resolved.alternatives.len(), 1);
        assert_eq!(resolved.alternatives[0].name, "AmiSSL-4.12.lha");
        assert!(resolved.disagreement.is_none());
    }

    #[test]
    fn an_unknown_package_resolves_to_nothing() {
        let candidates = store().in_directory(PROVIDER_AMINET, "util/libs").unwrap();
        assert!(resolve(&candidates, "nosuchthing").is_none());
    }

    /// §41.5.2: when the catalog order and the readme version disagree, both
    /// are shown. ART must not quietly prefer either.
    #[test]
    fn a_disagreement_is_surfaced_rather_than_resolved() {
        let mut candidates = store().in_directory(PROVIDER_AMINET, "util/libs").unwrap();

        // The newest file by age claims an older version than its predecessor —
        // a re-upload of an old release, which really happens on Aminet.
        for entry in &mut candidates {
            let version = match entry.name.as_str() {
                "AmiSSL-5.5.lha" => "3.0",
                "AmiSSL-4.12.lha" => "4.12",
                _ => continue,
            };
            entry.version = Some(Claim::from_readme(version.to_string()));
        }

        let resolved = resolve(&candidates, "amissl").expect("a resolution");
        let disagreement = resolved.disagreement.expect("a disagreement");

        assert_eq!(resolved.best.name, "AmiSSL-5.5.lha", "catalog order wins");
        assert_eq!(disagreement.newest_version.as_deref(), Some("3.0"));
        assert_eq!(disagreement.highest_version, "4.12");
        assert!(disagreement.highest.path.ends_with("AmiSSL-4.12.lha"));
    }

    #[test]
    fn agreeing_versions_produce_no_disagreement() {
        let mut candidates = store().in_directory(PROVIDER_AMINET, "util/libs").unwrap();
        for entry in &mut candidates {
            let version = match entry.name.as_str() {
                "AmiSSL-5.5.lha" => "5.5",
                "AmiSSL-4.12.lha" => "4.12",
                _ => continue,
            };
            entry.version = Some(Claim::from_readme(version.to_string()));
        }

        let resolved = resolve(&candidates, "amissl").unwrap();
        assert!(resolved.disagreement.is_none());
    }

    // ---- version comparison ----

    #[test]
    fn versions_compare_the_way_a_human_reads_them() {
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("2.0", "1.99"), Ordering::Greater);
        assert_eq!(compare_versions("5.5", "5.5"), Ordering::Equal);
        assert_eq!(compare_versions("4.12", "5.5"), Ordering::Less);
        // A trailing zero segment is not a newer release.
        assert_eq!(compare_versions("1.2.0", "1.2"), Ordering::Equal);
    }

    #[test]
    fn a_qualified_version_is_older_than_the_plain_one() {
        assert_eq!(compare_versions("2.0", "2.0beta"), Ordering::Greater);
        assert_eq!(compare_versions("2.0-rc1", "2.0"), Ordering::Less);
    }

    /// Amiga version strings are not semver. Refusing to order them would be
    /// worse than ordering them approximately.
    #[test]
    fn unparseable_versions_still_order_deterministically() {
        assert_eq!(compare_versions("weird", "weird"), Ordering::Equal);
        assert_ne!(compare_versions("alpha", "beta"), Ordering::Greater);
    }

    // ---- store behaviour ----

    /// A package pulled from Aminet must disappear from the catalog, so ART
    /// never offers a download that no longer exists.
    #[test]
    fn syncing_replaces_a_provider_wholesale() {
        let store = store();
        let before = store.stats().unwrap().total;
        assert_eq!(before, 5);

        let (fresh, _) = index::parse_index("Only.lha util/x 1K 0 only\n", PROVIDER_AMINET);
        store.replace_provider(PROVIDER_AMINET, fresh).unwrap();

        assert_eq!(store.stats().unwrap().total, 1);
        assert!(store
            .search(&SourceQuery::text("amissl"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn syncing_one_provider_leaves_another_alone() {
        let store = store();
        let (other, _) = index::parse_index("Other.lha x/y 1K 0 other\n", "os4depot");
        store.replace_provider("os4depot", other).unwrap();

        assert_eq!(store.stats().unwrap().total, 6);
        store.replace_provider(PROVIDER_AMINET, Vec::new()).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.providers, vec![("os4depot".to_string(), 1)]);
    }

    #[test]
    fn get_finds_an_entry_by_reference() {
        let store = store();
        let reference = PackageRef::new(PROVIDER_AMINET, "game/misc/Nettle.lha");
        assert_eq!(store.get(&reference).unwrap().unwrap().name, "Nettle.lha");

        let missing = PackageRef::new(PROVIDER_AMINET, "game/misc/Nope.lha");
        assert!(store.get(&missing).unwrap().is_none());
    }
}
