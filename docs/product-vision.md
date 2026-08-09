# Product Vision

> **DROP IT INTO ART.**
> **NEVER MODIFY WHAT YOU CANNOT SAFELY VERIFY.**

## What ART is

ART is a **unified Amiga workflow platform**. It is not merely an ADF editor,
an HDF creator, an LHA extractor, a ROM manager, a WinUAE launcher, or a
Gotek utility. It combines all of these into one coherent application whose
central interaction is:

```
DROP → ANALYZE → UNDERSTAND → RECOMMEND → PREVIEW → BACKUP → APPLY → VERIFY
```

The user should not need to know *which* low-level Amiga utility a task
requires. ART understands the object and presents the appropriate workflows.

## Target users

ART serves three levels, progressively revealing complexity:

### Beginner
Wants to open, browse, extract, convert, launch, copy, organize. Technical
complexity stays hidden. Status is reported as Healthy / Warning / Problem /
Unknown rather than raw structures.

### Enthusiast
Wants ADF editing, HDF management, WHDLoad, Gotek, WinUAE, Kickstart,
collections, conversions, optimization, backups.

### Power User
Wants RDB, partitions, filesystem internals, boot blocks, block/sector
inspection, hex analysis, binary inspection, filesystem repair, image
comparison, PiStorm configuration. Power User Mode exposes this without
making it mandatory for everyone.

## Design philosophy

- **Drag everything.** Every major object supports intuitive drag-and-drop
  where technically possible. If it's technically impossible, provide an
  equally simple alternative.
- **No dead-end objects.** After recognition, ART always answers
  *"What can I do with this?"*
- **Explain before modify.** Before a destructive operation, show WHAT, WHY,
  what will change, what will be backed up, what will remain unchanged.
- **Originals are sacred.** Prefer Source → Copy → Modify → Verify → Commit.
  Never modify originals unnecessarily.
- **Offline first.** Core functionality works without internet. Online
  metadata is an enhancement, never a dependency.
- **Complexity belongs in the core.** The UI presents, navigates, and
  visualizes. The Rust core parses, validates, converts, hashes, analyzes,
  and recovers.

## What ART must never do

- Silently destroy data.
- Silently overwrite important files.
- Claim unsupported functionality.
- Freeze the UI.
- Hide dangerous operations.
- Present uncertain information as fact.
- Modify original images unnecessarily.
- Require internet for core functions.
- Distribute copyrighted ROMs or commercial software.

## The ultimate experience

```
USER drops something
  ▼
ART
  ├── What is it?
  ├── Is it valid?
  ├── What can I do with it?
  ├── What is the safest option?
  ├── What machine is it for?
  ├── What configuration is recommended?
  ├── What will change?
  ├── Can I back it up?
  ├── Can I verify it?
  └── Can I launch/use it?
```

The user should experience ART as *"An intelligent Amiga workbench for the
modern Windows PC."*
