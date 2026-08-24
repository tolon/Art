//! Seeding a card's network — the thin adapter.
//!
//! The owner's decision, 2026-08-24: *"ART sorsun, kart kurarken WiFi
//! bilgilerini girelim."* The judgement is all in `core/amiganet`; this
//! deserialises, calls it, and logs what happened.
//!
//! # The passphrase reaches exactly one place
//!
//! It arrives inside a `Secret`, which cannot be serialised — so it cannot
//! reach the frontend, a manifest or an AI prompt through `serde`. What is
//! left for this file to get right is the **operation log**, and the way it is
//! got right is that the record is built from `Seeded`, which counts the
//! networks and never names one. No `expose()` call appears in this file.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::amiganet::seed::{networks_already_there, seed_tree, Seed, Seeded};
use crate::core::amiganet::{tolunnet, wpa};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::{AppError, AppResult};

/// What the screen sends. **No `Serialize`**: it carries passphrases.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedRequest {
    pub tree: String,
    #[serde(default)]
    pub networks: Vec<wpa::Profile>,
    #[serde(default)]
    pub tolunnet: Option<tolunnet::Config>,
}

/// How many networks a rewrite would replace — asked **before** the button.
///
/// Its own command for the reason `osinstall_destination_taken` is one: a
/// surprise that arrives after somebody has committed reads as the application
/// doing something it never warned about. Reads one file and writes nothing.
#[tauri::command]
pub fn amiganet_networks_already_there(tree: String) -> AppResult<Option<usize>> {
    Ok(networks_already_there(std::path::Path::new(&tree)))
}

/// Put the two files into a system volume.
#[tauri::command]
pub fn amiganet_seed(
    request: SeedRequest,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<Seeded> {
    let tree = PathBuf::from(&request.tree);
    let result = seed_tree(
        &tree,
        &Seed {
            networks: request.networks,
            tolunnet: request.tolunnet,
        },
    )
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Set up the card's network").destination(&request.tree),
        &result,
        |record, done: &Seeded| {
            // **Counts and filenames, never an SSID and never a passphrase.**
            // An SSID is the name of somebody's home, and this file is kept.
            let record = record
                .detail("Files written", done.written.join(", "))
                .detail("Networks", done.networks.to_string());
            let record = match done.replaced_networks {
                Some(had) => record.detail("Networks replaced", had.to_string()),
                None => record,
            };
            record
                .detail(
                    "Stack config",
                    if done.tolunnet_merged {
                        "edited in place"
                    } else {
                        "created"
                    },
                )
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}
