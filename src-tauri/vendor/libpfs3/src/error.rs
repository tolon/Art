//! Error types for libpfs3.
//!
//! Modified by ART on 2026-09-14 (ART-314): the `NameTooLong` variant, for a
//! name the volume cannot store and find again; (ART-319) the `CommitFailed`
//! variant, for a writer that has locked itself; on 2026-09-14, the final
//! review (M1): `CommitFailed`'s sentence covers both causes that reach it,
//! not only a failed commit.
//! Modified by ART on 2026-09-15 (ART-323): the `HardLinkNotWritten`,
//! `Pfs3aioLinkNotDeleted` and `HasHardLinks` variants, the writer's refusals
//! around hard links. `ART-PATCH.md` in this crate's root says what and why.

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

    /// ART-323: `Writer::create_hardlink` is refused. 0.1.3 wrote the
    /// linked object's anode into the link's own entry; pfs3aio's link has a
    /// link node of its own and sits in the object's chain of links
    /// (`CreateLink`, `directory.c:2672-2748`, `tonioni/pfs3aio` `211f7f0`).
    #[error(
        "hard link '{0}' was not created: this writer cannot write pfs3aio's hard-link format \
         (a link anode and the linked object's chain of links)"
    )]
    HardLinkNotWritten(String),

    /// ART-323: deleting a hard link pfs3aio made means taking its node out
    /// of the object's chain of links (`DeleteLink`, `directory.c:3835-3895`),
    /// which the writer does not do.
    #[error(
        "'{0}' is a hard link pfs3aio made, and this writer cannot take it out of its object's \
         chain of links, so it did not delete it — delete it on the Amiga"
    )]
    Pfs3aioLinkNotDeleted(String),

    /// ART-323: an object that hard links still name is not deleted. pfs3aio
    /// promotes a link to be the object instead (`RemapLinks`,
    /// `directory.c:3903-3965`); the writer does not. `links` names the link
    /// found, or the object's own chain of links when no link entry was.
    #[error(
        "'{name}' has hard links ({links}): this writer cannot hand it over to one of them as \
         pfs3aio does, so it did not delete it — delete the links first"
    )]
    HasHardLinks { name: String, links: String },

    /// ART-319: the writer has locked itself, and every later mutating call
    /// refuses immediately with this, before touching anything. Two causes
    /// reach it: a commit itself failed part-way through — the rootblock
    /// extension write, flushing the pending reserved-block writes, or the
    /// rootblock cluster write itself, or `set_volume_name`'s own direct
    /// write — where writes land in place, not copy-on-write, so the device
    /// may already hold part of this operation's metadata while the
    /// rootblock that would make it official does not; or an ordinary
    /// operation failed for an unrelated reason and `discard_to_last_commit`'s
    /// own reload could not even read the device back (M1, final review) —
    /// with nothing safe left to fall back to, that locks the writer too,
    /// even though nothing of this attempt was written. The sentence below
    /// is written to be true of both causes, rather than naming only the
    /// first (M1). Reopening means building a new `Volume` from the device
    /// (`Volume::from_device` / `open*`), **not** `Writer::open` on the
    /// `Volume` a poisoned writer's own `into_volume` hands back — that one
    /// still holds this failed attempt's in-memory state (M2, final review).
    #[error(
        "a write to this PFS3 volume failed and ART could not confirm what is on the disk: \
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
