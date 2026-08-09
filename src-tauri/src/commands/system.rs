//! System / app-info commands: health check and metadata.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
}

/// Health-check command the frontend calls on startup to confirm the Rust
/// backend is alive.
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Amiga Retro Toolkit",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}
