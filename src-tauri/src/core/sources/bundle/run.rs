//! One set, downloaded in order.
//!
//! Sequential rather than parallel, and both halves of that are deliberate:
//! Aminet's mirrors are volunteer-run, and a readable report needs a
//! determinate order.

use std::path::PathBuf;

use super::resolve::{resolve, Resolution};
use super::BundleEntry;
use crate::core::jobs::ProgressSink;
use crate::core::lha::safe_extract::OverwritePolicy;
use crate::core::sources::cache::CacheLayout;
use crate::core::sources::fetch::fetch_package;
use crate::core::sources::library::Library;
use crate::core::sources::mirror::{Mirror, MirrorClient};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum EntryOutcome {
    Downloaded {
        bytes: u64,
        path: PathBuf,
    },
    AlreadyHave {
        path: PathBuf,
    },
    /// Fetched, but **not placed**: a file of that name is already in the
    /// library and ART does not overwrite what the user already has. Nobody
    /// established whether it is the same file — a name is not an identity
    /// — so this is neither `Downloaded` nor `AlreadyHave`.
    NotPlaced {
        existing: PathBuf,
    },
    /// ART will not fetch it, and said so before the run — a `user-supplied`
    /// entry, or a source with no configured mirror.
    Refused {
        why: String,
    },
    /// It was tried and the mirror or the network said no.
    Failed {
        error: String,
    },
    /// The user cancelled before reaching it. **Not** a failure: nothing was
    /// attempted.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntryReport {
    pub id: String,
    pub name: String,
    pub outcome: EntryOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleReport {
    pub entries: Vec<EntryReport>,
}

pub struct DownloadContext<'a> {
    pub aminet: &'a [Mirror],
    pub configured: &'a [(String, Mirror)],
    pub client: &'a dyn MirrorClient,
    pub cache: &'a CacheLayout,
    pub library: &'a Library,
    pub subfolder: &'a str,
}

pub fn download_entries(
    entries: &[BundleEntry],
    ctx: &DownloadContext<'_>,
    sink: &dyn ProgressSink,
) -> BundleReport {
    let mut reports = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        // Said before the check, so the screen can name what is being
        // considered — and so a test can make cancellation deterministic by
        // watching for a name rather than by counting bytes or sleeping.
        sink.report(index as u64, Some(entries.len() as u64), &entry.name);

        // Between whole entries, never mid-write. That is what makes
        // cancelling safe: unfinished work, never a half-written file.
        if sink.is_cancelled() {
            reports.push(EntryReport {
                id: entry.id.clone(),
                name: entry.name.clone(),
                outcome: EntryOutcome::Skipped,
            });
            continue;
        }

        let outcome = match resolve(entry, ctx.aminet, ctx.configured) {
            Resolution::Refused { why } => EntryOutcome::Refused { why },
            Resolution::Fetchable { meta, mirrors } => {
                match fetch_package(&meta, &mirrors, ctx.client, ctx.cache, sink) {
                    Ok(fetched) => {
                        match ctx.library.place(
                            &fetched.path,
                            ctx.subfolder,
                            &meta.name,
                            OverwritePolicy::Skip,
                        ) {
                            Ok(placement) if fetched.from_cache => EntryOutcome::AlreadyHave {
                                path: placement.path,
                            },
                            // The fetch itself was a real network transfer, but
                            // nothing was written at the destination: a file of
                            // that name was already there and ART does not
                            // overwrite it. Reporting `Downloaded` here would be
                            // exactly the "confident, wrong sentence" this
                            // project singles out — the bytes at `path` are
                            // whatever was already there, not what was just
                            // fetched, and are not necessarily even the same
                            // size as `fetched.bytes`.
                            Ok(placement) if placement.skipped_existing => {
                                EntryOutcome::NotPlaced {
                                    existing: placement.path,
                                }
                            }
                            Ok(placement) => EntryOutcome::Downloaded {
                                bytes: fetched.bytes,
                                path: placement.path,
                            },
                            Err(e) => EntryOutcome::Failed {
                                error: e.to_string(),
                            },
                        }
                    }
                    Err(e) => EntryOutcome::Failed {
                        error: e.to_string(),
                    },
                }
            }
        };

        reports.push(EntryReport {
            id: entry.id.clone(),
            name: entry.name.clone(),
            outcome,
        });
    }
    BundleReport { entries: reports }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::sources::bundle::{BundleEntry, EntrySource};
    use crate::core::sources::mirror::tests::MockMirror;
    use crate::core::ScratchDir;

    const BASE: &str = "https://mirror.invalid/";

    fn url(path: &str) -> String {
        format!("{BASE}{path}")
    }

    fn entry(id: &str, source: EntrySource) -> BundleEntry {
        BundleEntry {
            id: id.into(),
            name: id.into(),
            source,
            order: 1,
            exclusive_group: None,
            requires: Vec::new(),
            permission: None,
        }
    }

    fn aminet_entry(id: &str, path: &str) -> BundleEntry {
        entry(id, EntrySource::Aminet { path: path.into() })
    }

    /// Cancelled from the moment `name` is reported — which
    /// `download_entries` does at the top of the loop, **before** the
    /// cancellation check. So the entry that triggers it is the first one
    /// skipped, deterministically, with no sleeping and no byte counting.
    struct CancelOn {
        name: &'static str,
        hit: AtomicBool,
    }

    impl ProgressSink for CancelOn {
        fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
            if message == self.name {
                self.hit.store(true, Ordering::SeqCst);
            }
        }
        fn is_cancelled(&self) -> bool {
            self.hit.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn one_entry_failing_does_not_take_the_set_with_it() {
        let scratch = ScratchDir::new("art-bundle-run", "mixed");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .failing(&url("util/arc/lzx121r1"), "mirror said no");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };

        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            entry(
                "tolunnet",
                EntrySource::UserSupplied {
                    why: "the owner's own".into(),
                },
            ),
            aminet_entry("lzx", "util/arc/lzx121r1"),
        ];

        let report = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(
            report.entries[0].outcome,
            EntryOutcome::Downloaded { .. }
        ));
        assert!(matches!(
            report.entries[1].outcome,
            EntryOutcome::Refused { .. }
        ));
        assert!(matches!(
            report.entries[2].outcome,
            EntryOutcome::Failed { .. }
        ));
    }

    #[test]
    fn entries_are_attempted_in_the_order_they_are_given() {
        // `MockMirror` already records every request as (url, from) in its
        // own `requests` mutex — no new mock API is needed.
        let scratch = ScratchDir::new("art-bundle-run", "order");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .with_file(&url("util/arc/lzx121r1"), b"lzx bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };

        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            aminet_entry("lzx", "util/arc/lzx121r1"),
        ];
        let report = download_entries(&entries, &ctx, &NoProgress);
        assert_eq!(report.entries.len(), 2);

        let asked: Vec<String> = client
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|(u, _)| u.clone())
            .collect();
        assert_eq!(
            asked,
            vec![url("util/arc/lha_68k"), url("util/arc/lzx121r1")]
        );
    }

    #[test]
    fn a_second_run_over_the_same_set_reports_already_have_rather_than_downloading_again() {
        let scratch = ScratchDir::new("art-bundle-run", "twice");
        let client = MockMirror::new().with_file(&url("util/arc/lha_68k"), b"lha bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };
        let entries = vec![aminet_entry("lha", "util/arc/lha_68k")];

        let first = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(
            first.entries[0].outcome,
            EntryOutcome::Downloaded { .. }
        ));

        let second = download_entries(&entries, &ctx, &NoProgress);
        assert!(matches!(
            second.entries[0].outcome,
            EntryOutcome::AlreadyHave { .. }
        ));
    }

    /// ART-review Critical: a cold-cache download (the fetch is real, not a
    /// cache hit) over a library slot that already holds a file of the same
    /// name must not be reported as `Downloaded` — nothing was written
    /// there, and the pre-existing file is left byte-for-byte alone.
    #[test]
    fn a_cold_fetch_over_an_occupied_library_slot_is_not_placed_rather_than_downloaded() {
        let scratch = ScratchDir::new("art-bundle-run", "occupied");
        let client = MockMirror::new().with_file(&url("util/arc/lha_68k"), b"lha bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));

        // Something already sits at the destination the entry would resolve
        // to — a file the user put there themselves, under the same name
        // `resolve()` would give the fetched package.
        let existing_dir = scratch.join("library").join("sets");
        std::fs::create_dir_all(&existing_dir).unwrap();
        let existing_path = existing_dir.join("lha_68k");
        std::fs::write(&existing_path, b"the user's own file").unwrap();

        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };
        let entries = vec![aminet_entry("lha", "util/arc/lha_68k")];

        let report = download_entries(&entries, &ctx, &NoProgress);

        match &report.entries[0].outcome {
            EntryOutcome::NotPlaced { existing } => assert_eq!(existing, &existing_path),
            other => panic!("expected NotPlaced, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(&existing_path).unwrap(),
            b"the user's own file",
            "the pre-existing file must be left untouched"
        );
    }

    #[test]
    fn cancelling_stops_between_entries_and_marks_the_rest_skipped() {
        let scratch = ScratchDir::new("art-bundle-run", "cancel");
        let client = MockMirror::new()
            .with_file(&url("util/arc/lha_68k"), b"lha bytes")
            .with_file(&url("util/arc/lzx121r1"), b"lzx bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = CacheLayout::new(scratch.join("cache"));
        let library = Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "sets",
        };
        let entries = vec![
            aminet_entry("lha", "util/arc/lha_68k"),
            aminet_entry("lzx", "util/arc/lzx121r1"),
            aminet_entry("zip", "util/arc/ZIP232"),
        ];

        let sink = CancelOn {
            name: "lzx",
            hit: AtomicBool::new(false),
        };
        let report = download_entries(&entries, &ctx, &sink);

        assert!(matches!(
            report.entries[0].outcome,
            EntryOutcome::Downloaded { .. }
        ));
        // Skipped, **not** Failed: nothing was attempted for either.
        assert!(matches!(report.entries[1].outcome, EntryOutcome::Skipped));
        assert!(matches!(report.entries[2].outcome, EntryOutcome::Skipped));
    }
}
