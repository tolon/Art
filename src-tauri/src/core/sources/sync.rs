//! Catalog sync: fetch a provider's index and replace what ART holds (§41.5.2).
//!
//! One sync makes the whole module work offline. Everything afterwards —
//! search, browse, version comparison — reads the local catalog, so a user can
//! sync somewhere with a connection and browse at home with none.
//!
//! ## A bad sync must not cost a good catalog
//!
//! The dangerous failure here is not a network error, which is obvious and
//! recoverable. It is a mirror that answers *successfully* with an error page,
//! a redirect notice, or an index in a format ART cannot read: the parse
//! produces three entries, the sync "succeeds", and ninety thousand packages
//! quietly vanish from the user's catalog.
//!
//! So the catalog is only replaced when the parse [looks
//! complete](super::index::SyncReport::looks_complete). Otherwise the existing
//! catalog stays and the user is told what arrived. §57's rule — never destroy
//! what is there on the strength of something that failed — applies to the
//! catalog as much as to an image.

use super::catalog::CatalogStore;
use super::fetch::LimitedWriter;
use super::index::{self, SyncReport};
use super::mirror::{fetch_with_failover, Mirror, MirrorClient};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;

/// The most index bytes ART will download in one sync.
///
/// Aminet's `INDEX` measured 7 229 355 bytes on 2026-08-09.
const MAX_INDEX_DOWNLOAD: u64 = 64 * 1024 * 1024;

/// A configured repository.
///
/// The index path is configuration rather than a constant because it is the
/// one thing that differs between providers, and because a mirror that moves
/// its index should be fixable in Settings rather than in a release.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider id, e.g. `"aminet"`.
    pub id: String,
    /// Repository-relative path of the machine-readable index.
    pub index_path: String,
    /// Mirrors in the order they should be tried.
    pub mirrors: Vec<Mirror>,
}

/// Aminet as ART ships it.
///
/// Verified against live mirrors on **2026-08-09**:
///
/// - the index is `INDEX` at the mirror root — 7 229 355 bytes, byte-identical
///   across all three, opening with a `|`-prefixed header;
/// - all three answer `Accept-Ranges: bytes` and return `206` to a range
///   request, so resume works;
/// - `INDEX.gz` exists too and is a quarter of the size, but ART has no
///   decompressor for it and shipping a guess would be the kind of untested
///   claim §89 forbids. A future change can add it behind the same config.
///
/// Several published mirrors are deliberately absent: `de`, `us3`, `au` and
/// `sg` failed TLS on the same day and `nl` did not answer on 443. Users can
/// add any of them in Settings — the list is configuration, not a constant.
pub fn aminet_defaults() -> CoreResult<ProviderConfig> {
    Ok(ProviderConfig {
        id: crate::core::sources::PROVIDER_AMINET.into(),
        index_path: "INDEX".into(),
        mirrors: vec![
            Mirror::new("Aminet", "https://aminet.net/")?,
            Mirror::new("Aminet (Sweden)", "https://se.aminet.net/")?,
            Mirror::new("FAU Erlangen (Germany)", "https://ftp.fau.de/aminet/")?,
        ],
    })
}

/// What a sync did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SyncOutcome {
    pub provider: String,
    /// The mirror that answered.
    pub mirror: String,
    pub index_bytes: u64,
    pub report: SyncReport,
    /// False when the parse looked too damaged to trust, in which case the
    /// previous catalog is still in place.
    pub applied: bool,
}

/// Fetch a provider's index and replace ART's catalog for it.
pub fn sync_catalog(
    provider: &ProviderConfig,
    client: &dyn MirrorClient,
    store: &dyn CatalogStore,
    sink: &dyn ProgressSink,
) -> CoreResult<SyncOutcome> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    sink.report(0, None, "Fetching the package index");

    let mut buffer = Vec::new();
    let attempt = {
        let mut writer = LimitedWriter::new(&mut buffer, MAX_INDEX_DOWNLOAD);
        fetch_with_failover(
            &provider.mirrors,
            client,
            &provider.index_path,
            0,
            &mut writer,
            sink,
        )?
    };

    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let index_bytes = buffer.len() as u64;
    sink.report(index_bytes, Some(index_bytes), "Reading the package index");

    let (entries, report) = index::parse_index_bytes(&buffer, &provider.id);

    // An index that parsed into almost nothing is a mirror problem, not a
    // catalog update. Keep what the user already has.
    let existing = store.stats()?.total;
    let applied = report.looks_complete() || existing == 0;

    if applied {
        sink.report(index_bytes, Some(index_bytes), "Storing the catalog");
        store.replace_provider(&provider.id, entries)?;
    }

    Ok(SyncOutcome {
        provider: provider.id.clone(),
        mirror: attempt.mirror,
        index_bytes,
        report,
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::sources::catalog::{tests::sample_catalog, MemoryCatalogStore, SourceQuery};
    use crate::core::sources::mirror::tests::MockMirror;
    use crate::core::sources::PROVIDER_AMINET;

    const INDEX: &str = "\
|
| Aminet index, created on 9-Aug-2026
|
AmiSSL-5.5.lha                 util/libs 1234K   4 Portable SSL/TLS library
Nettle.lha                     game/misc  100K  12 A small puzzle game
AmiTCP-SDK-4.3.lha             comm/tcp  1200K 600 SDK for AmiTCP
";

    fn provider() -> ProviderConfig {
        ProviderConfig {
            id: PROVIDER_AMINET.into(),
            index_path: "INDEX".into(),
            mirrors: vec![Mirror::new("Test", "https://m.invalid").unwrap()],
        }
    }

    fn client_with(body: &str) -> MockMirror {
        MockMirror::new().with_file("https://m.invalid/INDEX", body.as_bytes())
    }

    /// The shipped defaults must actually be usable: valid mirrors, and an
    /// index path that composes into the URL verified on 2026-08-09.
    #[test]
    fn the_shipped_aminet_defaults_compose_a_real_url() {
        let config = aminet_defaults().expect("the defaults must be valid");

        assert_eq!(config.id, PROVIDER_AMINET);
        assert_eq!(config.index_path, "INDEX");
        assert_eq!(config.mirrors.len(), 3, "one mirror is not failover");

        assert_eq!(
            config.mirrors[0].url_for(&config.index_path).unwrap(),
            "https://aminet.net/INDEX"
        );
        assert_eq!(
            config.mirrors[2]
                .url_for("util/libs/AmiSSL-5.5.lha")
                .unwrap(),
            "https://ftp.fau.de/aminet/util/libs/AmiSSL-5.5.lha"
        );

        // Every default must be HTTPS: these are plain-text mirrors otherwise.
        for mirror in &config.mirrors {
            assert!(
                mirror.base_url().starts_with("https://"),
                "{} is not HTTPS",
                mirror.name
            );
        }
    }

    #[test]
    fn a_sync_fills_the_catalog() {
        let store = MemoryCatalogStore::new();
        let outcome = sync_catalog(&provider(), &client_with(INDEX), &store, &NoProgress).unwrap();

        assert!(outcome.applied);
        assert_eq!(outcome.report.parsed, 3);
        assert_eq!(outcome.mirror, "Test");
        assert_eq!(store.stats().unwrap().total, 3);
    }

    /// §41.5.2: after one sync, search works with nothing connected.
    #[test]
    fn search_works_after_a_sync_with_no_further_requests() {
        let store = MemoryCatalogStore::new();
        let client = client_with(INDEX);
        sync_catalog(&provider(), &client, &store, &NoProgress).unwrap();

        let before = client.request_count();
        let hits = store.search(&SourceQuery::text("nettle")).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(client.request_count(), before, "search must not fetch");
    }

    /// The quiet disaster: a mirror answers with an error page, the parse
    /// yields nothing usable, and a good catalog is replaced by rubbish.
    #[test]
    fn an_unreadable_index_does_not_replace_a_good_catalog() {
        let store = MemoryCatalogStore::from_entries(sample_catalog());
        let junk = "<html><body>404 Not Found</body></html>\n".repeat(50);

        let outcome = sync_catalog(&provider(), &client_with(&junk), &store, &NoProgress).unwrap();

        assert!(!outcome.applied, "a junk index must not be applied");
        assert!(!outcome.report.looks_complete());
        assert_eq!(
            store.stats().unwrap().total,
            5,
            "the previous catalog must survive"
        );
    }

    /// With nothing to lose, even a poor parse is better than no catalog — but
    /// the report still says what happened.
    #[test]
    fn a_first_sync_applies_even_when_the_parse_is_poor() {
        let store = MemoryCatalogStore::new();
        let mixed = format!("{INDEX}{}", "garbage line\n".repeat(50));

        let outcome = sync_catalog(&provider(), &client_with(&mixed), &store, &NoProgress).unwrap();

        assert!(outcome.applied);
        assert!(!outcome.report.looks_complete());
        assert_eq!(store.stats().unwrap().total, 3);
    }

    #[test]
    fn a_resync_replaces_rather_than_accumulates() {
        let store = MemoryCatalogStore::new();
        sync_catalog(&provider(), &client_with(INDEX), &store, &NoProgress).unwrap();
        sync_catalog(&provider(), &client_with(INDEX), &store, &NoProgress).unwrap();

        assert_eq!(store.stats().unwrap().total, 3);
    }

    #[test]
    fn every_mirror_failing_leaves_the_catalog_untouched() {
        let store = MemoryCatalogStore::from_entries(sample_catalog());
        let client = MockMirror::new().failing("https://m.invalid/INDEX", "timed out");

        let err = sync_catalog(&provider(), &client, &store, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
        assert_eq!(store.stats().unwrap().total, 5);
    }

    #[test]
    fn an_endless_index_is_cut_off() {
        let store = MemoryCatalogStore::new();
        let huge = "y.lha util/x 1K 0 x\n".repeat(4 * 1024 * 1024);
        assert!(huge.len() as u64 > MAX_INDEX_DOWNLOAD);

        let err = sync_catalog(&provider(), &client_with(&huge), &store, &NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-MIRROR-UNREACHABLE");
        assert_eq!(store.stats().unwrap().total, 0);
    }

    #[test]
    fn cancelling_leaves_the_catalog_untouched() {
        struct Cancelled;
        impl ProgressSink for Cancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let store = MemoryCatalogStore::from_entries(sample_catalog());
        let client = client_with(INDEX);

        let err = sync_catalog(&provider(), &client, &store, &Cancelled).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED");
        assert_eq!(client.request_count(), 0);
        assert_eq!(store.stats().unwrap().total, 5);
    }
}
