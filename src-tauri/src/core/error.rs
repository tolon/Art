//! Core error type.
//!
//! Kept separate from the Tauri command error (`crate::error::AppError`) so the
//! core engine can be compiled and tested without Tauri.

use thiserror::Error;

/// Errors produced by the Amiga core engine.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path is not valid UTF-8")]
    NonUtf8Path,

    #[error("unsupported or unknown format: {0}")]
    UnsupportedFormat(String),

    #[error("malformed {format}: {detail}")]
    Malformed { format: String, detail: String },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A destructive operation was refused to protect the original file.
    #[error("operation refused to protect data: {0}")]
    SafetyRefused(String),

    #[error("not yet implemented: {0}")]
    NotImplemented(String),

    /// Every configured repository mirror failed. An online enhancement being
    /// unavailable is not a fault in the user's data (§60/§94) — ART keeps
    /// working, this operation does not.
    #[error("could not reach any configured mirror: {0}")]
    MirrorUnreachable(String),

    /// A download did not match what the repository catalog described. The
    /// file is discarded; nothing reaches the cache or an image.
    #[error("download does not match the catalog: {0}")]
    IntegrityMismatch(String),

    /// The user asked for the operation to stop. Not a fault — the UI should
    /// say "cancelled", not show an error.
    #[error("operation cancelled")]
    Cancelled,

    /// Cancelled, but some of the work is already durable on disk (ART-058).
    ///
    /// The whole-file strategy can be cancelled with nothing left behind: it
    /// works in a buffer and returns before the commit. The block-journal
    /// strategy — the one an image too large to hold in memory uses — cannot.
    /// Each file it copied is its own committed, journalled, verified
    /// operation, already durable in the file before the next one starts, and
    /// they are deliberately left in place rather than rolled back.
    ///
    /// Both used to come back as plain [`Cancelled`](Self::Cancelled), so
    /// somebody who stopped a large install part way through had no way to
    /// learn that some of it is on the volume. The count travels as a number
    /// rather than inside the sentence because the sentence is English and the
    /// UI's is not (§68).
    ///
    /// [`Cancelled`]: Self::Cancelled
    #[error("operation cancelled after writing {files} file(s)")]
    CancelledPartway { files: u64 },
}

impl CoreError {
    /// A short, stable identifier for this class of failure.
    ///
    /// Spec §68: an error the user can act on needs a name, not an opaque code
    /// like `0x80004005`. This is what appears at the end of a message and in
    /// the operation log, so a user can quote it and a maintainer can find it.
    ///
    /// These strings are part of ART's user-facing surface — treat them as
    /// stable once released.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "ART-IO",
            Self::NonUtf8Path => "ART-PATH-ENCODING",
            Self::UnsupportedFormat(_) => "ART-FORMAT-UNSUPPORTED",
            Self::Malformed { .. } => "ART-FORMAT-MALFORMED",
            Self::InvalidInput(_) => "ART-INPUT-INVALID",
            Self::SafetyRefused(_) => "ART-SAFETY-REFUSED",
            Self::NotImplemented(_) => "ART-NOT-IMPLEMENTED",
            Self::MirrorUnreachable(_) => "ART-MIRROR-UNREACHABLE",
            Self::IntegrityMismatch(_) => "ART-INTEGRITY-MISMATCH",
            Self::Cancelled => "ART-CANCELLED",
            Self::CancelledPartway { .. } => "ART-CANCELLED-PARTWAY",
        }
    }

    /// The message a user should see: what went wrong, then how to refer to it.
    ///
    /// Technical detail belongs in the log, not in front of the user (§68).
    pub fn user_message(&self) -> String {
        format!("{self}\n\nError ID: {}", self.code())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_distinct_code() {
        let errors = [
            CoreError::Io(std::io::Error::other("x")),
            CoreError::NonUtf8Path,
            CoreError::UnsupportedFormat("x".into()),
            CoreError::Malformed {
                format: "adf".into(),
                detail: "x".into(),
            },
            CoreError::InvalidInput("x".into()),
            CoreError::SafetyRefused("x".into()),
            CoreError::NotImplemented("x".into()),
            CoreError::MirrorUnreachable("x".into()),
            CoreError::IntegrityMismatch("x".into()),
            CoreError::Cancelled,
        ];

        let mut codes: Vec<&str> = errors.iter().map(|e| e.code()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "error codes must be distinct");
        assert!(codes.iter().all(|c| c.starts_with("ART-")));
    }

    #[test]
    fn user_message_carries_the_id() {
        let e = CoreError::SafetyRefused("the original was not modified".into());
        let msg = e.user_message();
        assert!(msg.contains("the original was not modified"));
        assert!(msg.contains("Error ID: ART-SAFETY-REFUSED"));
    }
}
