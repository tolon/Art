# Security Model

ART treats **all external files as untrusted input**. Disk images, archives,
and ROMs may be malformed or hostile. This document defines the threats, the
operation classification, and the data-safety pipeline.

## Threats

| Threat | Mitigation |
|--------|-----------|
| Archive path traversal (`../../Windows/System32/...`) | Reject any entry whose normalized path escapes the destination root. |
| Malformed archives | Bound reads; reject implausible header values. |
| Malformed disk images | Validate geometry before parsing; never trust unchecked sizes. |
| Malicious filenames | Sanitize on extraction; never use raw entry names as paths. |
| Oversized allocations | Stream files; refuse to allocate based on a single unchecked length field. |
| Shell injection | Launch external tools only with validated, structured arguments — never a raw shell string. |
| Unsafe external process execution | Whitelist tool paths; validate arguments; never pass user input unsanitized. |
| Accidental raw-device writes | Never write to raw devices without explicit device selection + double confirmation. |

## Two safety modules, different threats

The two module names are easy to confuse and they defend opposite things.
**`core/security/` defends ART against hostile input; `core/safety/` defends the
user's existing data against ART itself.** Both are choke points — do not
bypass either.

### `core/security/` — hostile input

**`core/security/path.rs::safe_join()` is the only way to turn an archive entry
name into a destination path.** It normalizes `\` and `/`, rejects absolute
paths, `..`, `RootDir` and Windows prefixes, and then re-checks containment
against the destination root. Extraction, a `.uaem` sidecar, a checkout's temp
path — all of them go through it.

Three rules travel with it:

- **Bound every read.** Never allocate from an unchecked length field; use
  `checked_add` on running totals, so a header claiming four gigabytes fails a
  comparison rather than an allocation.
- **Never read a whole card or a whole large image to answer a small question.**
  `read_card` takes an 8 MB window per area; `open_hdf` reads a 1 MB window and
  takes the size from metadata. A gigabyte-scale file that has to fit in memory
  before it can be identified is a denial of service the user supplied
  themselves.
- **Launch external tools with structured argv, never a shell string** assembled
  from a file name — the editor a checkout opens included.

### `core/safety/` — the user's data

Every write goes through it:

- **`atomic_write(path, bytes)`** — temp file in the same directory ->
  `sync_all` -> rename. Never call `std::fs::write` on a user file directly; a
  truncated ADF or HDF is a destroyed one.
- **`guarded_write(path, bytes, policy)`** — backup, then atomic write. It
  returns the backup path, which commands surface to the UI
  (`MutationOutcome`, `GotekSaveOutcome`, ...) so the user is always told where
  the previous version went.
- **`BackupPolicy::{DISK_IMAGE, CONFIG, LARGE_IMAGE, NONE}`** — 3 generations
  for ADFs, 5 for config files, off for multi-gigabyte HDFs (that is the
  Snapshot Manager's job).

**Creating a file is `SAFE_CREATE`**: refuse when the target already exists
rather than replacing it.

## Destructive-operation classification

Every workflow carries a `Safety` tag (see `core/workflow/types.rs`). The UI
uses it to decide what confirmation is required.

| Level | Meaning | Confirmation |
|-------|---------|-------------|
| `ReadOnly` | No writes anywhere. | None. |
| `Safe` | Writes only to new/derivative files; originals untouched. | Single confirm. |
| `RequiresBackup` | Modifies the original after an automatic backup. | Confirm + show backup path. |
| `Destructive` | Destructive; cannot be undone. | Double confirmation. |
| `Experimental` | Unproven; may not be reliable. | Clearly flagged warning. |

## Data-safety pipeline

For any operation that modifies data, ART follows:

```
Original
   ↓
Backup (if required)
   ↓
Temporary copy
   ↓
Operation
   ↓
Validation
   ↓
Commit (or discard)
```

**Never destroy the original before successful validation.** If validation
fails, the original is preserved and the user is told.

```
Verification:
FAILED — Original preserved.
```

### The same pipeline at block granularity

An image over 16 MiB gets the same guarantee without a whole-file copy: the
undo journal (`core/volume/journal.rs`) is the "backup" step, scoped to the
blocks one operation touches.

```
Original blocks  → saved to <image>.artjournal and fsynced
                 → in-place writes into the image
                 → validation re-reads what landed
                 → valid:   delete the journal
                   invalid: restore from it, then delete it
```

### Journal trust rules

A journal is a file next to a user's disk image, and ART is about to write its
contents *into* that image. So it is treated as input, not as ART's own state:

- **Path and size must match** before a rollback. They are checked against the
  image on disk, and a mismatch is surfaced to the user with both numbers —
  never applied. Restoring blocks from an operation on a different file would
  be ART corrupting a disk entirely on its own initiative (§89).
- **The mtime is recorded and deliberately not compared.** It is the time from
  *before* the operation, and a crash mid-write is exactly the case where the
  file has changed since. Gating on it would reject every journal worth
  replaying.
- **Every entry carries a checksum.** A replay stops at the first entry that
  does not add up or does not complete, and applies only what came before it.
  A journal cut off mid-entry means the crash happened while the journal itself
  was still being written — at which point no image write had started, so the
  complete entries are bytes that are already there.
- **A block offset from a journal is bounds-checked** against the image's
  recorded size before the write, even though the size check above already
  passed. The journal's own numbers are untrusted input like any others.
- **A file ART cannot parse is an error, not a deletion.** ART does not remove
  a file it does not understand from next to a user's disk image, even one
  named like its own journal.
- **An image with a pending journal is read-only** until the user chooses to
  roll it back or discard it. Discarding is a separate, deliberate act: it
  leaves the image exactly as it is and removes only the record.

### Names from a disk are untrusted

Every name that reaches a path comes off an Amiga filesystem or out of an
archive, so it goes through `core/security/path.rs::safe_join` — extracting a
file, writing a `.uaem` sidecar, and choosing a checkout's temp path all do.
Names going the other way are escaped for NTFS deterministically
(`windows_safe_name`), including the DOS device names and trailing dots, which
are legal to create on Windows and impossible to open afterwards.

External programs are launched with structured argv, never a shell string
assembled from a file name — the editor a checkout opens included.

## Where ART may fetch from

`core/sources/mirror.rs` declares the `MirrorClient` trait **and every rule
about where ART may fetch from**. A request is always *constructed* from a
configured `Mirror` plus a validated repository path, so **there is no function
anywhere in ART that fetches a caller-supplied URL.**

That rule lives in `core/` on purpose. §41.5.7 promises "configured mirrors,
never arbitrary URLs", and the promise is worthless if the check lives in the AI
layer that is supposed to be constrained by it.

ART fetches index files and packages, never HTML, so nothing scrapes. The
transport that carries it out is `src-tauri/src/net/`, and nothing else in ART
may open a connection — see
[architecture.md § Where the network lives](architecture.md#where-the-network-lives).

## Raw-device operations

**Not implemented, and deliberately so** — see FEATURES.md's "Raw device
writes" row. ART builds a card *image* and leaves writing it to a device to
the card writers that already exist. This section is therefore the standing
requirement any future implementation has to meet, not a description of code:

1. Explicit device selection (user picks the device, not a path).
2. Full device information display (name, capacity, drive letter).
3. Clear warning that the device will be erased.
4. First confirmation.
5. Second, typed confirmation ("I UNDERSTAND") for destructive operations.

ART never auto-detects "the right device" and never writes silently.

## File associations

File associations (`.adf`, `.lha`, etc.) require **explicit user consent**.
ART never registers file associations without asking.

## Error reporting

Errors must be understandable, not raw codes. ART prefers:

```
The filesystem could not be safely verified.
The original image was not modified.
Error ID: ART-SAFETY-REFUSED
```

over opaque codes like `0x80004005`. Technical details go to logs.

**Where the identifier comes from.** `CoreError::code()` is the registry —
`ART-IO`, `ART-FORMAT-MALFORMED`, `ART-SAFETY-REFUSED`, `ART-PFS3-NON-ASCII-NAME`
and the rest. They are user-facing, so treat them as stable. They are a
*different* namespace from [ISSUES.md](ISSUES.md)'s `ART-NNN` defect ids, which
identify a defect in this project rather than a class of failure in a running
operation. The operation log records the `code()` one
(`commands/oplog.rs::write_result` → `record.failure(e.code(), …)`).
