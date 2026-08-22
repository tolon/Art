//! Tauri command error type.
//!
//! `AppError` wraps [`crate::core::CoreError`] and serialises to a string so
//! the frontend receives a human-readable, structured message. We never leak
//! raw OS error codes; we render them through `Display` via `thiserror`.

use serde::Serialize;
use thiserror::Error;

use crate::core::CoreError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Core(#[from] CoreError),

    /// The scratch root the user chose cannot be staged into right now —
    /// unplugged, renamed, full, or read-only (ART-196).
    ///
    /// Its own variant rather than an `Other`, because it is the one failure
    /// in this enum a user can always fix themselves, and the sentence has to
    /// say how. Falling back to `std::env::temp_dir()` would be worse than
    /// refusing: somebody who has told ART to stay off their system drive
    /// would have it filled anyway, and nothing would say so.
    #[error(
        "ART stages its work in '{root}', and cannot right now: {why}.\n\nReconnect that \
         folder, or choose another one under Settings, Scratch folder. Nothing already in it \
         has been moved or removed."
    )]
    ScratchUnavailable {
        root: std::path::PathBuf,
        why: String,
    },

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Stable identifier for this class of failure (spec §68).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::ScratchUnavailable { .. } => "ART-SCRATCH-UNAVAILABLE",
            Self::Other(_) => "ART-INTERNAL",
        }
    }

    /// The text the user sees: what went wrong, then the ID to quote.
    pub fn user_message(&self) -> String {
        match self {
            Self::Core(e) => e.user_message(),
            // Every non-core variant renders through its own `#[error]`
            // attribute and then gains the id, so a new one cannot ship
            // without the line a user is meant to quote.
            other => format!("{other}\n\nError ID: {}", self.code()),
        }
    }
}

// Tauri commands must return `Result<T, E: Serialize>`. We serialise the error
// to its user-facing message, which carries the error ID — the frontend shows
// it verbatim, so a user can quote something more useful than "failed".
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.user_message())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Core(CoreError::Io(e))
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}
