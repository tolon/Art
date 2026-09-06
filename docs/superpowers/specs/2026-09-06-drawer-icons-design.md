# Drawer icons — a tree Workbench can actually show

*Written 2026-09-06. **This is history**: it describes the tree and the owner's
own material on the day it was written. Re-run the commands below rather than
re-trusting the numbers.*

**Round 2 of the Emu68 Hatcher intake**, and it grew a first half that was not
in the intake plan at all: researching the icon round found a **defect that has
been shipping**. The plan called this round "the tree looks made". Measurement
says the tree is not merely unpolished — its top-level drawers are **invisible
on a real Workbench**, because ART never copies their icons.

Round 1's design is [2026-09-05-prefs-and-wallpaper-design.md](2026-09-05-prefs-and-wallpaper-design.md);
the intake note is [2026-09-05-emu68hatcher-teardown.md](../notes/2026-09-05-emu68hatcher-teardown.md).

---

## 1. What was measured

Nothing below rests on recalled knowledge or on the other project's source.

### 1.1 The defect: a `Subtree` rule loses the drawer's own icon

The AmigaOS 3.2 install media carries drawer icons at the root of each disk:

```bash
cd "E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ADF"
xdftool Workbench3.2.adf list | grep -E "^  [A-Za-z]"
```

`Workbench3.2.adf`'s root holds `Devs.info`, `Disk.info`, `Expansion.info`,
`Prefs.info`, `System.info`, `Utilities.info` and `WBStartup.info` — seven.
Across five disks (`Workbench`, `Extras`, `Classes`, `Fonts`, `Storage`) there
are **19 root-level `.info` files**.

ART's own built tree has **none**:

```bash
ls E:/amiga/ProjeART/dist-3.2/*.info | wc -l    # 0
```

**The cause is structural, not a typo.** `workbench-base`'s rules are
`from: "Prefs" → to: "Prefs"`, kind `subtree`, and so on for `C`, `Classes`,
`Devs`, `Expansion`, `Libs`, `Rexxc`, `S`, `System`. A `Subtree` rule copies
everything **inside** the drawer — including nested drawer icons, which is why
`dist-3.2` does have `Prefs/Presets/Backdrops.info` — but the drawer's **own**
icon is its *sibling*, not its child, and no rule names it.

The shape is the same in every shipped recipe: **174 `Subtree` rules across
AmigaOS 3.2, 3.2.2 and 3.9**, and exactly **one** rule in all three names a
`.info` at all (`update-322-system`'s `Update/IconEdit.info`, kind
`icon-tooltypes`, which is ART-104's tooltype merge and a different thing). So
nothing today copies a drawer icon by name, and nothing can double-copy one.

**What this costs on a real Amiga.** `Prefs`, `System`, `Utilities` and
`WBStartup` do not appear in the Workbench window, and the volume has no
`Disk.info` either. The tree boots, every file is present, and the screen is
wrong — this project's named failure class. `core/whdload`'s own module doc
already records the underlying fact: *"a drawer copied without its icon is
indistinguishable from the install having failed."* ART knows it about WHDLoad
packs and does not apply it to the tree it builds.

### 1.2 The icons themselves — 798 of the owner's own

Census over `E:\amiga\Amigatolon\os39`, ART-built AmigaOS 3.9 trees:

| | |
|---|---|
| Classic `0xE310` icons | **798** |
| …carrying an appended **ColorIcon** (`FORM ICON`) | 636 |
| …carrying **NewIcons** (`IM1=` tooltype) | 68 |
| …plain planar | 94 |
| OS4/PowerIcons **PNG** icons | **0** |
| **Gadget size disagrees with rendered size** | **613 (77 %)** |
| **No position** (`do_CurrentX` = `0x80000000`) | **361 (45 %)** |
| Position set by the release | 437 (55 %) |
| Icon **revision 1**, so `DrawerData2` is present | **798 (100 %)** |
| `do_Type` 1 disk / 2 drawer / 3 tool / 4 project / 5 garbage | 37 / 56 / 196 / 506 / 3 |

Two of those rows decide the design.

**77 % is not a rounding error.** A grid built on the `Gadget` width is wrong
for three icons in four. The extreme case is a NewIcon whose `Gadget` says
**3×3** and whose `IM1=` says **46×46** — fifteen times too small. Measured on
`art1/Prefs/Env-Archive/Sys/def_8svx.info` and two dozen siblings. A layout
using the wrong number does not look slightly off; it overlaps every label.

**`0x80000000` is the "no position" sentinel, not `0`.** `art1/Fonts/Disk.info`
carries it; `art1/Devs/DataTypes.info` carries a real `13,4`. Code that treats
`0` as "unset" would move an icon the release deliberately placed at the
window's origin.

### 1.3 Field offsets, read out of real files

```bash
python - <<'PY'   # or use the editor; see CLAUDE.md on heredocs and paths
import struct
d = open(r"E:\amiga\Amigatolon\os39\art1\Devs\DataTypes.info","rb").read()
print(struct.unpack(">HH", d[12:16]))      # Gadget w,h  -> (44, 44)
print(d[48])                                # do_Type     -> 2  (WBDRAWER)
print(struct.unpack(">ii", d[58:66]))       # CurrentX,Y  -> (13, 4)
print(struct.unpack(">I", d[44:48])[0] & 1) # revision    -> 1  (DrawerData2 follows)
print(struct.unpack(">I", d[66:70])[0] != 0)# DrawerData present
print(struct.unpack(">hhhh", d[78:86]))     # NewWindow   -> (393, 126, 342, 163)
PY
```

| Field | Offset | Confirmed on |
|---|---|---|
| `Gadget.Width` / `Height` | 12 / 14 | all three samples |
| `Gadget.UserData` (bit 0 = revision 1) | 44 | 798 of 798 |
| `do_Type` | 48 | 1, 2 and 4 seen |
| `do_CurrentX` / `do_CurrentY` (`i32`) | 58 / 62 | a real position and the sentinel |
| `do_DrawerData` (presence) | 66 | non-zero for both containers, zero for the project icon |
| `DrawerData.NewWindow` (`>hhhh`) | 78 | two real geometries |

**The rendered size**, which is what a layout needs and what the `Gadget`
fields do not give:

- **ColorIcon**: `FACE` chunk, width `data[face+8] + 1`, height `data[face+9] + 1`,
  frameless flag `data[face+10] & 1`. `DataTypes.info` reads 46×46 where the
  Gadget says 44×44. **Trust `FACE` only when an `IMAG` chunk backs it** —
  MagicWB-era icons carry a degenerate `FACE` claiming 256×256 with no image.
- **NewIcons**: the first `IM1=` tool type, width `data[im1+5] - 0x21`,
  height `data[im1+6] - 0x21`. That is where the 3×3 → 46×46 case comes from.
- **Otherwise** the `Gadget` size.

### 1.4 A Hatcher feature that measurement refuses

`builder/staging/icons.py::ensure_dirs_for_orphan_drawer_icons` creates a
directory for any drawer-type `.info` whose directory is missing. Run against
the owner's real 3.9 tree, the only two icons that look orphaned are
`Prefs/Env-Archive/Sys/def_drawer.info` and `def_trashcan.info` — **default
icon templates, which are supposed to have no directory.** Adopting that
function as written would create two bogus drawers inside `Env-Archive`.

This is the same shape as round 1's `Depth × 32` guard, which would have
refused a file AmigaOS itself ships. **Not taken.**

---

## 2. What this round builds

### 2.1 A `Subtree` rule takes the drawer's own icon

`core/osinstall`: when a `Subtree` rule's `from` names a drawer and the medium
holds `<from>.info` beside it, that file is copied to `<to>.info`. It is an
ordinary placed file — recorded in `distribution.json` against the same
component, carrying its `.uaem` sidecar, subject to the same collision rules as
any other destination.

**Two cases are excluded, each for a measured reason:**

- **`from: ""`** — two rules in the 3.2 recipe (`fonts` and `backdrops`) copy a
  whole medium. The medium's root icon is `Disk.info`, a **volume** icon
  (`do_Type = 1`), not a drawer icon (`do_Type = 2`). Handing a drawer a disk
  icon is not "the icon it should have"; those rules take none.
- **The volume's own `Disk.info`** is not placed automatically. Five of the
  owner's disks carry one and only the Workbench disk's is the right one for
  `SYS:`; the engine cannot know which, so a recipe names it explicitly with a
  `File` rule or the tree has none. **This is a decision, not an oversight** —
  it is written here so the next reader does not "fix" it.

### 2.2 `core/amigaicon` learns to read what it skips

Today the module reads the layout, the tool types and `do_StackSize`, and
deliberately skips everything else — it never needed a `DrawerData`, an `Image`
or an appended blob. A layout does. It gains, all bound-checked in the same
style: `do_Type`, the position with its sentinel, `DrawerData` presence and its
`NewWindow`, the revision bit and `DrawerData2`'s flags, and the **rendered
size** rule of §1.3.

### 2.3 …and to write, preserving everything else

`set_tooltypes` (an arbitrary list, where today only `merge_tooltypes` exists),
`set_position`, `set_window`, `set_show_all_files`. Every one rebuilds the file
with **every byte it was not asked to change carried through** — appended
ColorIcon and NewIcon blobs included, which this module already treats as an
opaque trailing region.

Round 1 paid three fix rounds to learn that lesson on `PTRN` flags, `wbp_Revision`
and `SCRM`'s reserved bytes. Here it is the premise, not a discovery.

### 2.4 The grid, scoped by the measurement

**Only icons with no position are placed.** 361 of 798 carry `0x80000000`;
the other 437 were positioned by the release and ART does not touch them. That
follows the project's own rule — *nothing changes unless the user changes it* —
and it is the owner's decision, recorded here. Re-laying out a whole drawer
stays available as an explicit action, not a default.

The arithmetic is Hatcher's, adopted rather than measured, and this document
says which is which so nobody has to re-derive it: Topaz character width 8, a
3-pixel emboss for the frame IControl draws, margins 10/4, an 8-pixel column
gap, an 18-pixel row pad for the label, a 520-pixel inner width budget, **420
for the root** because the `SYS:` window geometry lives in the volume's
`disk.info` which ART leaves alone, and a 2.0 target aspect taken from iTidy.
A cell is the wider of the rendered image and the label at 8 px per character;
rows are bottom-aligned so a row's labels share a baseline.

---

## 3. Verification

`scripts/icon-oracle-check.py` **already exists** and round-trips real `.info`
files through `core/amigaicon` — 485 of the owner's own icons, 0 failures. This
round's writes must go through it: for every real icon, apply each write and
require that **everything not named by that write is byte-identical**, and that
the field named by it reads back as asked. An oracle that only re-reads what
ART wrote proves nothing; the corpus is the point.

Beside it: unit tests pinned to the **measured** bytes of §1.3, the degenerate
`FACE` case, the 3×3 NewIcon, the `0x80000000` sentinel, and — for §2.1 — a
recipe-level test that a `Subtree` rule with a sibling icon on the medium places
it, and that a `from: ""` rule does not.

**Mutation, per `CLAUDE.md`:** every guard that matters has its defect put back
and is watched to fail. Survivors are reported, and each is asked whether the
guard is weak or the mutation was wrong for it.

---

## 4. Not in this round

- **`ensure_dirs_for_orphan_drawer_icons`** — §1.4, refused on measurement.
- **PNG / OS4 icons.** Zero in 798 real icons. The reader may recognise and
  refuse them; nothing lays them out.
- **HAM, EHB, icon-set swapping** (Standard ↔ GlowIcons). A separate decision
  about which icons a tree should carry, not about where they sit.
- **Re-laying out drawers the release positioned**, other than on explicit
  request.
- **A drawer icon for a drawer the media has none for.** Hatcher fills those
  from a bundled template; ART ships no Amiga content, so a template would have
  to come from the user's own media. Left for a round that decides where from.

## 5. Known risks

- **No Amiga has opened an ART-positioned icon.** The oracle proves the bytes
  round-trip and that the fields read back; it cannot prove Workbench draws the
  grid the way the arithmetic predicts. That needs the owner and a screen, and
  it is the same bar round 1 ended on.
- **The grid constants are adopted, not measured.** They come from a project
  that has run them; ART has not. §2.4 marks them so, and the first real
  screenshot is what will confirm or correct them.
- **`Disk.info` stays absent** until a recipe names one. A tree built today
  still has no volume icon, and that is deliberate (§2.1) rather than fixed.
