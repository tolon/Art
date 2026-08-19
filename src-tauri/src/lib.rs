//! Amiga Retro Toolkit — Tauri shell.
//!
//! Wires up plugins (sql, log, store, dialog, fs), the Workflow Engine state,
//! and the command handlers. All business logic lives in [`crate::core`].

// The Amiga core deliberately exposes more API than the current UI consumes —
// helpers exist for modules still being wired up. Real mistakes (unused
// imports, unused variables, dead assignments) stay enabled so they surface.
#![allow(dead_code)]

mod commands;
mod core;
mod error;
mod net;
mod tools;

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_sql::{Migration, MigrationKind};

use crate::commands::jobs::JobRegistry;
use crate::commands::sources::SourcesState;
use crate::core::oplog::JsonlOperationLog;
use crate::core::workflow::{WorkflowEngine, WorkflowRegistry};

/// File name of the SQLite database (lives under the app's data dir).
const DB_URI: &str = "sqlite:art.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "create_initial_tables",
        sql: include_str!("../migrations/001_initial.sql"),
        kind: MigrationKind::Up,
    }];

    let engine = build_engine();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .level(log::LevelFilter::Info)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("art".to_string()),
                    }),
                ])
                .build(),
        )
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations(DB_URI, migrations)
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_drag::init())
        .manage(engine)
        .setup(|app| {
            log::info!(
                "Amiga Retro Toolkit {} starting on {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            );

            // The operation log lives beside the application log. Falling back
            // to the temp directory keeps ART usable on a machine where the app
            // data directory cannot be resolved — a missing history is a far
            // smaller problem than refusing to start.
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            app.manage(JsonlOperationLog::new(log_dir.join("operations.jsonl")));
            app.manage(std::sync::Arc::new(JobRegistry::new()));

            // The software catalog and download cache (§41.5). Both live
            // outside any disk image by design: a failed download must never
            // be able to touch one (§57). Same temp-directory fallback as the
            // operation log — a missing catalog is recoverable with one sync,
            // refusing to start is not.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            // Downloads default to the OS download folder so the first one
            // lands somewhere findable; Settings can point it anywhere.
            let downloads = app
                .path()
                .download_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("Amiga Retro Toolkit");
            app.manage(SourcesState::new(data_dir.join("sources"), downloads));

            // Files checked out for editing (§6). The cache directory, so the
            // OS knows they are scratch and a restart still finds the work.
            let cache_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            app.manage(crate::commands::checkout::CheckoutState_::new(
                cache_dir.join("checkout"),
            ));
            // Note: SQLite migrations run lazily on the first `Database::load`
            // call from the frontend. We do not touch the DB here to avoid
            // guessing at the plugin's async API surface.
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::ping,
            commands::system::app_info,
            commands::workflow::plan_path,
            commands::workflow::run_workflow,
            commands::workflow::list_workflows,
            commands::dragdrop::analyze_paths,
            commands::adf::adf_open,
            commands::adf::adf_list,
            commands::adf::adf_validate,
            commands::adf::adf_extract_file,
            commands::adf::adf_create_blank,
            commands::adf::adf_add_file,
            commands::adf::adf_create_directory,
            commands::adf::adf_delete_entry,
            commands::adf::adf_rename_entry,
            commands::lha::lha_open,
            commands::lha::lha_extract,
            commands::lha::lha_detect_whdload,
            commands::hdf::hdf_open,
            commands::hdf::hdf_create,
            commands::card::card_open,
            commands::card::card_plan_build,
            commands::card::card_build,
            commands::card::card_check_image,
            commands::card::card_intake,
            commands::preload::preload_probe,
            commands::preload::preload_plan,
            commands::preload::preload_run,
            commands::preload::preload_rom_pairing,
            commands::layout::layout_plan,
            commands::layout::layout_recheck,
            commands::layout::layout_apply,
            commands::osinstall::osinstall_scan_media,
            commands::osinstall::osinstall_destination_taken,
            commands::osinstall::osinstall_plan,
            commands::osinstall::osinstall_components,
            commands::osinstall::osinstall_apply,
            commands::osinstall::osinstall_verify,
            commands::gotek::gotek_scan,
            commands::gotek::gotek_save,
            commands::distro::distro_profiles,
            commands::distro::distro_check_card,
            commands::distro::distro_rom_family_matches,
            commands::distro::distro_measure_image,
            commands::pistorm::pistorm_scan,
            commands::pistorm::pistorm_preview,
            commands::pistorm::pistorm_save,
            commands::pistorm::pistorm_hardware_matrix,
            commands::pistorm::pistorm_hardware_notes,
            commands::pistorm::pistorm_profile,
            commands::pistorm::pistorm_tokens,
            commands::pistorm::pistorm_identify_rom,
            commands::pistorm::pistorm_rom_suits,
            commands::pistorm::pistorm_copy_rom,
            commands::pistorm::pistorm_preview_config_set,
            commands::pistorm::pistorm_write_config_set,
            commands::pistorm::pistorm_rename_config_set,
            commands::pistorm::pistorm_preview_activate_set,
            commands::pistorm::pistorm_activate_config_set,
            commands::pistorm::pistorm_delete_config_set,
            commands::pistorm::hex_read,
            commands::gameindex::gameindex_scan,
            commands::gameindex::catalogue_load,
            commands::gameindex::catalogue_add_root,
            commands::gameindex::catalogue_remove_root,
            commands::gameindex::catalogue_refresh,
            commands::gameindex::catalogue_set_override,
            commands::gameindex::name_suggestions,
            commands::gameindex::rename_title_file,
            commands::artwork::artwork_defaults,
            commands::artwork::artwork_check_source,
            commands::artwork::artwork_dir,
            commands::artwork::artwork_known,
            commands::artwork::artwork_for_title,
            commands::artwork::artwork_enrich,
            commands::artwork::artwork_adopt_local,
            commands::artwork::artwork_attach,
            commands::artwork::artwork_detach,
            commands::winuae::winuae_detect,
            commands::winuae::winuae_list_profiles,
            commands::winuae::winuae_launch,
            commands::winuae::rom_identify,
            commands::winuae::rom_scan_dir,
            commands::launch::launch_plan,
            commands::launch::launch_title,
            commands::oplog::oplog_recent,
            commands::oplog::oplog_export,
            commands::oplog::oplog_export_to,
            commands::oplog::oplog_path,
            commands::jobs::job_list,
            commands::jobs::job_cancel,
            commands::jobs::job_clear_finished,
            commands::sources::sources_stats,
            commands::sources::sources_search,
            commands::sources::sources_resolve,
            commands::sources::sources_provider,
            commands::sources::sources_set_mirrors,
            commands::sources::sources_reset_mirrors,
            commands::sources::sources_check_updates,
            commands::sources::sources_forget_download,
            commands::sources::sources_sync,
            commands::sources::sources_fetch,
            commands::sources::sources_readme,
            commands::sources::sources_library,
            commands::sources::sources_set_library_root,
            commands::sources::sources_relocate,
            commands::sources::sources_install_adf,
            commands::sources::sources_install_volume,
            commands::panel::panel_list_local,
            commands::panel::panel_local_roots,
            commands::volume::volume_scan,
            commands::volume::volume_list,
            commands::volume::volume_extract_to,
            commands::iso::iso_open,
            commands::iso::iso_list,
            commands::iso::iso_extract_file,
            commands::iso::iso_extract,
            commands::iso::iso_copy_to_volume,
            commands::archive::archive_open,
            commands::archive::archive_list,
            commands::archive::archive_extract_file,
            commands::archive::archive_extract,
            commands::archive::archive_copy_to_volume,
            commands::cbm::cbm_open,
            commands::cbm::cbm_list,
            commands::cbm::cbm_extract_file,
            commands::cbm::cbm_extract,
            commands::volume_write::volume_write_capability,
            commands::volume_write::volume_plan_copy,
            commands::volume_write::volume_plan_copy_many,
            commands::volume_write::volume_make_dir,
            commands::volume_write::volume_rename,
            commands::volume_write::volume_delete,
            commands::volume_write::volume_delete_many,
            commands::volume_write::volume_put_file,
            commands::volume_write::volume_copy_in,
            commands::volume_write::volume_copy_in_many,
            commands::volume_write::volume_copy_out,
            commands::volume_write::volume_copy_between,
            commands::volume_write::volume_recover,
            commands::volume_write::volume_attributes,
            commands::volume_write::volume_set_attributes,
            commands::volume_write::volume_read_head,
            commands::checkout::checkout_open,
            commands::checkout::checkout_list,
            commands::checkout::checkout_edit,
            commands::checkout::checkout_checkin,
            commands::checkout::checkout_discard,
            commands::checkout::volume_icon_for,
            commands::whdload::whdload_plan,
            commands::whdload::whdload_install,
            commands::archives::archives_plan_install,
            commands::archives::archives_install,
        ])
        .run(tauri::generate_context!())
        .map_err(|e| {
            eprintln!("Tauri startup error: {:?}", e);
            let _ = std::fs::write("crash.log", format!("Tauri startup error: {:?}", e));
            e
        })
        .expect("error while running tauri application");
}

/// Build the Workflow Engine with every registered workflow.
///
/// The catalogue itself lives in `core::workflow::builtin` — add new actions
/// there, not here, so `can_handle` routing and the tests that guard it stay
/// in one place.
fn build_engine() -> WorkflowEngine {
    let mut registry = WorkflowRegistry::new();
    crate::core::workflow::builtin::register_all(&mut registry);
    WorkflowEngine::new(registry)
}
