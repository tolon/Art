# A layered release: design

*2026-09-04. Closes [work-list item 3](2026-09-04-work-list.md#3--a-release-update-is-layered-and-art-cannot-say-so).
Every number and every claim about the media in here comes from
[the research note](2026-09-04-layered-release-research.md), which was written
first and states what was run.*

**The problem in one sentence.** An AmigaOS release arrives as a base plus an
update, the update says so itself, and ART has no way to express that — so the
owner's 3.2 + 3.2.2 material is refused today over a single disk name, and even
if it were not, the tree ART built would not be 3.2.2.

**The shape of the answer.** A release is already data (`recipes/*.json`); an
update is data too. Nothing in this design resolves anything by the order the
user clicked in.

---

## 1. Layers

A recipe may declare an ordered list of **media layers**, and a component says
which layer its `media` lives in.

```jsonc
{
  "release": "AmigaOS 3.2.2",
  "base": "AmigaOS 3.2",
  "layers": [
    { "id": "base",         "label_key": "osinstall.layer.base32" },
    { "id": "update-3.2.2", "label_key": "osinstall.layer.update322" }
  ],
  "components": [ /* only the update's own */ ]
}
```

- `Recipe.layers: Vec<MediaLayer { id, label_key }>`, `#[serde(default)]`.
  Empty means one implicit layer and today's behaviour, unchanged.
- `Component.layer: Option<String>`, `#[serde(default)]`. **Required whenever
  the recipe declares more than one layer** — a component that omits it is a
  recipe error, not a component that searches everywhere. No implicit
  precedence anywhere in this design.
- `Recipe.base: Option<String>` names another recipe by its `release` string.
  The loader resolves it: the base's components are inherited and stamped with
  the **first** declared layer; this recipe's own components are added after.
  A `base` chain that cycles, or names a release that does not exist, is an
  error at load time and is covered by a test.

`InstallRequest` replaces the folder bag:

```rust
/// One folder per declared layer, keyed by `MediaLayer::id`.
pub media_folders: BTreeMap<String, PathBuf>,
```

`media_folder` and `extra_media_folders` keep their `#[serde(default)]` and are
mapped onto the single layer when `media_folders` is empty, so a request
serialised by an older ART still deserialises and still plans — the rule those
two fields were given when they were added.

`FoundMedia` gains the layer id it was found in. `find_media_across` takes
`(layer_id, folder)` pairs instead of a flat list; `media_for` is asked within
one layer. **Two disks of one name inside one layer are still
`MediaMatch::Ambiguous` and still a refusal** — the layer changes which
question is asked, never how an ambiguous answer is treated.

Two things about resolution *do* have to change with it, and both are the kind
that fail quietly if they are missed:

- **`dedupe_identical_disks` becomes per-layer.** Today it runs over one flat
  list, which is right when there is one; across layers it would drop a
  byte-identical `Workbench3.2` from the update folder and leave that layer
  unable to resolve a component that names it. Identity of content answers
  "is this one disk or two?" **within** a layer; across layers the same bytes
  in two roles are two answers.
- **Two layers pointed at one folder is a refusal that says so.** A user who
  extracted the update into their base folder makes every layer see both
  `DiskDoctor`s, so both layers refuse `MediaAmbiguous` — true, but it reads
  as a problem with their disks rather than with the two fields. The layers
  are compared by canonical path first and the refusal names the real cause:
  *"the base and update fields point at the same folder"*.

### Why not "the later folder wins"

The work-list entry asked for the later disk to win *because it is later*.
This design gives a stronger answer: the later disk wins because **the recipe
names its layer**, and the recipe is ART's transcription of what the release
itself states. A list order is a click; a recipe is reviewable in a diff. The
user still supplies the ordering information — they say which folder is the
base and which is the update — but they say it by answering a labelled
question rather than by the sequence they happened to press "Add" in.

### On the screen

The media step stops being one folder plus an editable list and becomes **one
labelled folder field per declared layer**, in the recipe's order. For 3.2.2
that is *"AmigaOS 3.2 base media"* and *"AmigaOS 3.2.2 update media"*. Both
labels are i18n keys, both catalogues, same commit.

`identify.rs` already answers "which release is this pile of media?" from the
recipes themselves. It gains the same question per layer, so pointing the
update folder at the base field produces *"this folder holds AmigaOS 3.2.2
update media, not the 3.2 base set"* rather than a `MediaMissing` naming a disk
the user does own.

## 2. `amigaos-3.2.2.json`

`release: "AmigaOS 3.2.2"`, `base: "AmigaOS 3.2"`, the two layers above.
Inherited: the base recipe's components on layer `base` — 30 today, **29 once
the empty `update-3.2.1` placeholder goes** (below). Added, all on layer
`update-3.2.2`:

| Component | Media | Rules | Files |
|---|---|---|---|
| `update-322-system` | `Update3.2.2` | ten `File` rules for `C/*.Z` (the drawer also holds `AmigaModel`, `CopyTooltypes` and `GuessBootDev`, which the release does not place); `Subtree` for `DEVS`→`Devs`, `L`, `LIBS`→`Libs`, `Locale/Countries`, `Prefs`, `System`, `Tools`, `Utilities`, `WBStartup`; `File` `Update/Release`→`Prefs/Env-Archive/Versions/Release` and `Update/Startup-HardDrive`→`S/Startup-Sequence` | 39 + 2 |
| `update-322-classes` | `Classes3.2.2` | `Subtree Classes → Classes` | 31 |
| `update-322-diskdoctor` | `DiskDoctor` | `File` ×3: `c/DAControl`, `c/DiskDoctor`, `Devs/trackfile.device` | 3 |
| `update-322-locale-XX` ×17 | `Locale3.2.2-XX` | up to five: `Catalogs`→`Locale/Catalogs`, `Help`→`Locale/Help`, `Languages`→`Locale/Languages`, `Support/Fonts`→`Fonts`, `Support/Prefs/Presets`→`Prefs/Presets` — **per disk, never one list copied seventeen times** | 39 for TR |
| `update-322-modules-a1200` | `ModulesA1200_3.2.2` | the base `modules-a1200`'s shape, `exclusive_group: "modules"` | a handful |

`update-322-system` and `update-322-classes` are `required: true`: picking the
3.2.2 release and supplying no update media is a refusal that names what is
missing, which is the honest outcome. The locale components mirror the base's —
the user ticks the languages they want.

**The seventeen locale components have seventeen different rule lists**, and
that is measured rather than tidy (research note §4): `-EN` carries `Help`
alone, eleven disks carry `Catalogs` and `Help`, only `-CZ`, `-RS` and `-RU`
carry `Languages`. Writing the five-rule list onto all of them would refuse
`MediaMissing` on a path the disk simply does not have. Two of the seventeen,
`-CZ` and `-RS`, have **no base component to override** — they are whole
locales the base set does not ship.

**`Other` and `ReadMe` are deliberately not placed.** `Locale3.2.2-CZ` carries
`Other/Keymaps/cz_ISO-8859-2`, `-RU` carries `Other/ENVARC/Sys/topaz.font` and
a root `ReadMe`, and the release's own `UPDATELOCALE` copies none of them. A
drawer the release leaves for the user to install by hand is not ART's to
install for them — the same rule as the ten `C/` tools above.

**`.Z` needs nothing.** `plan::expand_rules` already marks an entry compressed
and strips the suffix from the destination, and `apply` decompresses as it
writes (ART-228). Rules are written with the destination they land at —
`to: "C/IPrefs"`, not `C/IPrefs.Z`.

**`overrides` is not guessed.** The existing rule — no two components may claim
one destination unless one declares an `overrides` relationship — is enforced
by a test over the recipe. Written against the *merged* recipe it names every
collision, and each update component declares exactly what that test reports.

**The empty `update-3.2.1` placeholder is removed** from the 3.2 recipe. It is
`available: false` with no rules — a §96 "Coming Later" box from the
2026-08-15 plan. Leaving a box reading *"3.2.1 update — coming later"* on a
screen whose release picker offers AmigaOS 3.2.2 is a screen that misleads: a
user ticks it and nothing happens. Three places use it as their example of an
unavailable component (`recipe.rs`'s test, `src/lib/osinstall.test.ts`, and the
old plan document); the two tests move to an example they construct themselves,
which is what they should have done anyway.

**AmigaOS 3.2.1 is not shipped as a release, and that is a decision with a
reason**: 3.2.2's own `HowToInstall` requires "3.2 **or 3.2.1**", so it is
cumulative and the owner's material needs one step, not two. For somebody
holding only 3.2.1 media it is one more JSON file and no code — and that file
is not written until its media is in hand, which is the rule
[ART-159](../../ISSUES.md#fixed) bought.

## 3. `removes` — a component may take a path away

`Component.removes: Vec<String>`, `#[serde(default)]`: destinations this
component deletes from the distribution tree. The 3.2.2 update needs exactly
one, `Tools/TextEditFileTypes/Default4Types`, which reaches the tree from
`Extras3.2` and which the release deletes as unsupported.

- **Only inside the distribution tree ART is building.** This never names a
  path on the user's own disks; it is not `core/hostfs`'s recycler and does not
  become one.
- **Removals run after every placement**, in the merged recipe's component
  order, so the result does not depend on how placement happened to interleave.
- **A removal may only name a destination placed by a component this one
  `overrides`.** Anything else is a recipe error, caught by the same test that
  guards collisions — a component that deletes a file nobody in the recipe
  places is either a typo or a claim about somebody else's tree.
- **Reported per entry**, by name and by result, and logged. A removal whose
  path is not there is *"not present, nothing removed"* — an outcome, not a
  failure; the base component may simply have been switched off.

## 4. `core/amigaicon` — merging an icon's tooltypes and stack

The release does not replace `Tools/IconEdit.info`; it merges into it, and the
research note measures why that matters: the update's icon carries
**`do_StackSize` 8 192 against the tree's 4 096** for a binary the same update
replaces. Skipping it runs a new program on half the stack its release
allocates. Replacing the file outright instead is worse — the icon in an
ART-built tree is the GlowIcons one, so an outright copy loses 1 486 bytes of
appended ColorIcon artwork and moves the icon from (198, 4) to (111, 45).

A new pure-Rust module inside `core/` (no platform, no dependency):

```rust
/// The byte ranges of one `.info`, as it is laid out on disk.
pub struct IconLayout { /* header, optional blocks, tooltypes: Range<usize>, trailing: Range<usize> */ }
pub fn layout(bytes: &[u8]) -> CoreResult<IconLayout>;
pub fn tooltypes(bytes: &[u8]) -> CoreResult<Vec<String>>;
pub fn stack_size(bytes: &[u8]) -> CoreResult<u32>;
/// Tooltypes and stack from `source`, everything else from `dest`, byte for byte.
pub fn merge_tooltypes(dest: &[u8], source: &[u8]) -> CoreResult<Vec<u8>>;
```

The layout was confirmed empirically rather than recalled (research note §8): a
`do_Magic` of `0xE310`, a 78-byte header, then `DrawerData`,
`GadgetRender`, `SelectRender`, `DefaultTool`, `ToolTypes`, `ToolWindow` in
that order — landing exactly on end-of-file for the two classic icons and
exactly on the start of the IFF `FORM` for the ColorIcon.

**The rules this module is written to**, because it is a parser pointed at
files ART did not write:

- Every block's length is computed with `checked_add`/`checked_mul` against the
  buffer's real length, never from a length field alone. An image's size is
  `((width + 15) / 16) * 2 * height * depth`, and every factor is bounded
  before it is multiplied.
- The trailing region — everything after the last parsed block — is **carried
  through verbatim and is never interpreted**. ART does not need to understand
  a ColorIcon to preserve one.
- A file that does not parse is a **refusal naming the file**, never a
  best-effort rewrite.

Expressed in a recipe as a third `RuleKind`:

```jsonc
{ "from": "Update/IconEdit.info", "to": "Tools/IconEdit.info", "kind": "icon-tooltypes" }
```

Semantics: if `to` does not exist in the tree, **do nothing and say so** — the
release's own `if exists` guard, reported as skipped rather than failed. The
rule participates in the override check like any other, so the component
declares that it amends `glowicons`' file.

### The oracle

The parser is checked against material nobody here wrote: a script in the
`scripts/` family extracts **every `.info` on the owner's own 3.2 media and
3.2.2 update** and asserts, for each, that the layout lands exactly at
end-of-file or at the start of a trailing IFF block, and that
`merge_tooltypes(x, x)` returns `x` byte for byte. Not in CI — it needs the
owner's media — and it is the check that says the structure is right rather
than plausible.

## 5. Conditions that can say `47.111`

`Condition::RomOlderThan` carries a `major` only. The Modules step's real test
is a revision comparison, and `core::rom::stated_version` already returns
`(major, minor)` — the minor is thrown away at the condition, not at the
source. So: `RomOlderThan { major, #[serde(default)] minor: Option<u16> }`,
with `None` behaving exactly as today.

`update-322-modules-a1200`'s condition is **"the paired Kickstart is older than
the one this update ships"** — older than 47.111.

**This is not the installer's own test, and the difference is stated rather
than smoothed over.** The release asks the *running* machine: `exec.library`
below 47.10, or `version res strap` below 47. ART has no running machine; it
has the header of a ROM file. The substitute errs towards switching the
modules **on**, which is the direction Hyperion's own `HowToInstall`
recommends ("we recommend using the softkick options for this update"), and
which costs a `LoadModule` double boot rather than a machine that will not
start.

## 6. The tree says what it is

`Prefs/Env-Archive/Versions/Release` is written by the release itself: 11 bytes
`Release 3.2` on `Workbench3.2`, 14 bytes `Release 3.2.2` in the update's
`Update/Release`. An ART-built 3.2 tree already carries the first, through
`workbench-base`'s `Prefs` subtree.

- `identify::release_of_tree(root) -> Option<String>` reads it back.
- `apply`'s result and the OS Builder's report carry it, and it is **its own
  sentence**: "the tree says `Release 3.2.2`", or "the tree says
  `Release 3.2`", or "the tree states no release". Never folded into a pass or
  a fail, because the three are three different next steps.
- `DistributionManifest` gains `layers: Vec<LayerRecord { id, folder }>`
  (`#[serde(default)]`, so an older tree reads back as one unnamed layer) so
  the record says which folder each layer came from.

This is the round's answer to the failure that shipped an AmigaOS 3.5 tree as
3.9: the claim comes from a file the release wrote, not from a drawer name, a
copyright line or ART's own dropdown.

## 7. Deliberately not built

- **An `.info` writer beyond the tooltype splice.** No image editing, no
  ColorIcon understanding, no icon creation.
- **The `WBStartup` reorganisation pass** (research note §4 item 12) — proven a
  no-op here, because no component in the 3.2 recipe targets `WBStartup` at
  all. That proof is a test, not a comment.
- **`UNPROTECT`.** ART writes files fresh into a host folder and carries
  protection bits into `.uaem` sidecars; there is nothing to unprotect first.
- **Unattended Amiga-side installation of the update.** Measured impossible for
  this installer (research note §7): four questions are forced at `(user 2)`
  and `core/amigainstall` treats a requester as a timeout. Running it with a
  person at the window stays a legitimate separate route.
- **AmigaOS 3.2.1, 3.2.2.1 and 3.2.3 recipes.** §2 says why.

## 8. Two `ART-NNN`s this round opens but does not close

1. **An ART-built 3.2 tree starts nothing from `WBStartup`.** `GlowIcons3.2`
   and `Workbench3.2` carry six `WBStartup/*.info` icons between them and no
   component places any of them; AmigaOS starts what the icon says. Found by
   this research, about the base recipe, not the update.
2. **The `.Z` decoder's dictionary-reset branch is still unexercised**
   (`core/archive/compress.rs`'s own disclosure). This round pushes about 108
   more files through that decoder without changing that.

## 9. Testing

Fixtures are synthetic and generated at runtime in a tempdir; ART ships no
copyrighted Amiga content.

| Guard | The mutation that must fell it |
|---|---|
| A name claimed in two layers resolves to the layer the recipe names | point the component at the other layer — the other file's bytes must land |
| A name claimed twice **inside** one layer still refuses | remove the ambiguity check; the plan must stop refusing |
| A byte-identical disk in two layers survives in both | dedupe across layers instead of within one; the second layer must fail to resolve it |
| Two layers on one folder refuse by naming the fields | drop the canonical-path comparison; the refusal must fall back to naming the disks |
| A component with no layer in a multi-layer recipe is a recipe error | drop the check; the recipe must load |
| A `base` that cycles is refused | drop the cycle guard; loading must hang or overflow |
| Merged-recipe destination collisions need `overrides` | delete one `overrides` entry; the recipe test must fail |
| `removes` deletes, and only what an overridden component placed | make it remove an unrelated path; the recipe test must fail |
| `rom-older-than` compares the minor | drop the minor from the comparison; a 47.102 ROM must stop switching modules on |
| `merge_tooltypes` preserves the trailing IFF block | truncate at the tooltype array's end; a GlowIcons round trip must differ |
| `merge_tooltypes` takes the stack from the source | keep the destination's stack; the merged icon must read 4 096 |
| An `icon-tooltypes` rule whose `to` is absent is skipped, not failed | make it fail; the "skipped" verdict must disappear |

Every one of these gets the defect put back and the failure watched, per
CLAUDE.md — and a survivor is reported as a survivor, after asking which of the
two things it is: a weak guard, or the wrong mutation for it.

**The real bar** is an `#[ignore]`d hook against the owner's own material:
build the tree from `E:\amiga\Amigatolon\paketler\3.2` (base) plus the 3.2.2
update, then read `Prefs/Env-Archive/Versions/Release` back and require it to
say `Release 3.2.2`. Passing tests are the claim about the code; that file is
the claim about the tree.

## 10. What could still be wrong

- **The locale rules are written from one copy of the media.** The census says
  what these 28 disks hold; a differently-packed 3.2.2 could carry a directory
  these rules do not name. That is the risk every existing recipe already
  carries, and the real-media hook is what surfaces it.
- **The ROM condition is a substitute** (§5), and a user whose ROM is already
  47.111 gets the modules switched on when the release would have left them
  off. Costs a double boot; says so on the screen.
- **`base` inheritance changes an existing recipe's loading path.** The 3.2 and
  3.9 recipes declare no `base` and no `layers`, so their behaviour must be
  byte-identical afterwards — asserted by planning both against a fixture
  before and after, not by inspection.
