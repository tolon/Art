# Cataloguing a WHDLoad drawer, and telling iGame what ART knows

*2026-09-05. Closes work-list item 5 (SD-2 G10 wave 2, "where `igame.data`
goes"). The work list posed this as a choice between two routes; **both of them
turned out to be about the wrong material**, and §1 is that measurement.*

---

## 1. The work list's question was wrong, and the measurement is why

The entry offered two routes for `igame.data`: into the user's own hardfiles
beside the slave, or into a distribution's Games drawer. It also recorded that
"beside each slave" cannot be done for this collection, because the owner's
2 815 catalogued titles are 1 697 `whdload-hardfile`, 1 016 `floppies`, 102
`hardfile` and **zero** drawers — the slaves are inside the hardfiles.

Four things were measured before this design, and the first one closes route 1
outright:

**iGame cannot see a self-booting hardfile at all.** Read out of
`MrZammler/iGame`'s own `funcs.c`: `scan_repositories()` calls `examineFolder()`,
which walks the Amiga's filesystem for `.slave` files
(`strcasestr(FIblock->fib_FileName, ".slave")`) and reads `igame.data` from the
directory the slave is in. A `.hdf` that boots itself is invisible to it — the
slave is inside the image, not on the volume iGame is walking. So writing
`igame.data` into those 1 697 images would put a file where **nothing ever
reads it**. `igame.rs` already measured that 148 of them have under a kilobyte
free and framed that as the route's cost; the real cost is that the route has
no reader, not that 148 are full.

**The owner does have material of exactly the shape iGame wants.**
`E:\amiga\Amigatolon\paketler\WHDLoadDemos100.lha`, 663 506 299 bytes, 8 858
entries: **893 `.slave` files in 893 distinct drawers**, one slave per drawer,
every one at the same depth (`Demos\<bucket>\<title>\<name>.slave`), and **zero
`igame.data` already present**. `E:\amiga\Amigatolon\igame\` holds iGame 2.6.1,
so this is a launcher the owner actually uses.

**A drawer's real contents, read out of the archive rather than assumed** —
three drawers, chosen from the start, middle and end of the set:

| Drawer | What is in it |
|---|---|
| `Demos\0-9\1001StolenIdeas` | `1001StolenIdeas` (273 888), `.info` (12 503), `.slave` (1 000), `ReadMe`, `ReadMe.info` |
| `Demos\M\Megademo2Disorder` | `Disk.1` (901 120 — a DD floppy dump), `.info`, `.Slave`, `ReadMe`, `ReadMe.info` |
| `Demos\T\Tag` | `data\01` … `data\82` (11 files), `.info`, `.Slave`, `ReadMe`, `ReadMe.info` |

So a drawer is **a slave, an icon, a ReadMe, and a payload** — and the payload
is a bare file, a disk image, or a `data/` subdirectory. The slave's extension
is `.slave` or `.Slave`, so the comparison is case-insensitive, as iGame's own
`strcasestr` is. And iGame's `examineFolder` explicitly skips directories named
`data`/`Data`, which the `Tag` row explains: they are payload, not games.

**There are two icons per title, not one, and they do different jobs.**
Counted over the whole set: **all 893 drawers have a sibling `.info` beside
them** — `Demos\0-9\1001StolenIdeas.info` next to
`Demos\0-9\1001StolenIdeas\` — which is the **drawer's own icon**, the thing
that makes it visible on Workbench at all. `core::whdload`'s module doc already
records why that matters: an install that copies the drawer and leaves its
icon behind produces a drawer that exists and is invisible, "which is
indistinguishable from the install having failed". The second icon is *inside*
the drawer and is the **Project icon** that launches the game — that is the one
§2 parses, and the one carrying `SLAVE=`. A design that says "the drawer's
icon" without saying which would be wrong about both.

**The launch path already exists and was deliberately left in place.**
[ART-147](../../ISSUES.md#fixed) says so in as many words:
`RequestKind::Whdload` — an unpacked drawer paired with a separate bootable
system — "stays in place… no `Media` variant reaches it today because nothing
in `core::gameindex` catalogues a loose drawer or an `.lha` archive as a title,
**not because the shape is imaginary**". This design is that missing producer.

## 2. How WHDLoad actually starts an install

Read from whdload.de's own manual (`docs/en/opt.html`, `docs/en/install.html`)
and then verified against a real icon, because the manual and the material can
disagree and only one of them ships.

- **From a shell**, the slave is the first positional argument:
  `WHDLoad SuperGame.Slave Preload NTSC QuitKey=69 Custom1=1`.
- **From Workbench**, options are ToolTypes and the slave is named by the
  `Slave` ToolType, whose default is `WHDLoad.Slave`. `Slave=*` makes WHDLoad
  search the current directory for `#?.slave`.
- **WHDLoad itself belongs in `C:`** so that it and its tools resolve without a
  path — which is exactly why a drawer needs a separate bootable system volume
  and a self-booting hardfile does not.
- `Data`, `SavePath` and `SaveDir` name where a title reads and writes.

**Verified on the owner's own material** — the icon *inside* the drawer,
`Demos\0-9\1001StolenIdeas\1001StolenIdeas.info`, extracted from the archive
and parsed byte by byte:

- it is a **Project** icon (`do_Type` 4) whose `DefaultTool` is `WHDLoad`, so a
  double-click runs WHDLoad with the icon's ToolTypes;
- its first ToolTypes are `SLAVE=1001StolenIdeas.Slave` and `PRELOAD` — the
  manual's `Slave` in upper case, which is why the key comparison must be
  case-insensitive;
- and then `*** DON'T EDIT THE FOLLOWING LINES!! ***` followed by **94
  ToolTypes of `IM1=` and `IM2=` data**, out of 98 in total, up to 127
  characters each.

**Those 94 lines are the icon's picture.** This is a NewIcon, which stores its
image inside the ToolType array. Two rules follow, and the second one is a
landmine this design is deliberately walking around:

1. **Reading a drawer's launch configuration means ignoring everything from
   the `DON'T EDIT` marker onwards, and any `IM1=`/`IM2=` key.** Otherwise the
   "configuration" is mostly image data.
2. **ART never writes a drawer icon's ToolTypes.** `core::amigaicon::merge_tooltypes`,
   built in the layered-release round, replaces the ToolType array wholesale —
   correct for `Tools/IconEdit.info`, which is a classic icon, and destructive
   for a NewIcon, where it would delete the artwork. Nothing in this design
   writes an icon. `igame.data` is a separate file, which is the whole reason
   iGame uses one.

## 3. Two shapes, two types, two endings

A drawer title can live in two places and they are **not** the same thing:

```rust
/// An unpacked WHDLoad drawer on the host: a directory holding one slave.
/// Launches through `RequestKind::Whdload` — drawer plus a separate bootable
/// system volume with WHDLoad in `C:`.
WhdloadDrawer { dir: String, slave: String },

/// A WHDLoad drawer inside an archive ART has not unpacked. Catalogued so the
/// collection can be browsed; **cannot be launched** until it is unpacked.
WhdloadArchive { file: String, inner: String, slave: String },
```

**Two variants rather than one with a location field, and the reason is
[ART-147](../../ISSUES.md#fixed).** That defect folded two physical shapes into
one record: `from_hardfile` recorded every self-booting hardfile as
`Media::WhdloadDrawer`, which sent Play down the drawer path — ask for a system
volume, mount the game as a plain directory, write a boot directory. The owner
picked a Workbench 3.0 image with no WHDLoad on it, landed at a CLI, and
reasonably concluded they had to install WHDLoad. They did not; the file boots
itself. A single variant carrying `location: Dir | InArchive` reproduces that
shape exactly — one missed `match` arm and an archived title silently takes the
launchable path.

So Play's answer for `WhdloadArchive` is **its own ending**: this title is
inside an archive, unpack it first, and here is where. Never a refusal that
reads like a broken install, and never a silent fall-through.

## 4. The scanner learns that a directory can be a title

`scan::collect_indexable` is extension-driven today —
`rp9 | hdf | img | adf | adz` — and the whole pipeline below it assumes one
file is one title. It gains a second question, and the two stay separate:

- **A directory is a title when it holds exactly one `#?.slave`**, compared
  case-insensitively. `data`/`Data` subdirectories are not descended into, for
  the reason iGame does not descend into them either.
- **A directory holding more than one slave is reported, not guessed.** The
  measurement says this does not occur in the owner's 893, so the case is a
  refusal naming the drawer rather than a rule invented for material nobody
  has seen.
- **An archive is walked by its headers.** LhA headers are sequential and each
  carries its packed size, so the scan seeks header to header and decompresses
  only the `.slave` members — 893 small reads out of a 663 MB file, not 663 MB
  of decompression.

`read_one` keeps its file-shaped contract; the directory and archive scans are
their own functions with their own returns, so nothing has to guess which kind
of thing it is holding.

## 5. What names a drawer title

Three sources, and this project's standing rule decides their order: a slave
header and an icon **state**; a directory name only **suggests**.

1. **The slave header** (`readers::slave`, already built) — the title's own
   name, its chipset and its declared Kickstart.
2. **The icon's ToolTypes** — `SLAVE=` says which slave is the real one, which
   settles a drawer that holds more than one; `PRELOAD`, `NTSC` and the rest
   say how this install expects to start. Parsed through `core::amigaicon`,
   with §2's rule 1 applied: stop at `DON'T EDIT`, drop `IM1=`/`IM2=`.
3. **The drawer name**, as a suggestion, when neither of the above states one —
   `1001StolenIdeas` is a name a person can read, and it is still a guess.

## 6. Writing `igame.data`

Two destinations. **The copy is the default and needs no ceremony; the user's
own collection is an explicit, previewed, backed-up action.**

- **Onto a copy ART made** — when ART lays WHDLoad titles onto a card or into a
  distribution it is building, `igame.data` goes beside each slave in that
  copy. Nothing of the user's is touched, so there is nothing to preview. The
  laying-out itself is built: `core::whdload` already works out what inside an
  archive is the pack and what its icon is called, for exactly this path, and
  it already carries the sibling drawer icon that keeps the result visible on
  Workbench. This design adds one file beside the slave; it does not re-answer
  where the pack goes.
- **Into the user's own unpacked collection** — the full §92 pipeline:
  SOURCE → ANALYZE → VALIDATE → RECOMMEND → PREVIEW → BACKUP → APPLY → VERIFY →
  REPORT, reported **per entry**. 893 drawers is 893 results, not one number.
- **Into an archive, never.** ART's archive readers are read-only behind
  `core/archive`'s single security gate. A request against a `WhdloadArchive`
  title is refused with a sentence that names the archive and says to unpack
  it — actionable, which is the rule a refusal has to meet.

The writing itself is built: `igame::merge_into` **edits an existing file
rather than regenerating it**, passing comments, ordering and unknown keys
through, for the same reason `FF.CFG` and `cmdline.txt` are edited in place —
somebody may have curated theirs by hand. The owner's archive ships **zero**
`igame.data`, so for that material every write is a create; the merge path is
what protects a collection that is not theirs.

ART writes `title`, `chipset`, `genre`, `year` and `players`. It does **not**
write `exe`: iGame rejects an `exe` containing `.slave` (`strcasestr`), and a
WHDLoad drawer is launched through its slave, so the key stays unwritten rather
than written and ignored. A value that will not fit iGame's 64-byte line is
**left out and named**, never truncated — a truncated title is a wrong title on
the Amiga's screen.

## 7. The schema, and ART-147's second half

Two new variants mean `GAMEINDEX_SCHEMA` moves up one, so every existing
catalogue is re-read rather than deserialised against a shape that has changed.
This is not housekeeping: ART-147's fix shipped without it and the Collection
screen came up showing `ART-FORMAT-MALFORMED: unknown variant 'whdload-drawer'`
**instead of any title at all**.

The name `WhdloadDrawer` returns, and an old catalogue may hold that same
string meaning the old, wrong thing. The schema bump discards those records
before anything reads them, so the collision is harmless — and a test says so
rather than leaving the next reader to work it out.

**Producer discipline is the other half of the lesson**, and it is a test, not
a comment: `WhdloadDrawer` is produced **only** by the directory scan,
`WhdloadArchive` **only** by the archive scan, and `from_hardfile` produces
neither. ART-147 happened because a hardfile reader assumed a drawer.

## 8. Testing

Fixtures are synthetic and built at runtime — ART ships no copyrighted Amiga
content. A synthetic drawer is a directory with a hand-built slave header
(`readers::slave::tests_support::build_slave` already exists), a hand-built
NewIcon-shaped `.info`, a `ReadMe`, and a `data/` subdirectory.

| Guard | The mutation that must fell it |
|---|---|
| A directory with one slave is a title | make the scan extension-only again |
| `data`/`Data` is not descended into | descend into it; the payload must become titles |
| A drawer with two slaves is refused by name | pick the first; the refusal must vanish |
| `SLAVE=` settles which slave is real | ignore the ToolTypes; a two-slave drawer must stop resolving |
| `IM1=`/`IM2=` and everything after `DON'T EDIT` are dropped | keep them; the parsed configuration must fill with image data |
| An archived title cannot be launched | route it to `RequestKind::Whdload`; the "unpack it first" ending must disappear |
| `from_hardfile` produces neither new variant | produce `WhdloadDrawer` from it; the producer test must fail |
| The schema bump re-reads old catalogues | leave the schema alone; an old record must survive into the new shape |
| A write into the user's own collection previews and backs up first | skip the backup; the guard must fail |
| An `igame.data` request against an archived title is refused | let it through; the refusal must vanish |

**The real bar** is an `#[ignore]`d hook against the owner's own material:
point it at an unpacked `WHDLoadDemos100` and require **893** titles, each with
a slave, and — on a copy, never on the original — an `igame.data` beside each
one that `igame::parse` reads back. The count is the measurement this design
rests on; if the hook finds a different number, that is the finding.

## 9. What this deliberately does not do

- **It does not write a drawer icon.** §2's rule 2 — a NewIcon's picture lives
  in its ToolTypes and `merge_tooltypes` would delete it.
- **It does not unpack anything by itself in this round.** Cataloguing an
  archive needs no unpacking; writing `igame.data` needs an unpacked tree, and
  where that tree comes from — the user unpacking it, or ART doing so into a
  folder they choose — is a separate decision with its own progress,
  cancellation and scratch-root rules.
- **It does not touch the 1 697 self-booting hardfiles.** §1 is why: iGame
  cannot see them, so there is nothing to tell it.
- **It does not claim the collection is browsable in iGame** until a real
  `igame.data` has been read by a real iGame. The catalogue can say what ART
  wrote and where; it may not say what the Amiga will show.
