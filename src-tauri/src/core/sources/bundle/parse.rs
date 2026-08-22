//! `serde` shapes for a shipped bundle file, and the gate every one passes.
//!
//! Deliberately separate from `mod.rs`'s public types: the wire shape may
//! grow a field the domain type folds away, the same split
//! `core/osinstall/package.rs` already draws between `RawPackage` and
//! `Package`.

use serde::Deserialize;

use super::{Bundle, BundleEntry, EntrySource, Permission};
use crate::core::error::{CoreError, CoreResult};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBundle {
    id: String,
    order: u32,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    name: String,
    /// Required. A missing `source` is a refusal, not a default: a default
    /// would have to invent where a package comes from.
    source: EntrySource,
    order: u32,
    #[serde(default)]
    exclusive_group: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    permission: Option<Permission>,
}

pub fn parse(json: &str) -> CoreResult<Bundle> {
    let raw: RawBundle = serde_json::from_str(json).map_err(|e| CoreError::Malformed {
        format: "bundle".into(),
        detail: e.to_string(),
    })?;
    let mut entries: Vec<BundleEntry> = raw
        .entries
        .into_iter()
        .map(|e| BundleEntry {
            id: e.id,
            name: e.name,
            source: e.source,
            order: e.order,
            exclusive_group: e.exclusive_group,
            requires: e.requires,
            permission: e.permission,
        })
        .collect();
    entries.sort_by_key(|e| e.order);
    let bundle = Bundle {
        id: raw.id,
        order: raw.order,
        entries,
    };
    validate(&bundle)?;
    Ok(bundle)
}

/// What a shipped file must get right: an id, and no two entries sharing one.
fn validate(bundle: &Bundle) -> CoreResult<()> {
    if bundle.id.trim().is_empty() {
        return Err(CoreError::Malformed {
            format: "bundle".into(),
            detail: "a bundle names no id".into(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &bundle.entries {
        if !seen.insert(entry.id.as_str()) {
            return Err(CoreError::Malformed {
                format: "bundle".into(),
                detail: format!("'{}': two entries share the id '{}'", bundle.id, entry.id),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// `arsiv.json`'s own entries happen to already sit in ascending `order`
    /// (review, Task 1 fix round 1), so a test that only reads the shipped
    /// file can't tell a real sort from a no-op — this fixture is written
    /// out of `order` on purpose, so only an actual sort produces the
    /// expected result.
    #[test]
    fn entries_come_back_sorted_by_order_even_when_the_file_lists_them_out_of_sequence() {
        let json = r#"{ "id": "x", "order": 1, "entries": [
            { "id": "third", "name": "Third", "order": 30, "source": { "aminet": { "path": "a" } } },
            { "id": "first", "name": "First", "order": 10, "source": { "aminet": { "path": "b" } } },
            { "id": "second", "name": "Second", "order": 20, "source": { "aminet": { "path": "c" } } }
        ] }"#;
        let bundle = super::parse(json).expect("well-formed, unordered entries still parse");
        let ids: Vec<&str> = bundle.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["first", "second", "third"]);
    }
}
