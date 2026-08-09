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

## Raw-device operations

Raw writes (SD cards for PiStorm, USB for Gotek preparation) are the most
dangerous operations in ART. They require:

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
HDF could not be resized.
The filesystem could not be safely verified.
The original image was not modified.
Error ID: HDF-RESIZE-017
```

over opaque codes like `0x80004005`. Technical details go to logs.
