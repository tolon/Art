//! Error types for libpfs3.
//!
//! Modified by ART on 2026-09-14 (ART-314): the `NameTooLong` variant, for a
//! name the volume cannot store and find again; (ART-319) the `CommitFailed`
//! variant, for a commit that failed part-way through. `ART-PATCH.md` in this
//! crate's root says what and why.

/// Result type alias using the PFS3 [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// PFS3 library error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}: data too short")]
    TooShort(&'static str),

    #[error("{0}: bad magic 0x{1:08x}")]
    BadMagic(&'static str, u32),

    #[error("{0}: expected block id 0x{1:04x}, got 0x{2:04x}")]
    BadBlockId(&'static str, u16, u16),

    #[error("block {0} out of range")]
    BlockOutOfRange(u64),

    #[error("anode {0} not found")]
    AnodeNotFound(u32),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    /// ART-314: a name longer than the volume can store and find again.
    #[error("name too long: '{name}' is {len} bytes, this volume stores at most {max}")]
    NameTooLong {
        name: String,
        len: usize,
        max: usize,
    },

    #[error("disk full: {0}")]
    DiskFull(String),

    /// ART-319: a commit failed part-way through — the rootblock extension
    /// write, flushing the pending reserved-block writes, or the rootblock
    /// cluster write itself. Writes land in place, not copy-on-write, so the
    /// device may already hold part of this operation's metadata while the
    /// rootblock that would make it official does not; the writer locks
    /// rather than reloading and continuing over that unknown state, and
    /// every later mutating call returns this immediately, before touching
    /// anything.
    #[error(
        "this PFS3 volume's last write failed part-way through and may be half-written: \
         reopen it (and check it) before writing to it again"
    )]
    CommitFailed,

    #[error("corrupt filesystem: {0}")]
    Corrupt(String),

    #[error("not a directory")]
    NotADirectory,

    #[error("directory not empty")]
    NotEmpty,

    #[error("invalid partition: {0}")]
    InvalidPartition(String),

    #[error("device is read-only")]
    ReadOnly,
}
