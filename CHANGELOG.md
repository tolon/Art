# Changelog

All notable changes to Amiga Retro Toolkit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Installing on the Amiga side — the disc a package asks for (2026-08-21)

#### Added
- **ART can now put a disc in the emulated machine, and it checks that it is
  the right one.** Some packages will not install until they have seen the
  medium they were shipped on: both AmigaOS 3.9 BoingBags look for named files
  on a volume called `AmigaOS3.9:` before they do anything. The Amiga-side
  install screen has a new field for your own copy of that disc, remembered
  like every other choice you make.

  Nothing here decrypts anything or works around any protection — giving an
  installer the disc it asks for is meeting its check, not avoiding it. ART
  ships no Amiga media and never will; the disc is yours, exactly as the
  Kickstart and the package archives are.

  Before anything is copied, ART **opens the image and asks it its own name**.
  A disc that calls itself something else is refused, naming both what the
  package needs and what you supplied — a filename is not proof of what is on
  a disc. The preview shows the volume the image itself states, so you can see
  what the machine will get before you start it.

- **A package that needs a disc and has none is refused before the run, and
  the refusal says which disc.** Previously such a run would have started, sat
  there, and been reported as having timed out — which would have been true
  and useless.

#### Known limitation
- **The AmigaOS 3.9 BoingBags still do not install.** With the right disc
  mounted under the right name — confirmed on the running Amiga, its icon on
  the Workbench — the package's own `Updater` starts and then does nothing:
  no output, no window, and no file written, on either BoingBag's installer.
  It stops *before* its own CD check, and behaves exactly the same with the
  disc absent, so the disc is not what it is waiting on and ART does not yet
  know what is. The refusals above are honest, your own tree is never touched,
  and ART does not claim an install it cannot perform.

### Installing on the Amiga side — the first run against real packages (2026-08-21)

#### Fixed
- **Five faults that only a real Amiga could show, and three of them would
  have told you the wrong thing.** ART's Amiga-side install was run for the
  first time against the owner's own AmigaOS BoingBags, and every one of these
  came out of that sitting rather than out of a test.

  Twice, ART was about to report *"nobody answered"* about an installer that
  had answered clearly and at once — the exact kind of confidently wrong
  sentence this whole area was rebuilt to prevent. In one case the installer
  said no and its answer was thrown away; in the other the installer had never
  been started at all, because loading AmigaOS 3.9's own ROM update restarts
  the machine and ART mistook the restart for an install that had already
  happened. Both now report what really occurred.

  The other three were about the environment ART sets up for a package's own
  installer: it now runs AmigaOS's own `SetPatch` the way your system folder's
  own start-up does, it makes the ReAction classes visible where AmigaOS 3.9's
  libraries expect them, and it builds `ENV:` the way a real boot builds it.
  Without the second, the BoingBag installer refused to start; without the
  third, it stopped on a *"Please insert volume ENV"* requester with nobody
  there to answer it.

#### Changed
- **The waiting time before ART gives up on an install is now an hour, not
  twenty minutes.** Twenty was a guess, and the first real package to meet it
  went past it. Erring long only delays the news that a run is stuck; erring
  short reports a working install as abandoned and throws its result away.

#### Known limitation
- **ART still cannot install an AmigaOS BoingBag.** The package's own
  installer checks for the original AmigaOS 3.9 CD-ROM before it will do
  anything, and ART does not yet put that disc in front of it. Everything up
  to that point now works — your system folder is copied, the package is
  unpacked, the emulator boots, the installer starts — and then it waits for a
  disc that is not there. ART is not going to fake the check; the fix is to
  hand it the disc you already own, and that is the next piece of work.
  Nothing is at risk in the meantime: the install runs against a copy and your
  own system folder is never touched, and ART tells you where the copy is
  instead of pretending the install worked.

### Installing on the Amiga side — the groundwork (2026-08-20)

#### Added
- **ART now refuses to install an update package out of order, and refuses
  to run a BoingBag installer that cannot work.** Two things, both found by
  reading the packages themselves.

  The AmigaOS BoingBags are a chain: a clean 3.9, then BoingBag 1, then
  BoingBag 2, then the optional community BoingBag 3 and 4. Installing one
  out of order gives you a system that starts perfectly and is quietly the
  wrong thing — the hardest kind of fault to notice. ART now reads what a
  system folder was actually built from and says, before it copies anything,
  which package has to go on first and in what order. A folder ART did not
  build gets a different sentence, because that needs a different answer.

  And BoingBag 3.9-1's own installer comes in two builds. The one in the
  original download is from 3 April 2001 and cannot install under an
  emulator at all; the fix, `Updater 45.15`, shipped seventeen days later in
  a small separate archive. ART asks the program which build it is — not how
  big the file is — and, if it is the older one, says so and names the
  archive to add rather than starting an install that was always going to
  fail. Add that second archive alongside the first and ART copies the newer
  installer over, exactly as the archive's own readme tells you to do by
  hand. **Nothing is unlocked or worked around**: the package's own installer
  still does all the work.

  ART also asks for a system folder it built. A folder from somewhere else
  has no record of what is already in it, so ART could neither check the
  order nor write down what it just installed — and an install it cannot
  write down is one it would have had to report as failed after it worked.
  It says so before starting rather than after.

  Still not reachable from the interface — that is the rest of this round.
- **The first piece of running a package's own installer inside the
  emulator.** Some update packages cannot be installed by copying files
  from Windows: the two AmigaOS BoingBags keep their payload locked with a
  password that lives in the package's own Amiga program, and others
  install through an Amiga Installer script. ART will run those where they
  belong — on the emulated Amiga, on a copy of the system it is building,
  with your own Kickstart. **Nothing is unlocked or worked around**; the
  package's own installer does what it always did.

  This release carries the part that had to be right first: the small boot
  disk ART writes for such a run. It boots ART's own instructions rather
  than your system's, so your `Startup-Sequence` is never read, edited or
  appended to; it records that a run started *before* the installer begins,
  so a run that stalls can be told apart from one that never began; it
  records an outcome whether the installer succeeds or refuses; and if the
  installer reboots the Amiga, the second boot does not install a second
  time.

  **Not yet reachable from the interface** — there is no screen for it and
  no emulator is started. That is the rest of this round.
- **The run itself: ART now knows how to start such an install, watch it,
  and stop it.** It reads the small file the Amiga writes as it goes, so it
  can tell you what happened while the emulator is still on screen. A run
  always ends and always says which ending it was: it finished, the
  installer refused, or nobody was there to answer a question it asked —
  three different answers, because they are fixed by three different
  things. A run that is waiting on a question does not wait for ever; after
  a set time ART closes the emulator it opened, and says it timed out
  rather than pretending it failed or succeeded. Cancelling stops it at the
  next safe moment and closes the emulator too. ART only ever closes the
  emulator **it** started — your own WinUAE window is never touched.

  The time limit is a placeholder for now, deliberately generous so a slow
  but working install is never cut short. It will be replaced with a real
  measurement of a real update package on real hardware later in this
  round.

  A run now also has a **fourth** possible ending: the emulator was closed
  before it reported anything. That is deliberately not called a timeout —
  "it timed out" tells you to watch the window and answer the question next
  time, which is no help at all for a window you closed yourself.

  And if something goes wrong while ART is watching — the folder becomes
  unreadable, a file cannot be opened — ART now still closes the emulator it
  opened. It used to report the error and leave the window running with no
  way for ART to ever close it again.

  **Still not reachable from the interface**, and no emulator is started by
  anything you can click yet.
- **And now you can ask for it.** The OS Builder has a new panel: *Run a
  package's own installer on the Amiga*. Point it at a system folder ART
  built, pick the package, its own archive and your own Kickstart, and ART
  copies the folder, opens an emulator, lets the package install itself, and
  tells you what happened. **Nothing is unlocked or worked around**; the
  package's own installer does what it always did.

  Four things it is careful about, because each of them is a way of being
  told something untrue:

  - **It says an emulator window is about to open on your desktop**, before
    it opens one, and asks you to confirm.
  - **A run ends four ways and each is its own sentence**: it worked, the
    installer refused, nobody answered a question it asked, or the window was
    closed. Each comes with a different thing to do next — being told "it
    timed out, watch the window next time" about a window you closed yourself
    is worse than being told nothing.
  - **If it did not work, it tells you where the copy is.** Your own folder
    is untouched; the copy the installer worked on is kept exactly as it was
    left, so you can look at what happened. And on the rare occasion the old
    folder cannot be deleted after a success, it says where that is too.
  - **A refusal names what to do about it.** Missing an earlier package? It
    says which, and in which order. Installer too old to run under an
    emulator? It names the second archive that fixes it — and says so *before*
    the run, not after, so one download is the whole fix.

  Beginner mode hides the machinery — the Amiga command line, the volume
  names — and hides nothing else; the warning, the four answers and the run
  itself are the same in both modes.

  And if something goes wrong *while* the install is under way — not the
  installer refusing, but ART losing its footing — the copy stays on disk and
  ART now tells you where it is, in the same red box as the error. It was
  already working that out and saying it; the screen was the one place it did
  not arrive. A cancelled run no longer claims the copy was cleaned up
  either: it says your own folder was never touched, which is true either
  way, and if the copy could not be removed it says that too rather than the
  opposite. While it runs, you also see which step it is on, not just a
  percentage.
- **Each BoingBag now says, in its own recipe, what the Amiga should
  run.** Both point at the `Updater` the package itself carries, with the
  update file it expects — read out of your own archives and out of each
  package's own install script, not assumed. It is written in the recipe
  file rather than in ART's code, so a fourth package that needs the same
  treatment is a new recipe, not a new version of ART. The Turkish catalog
  pack declares nothing, because ART already installs it directly.

  Still not reachable from the interface, and still no emulator: this is
  the declaration, not the run.
- **The install runs against a copy of your system, never the system
  itself.** An update package's installer is an Amiga program ART did not
  write and cannot watch file by file, so ART copies the whole system it
  built — beside itself, which takes a few seconds for a 19 MB AmigaOS
  tree — and lets the installer loose on the copy. The copy takes your
  system's place **only** when the run reports that it succeeded.

  If the run fails, times out, or you close the emulator window, your
  system is left exactly as it was, down to the last byte, and the copy
  stays on disk so you can look at what the installer did before it
  stopped — ART tells you where it is rather than throwing away the
  evidence. Cancelling throws the copy away and leaves your system alone.

  The moment the copy takes over is two renames and never a delete: your
  system is moved aside first and removed only once the new one is in
  place, so there is no instant in which a power cut could leave you with
  neither.
- **The pieces above are now one operation ART can be asked to perform.**
  There are two things to ask for: a **preview**, which says exactly what
  would run — which program, from which drawer, with which arguments, on
  which system, on which emulated machine, and whether your Kickstart and
  an emulator are actually there — and starts nothing and writes nothing;
  and the **run**, which happens in the background so the rest of ART keeps
  working, and can be stopped from the job bar.

  What comes back is the ending, not a verdict ART invented: it succeeded,
  the installer refused, nobody answered a question it asked, or the
  emulator window was closed. Only the first replaces your system with the
  copy. The other three tell you **both** where your untouched system is and
  where the copy is, so you can look at what the installer did. Cancelling
  throws the copy away. Every one of those is written into the operation
  log as it happens.

  The command line handed to the Amiga is assembled by ART from the
  package's own recipe and nothing else — never from anything inside an
  archive — and the installer is run from the package's own drawer, which is
  where the package's own script runs it from and what makes the arguments
  it was written with resolve.

  **Still no screen** — that is the next step of this round — so nothing you
  can click starts an emulator yet.
- **ART now unpacks the package's own archive and gives it to the Amiga as a
  third disk.** Everything above assembled the command that runs the
  package's installer; nothing had actually put that installer anywhere the
  emulated Amiga could see it. The run mounted the system being built and
  ART's own small boot disk, and the installer is in neither — it cannot be
  copied into the system beforehand, which is the whole reason this round
  exists.

  So ART now unpacks the archive you point it at into a folder of its own
  and mounts that alongside, and it checks that what came out really is the
  package you ticked: the drawer has to be there and the installer has to be
  inside it, or ART says so and stops **before** any emulator opens, naming
  what your archive actually held. Pointing at the wrong `.lha` now gets a
  sentence rather than a twenty-minute run.

  **Nothing is unlocked.** The archive ART unpacks is an ordinary one; the
  locked file inside it is copied out untouched, for the package's own
  program to open on the Amiga, exactly as before. Anything inside the
  archive that tries to write outside the folder ART unpacked into is
  refused and reported.

#### Fixed
- **A run would have told you the installer refused, about a program that
  never started.** The installer was on no disk the emulated Amiga could
  see, so the change-directory step failed, the shell found nothing to run,
  and ART's own script recorded a refusal — which ART would then have
  reported as "the installer ran and said no". A confidently wrong sentence
  is worse than an error. Fixed by the third disk described above, and the
  run now refuses to start at all if the package is not there
  ([ART-185](docs/ISSUES.md)).
- **A mistyped key in a recipe file is now an error instead of silence.**
  ART reads its package recipes from small text files, and a key it did not
  recognise used to be ignored without a word — so a recipe that named an
  Amiga-side installer with the wrong spelling was accepted with the
  installer quietly missing, and the package would then have been reported
  as one ART cannot install. ART now refuses such a file and names the key
  it did not understand. Notes written into a recipe for human readers,
  which begin with an underscore, still pass through untouched.
- **An error about a package file no longer says "recipe".** A path typed
  wrongly in a package file was reported as a fault in a recipe — a
  different file, which the reader would have gone and searched in vain.

### Archive names, drawer names, and a batch of long-standing debt (2026-08-20)

#### Fixed
- **The Turkish catalogs now land in the drawer the Amiga actually
  reads.** They belong in `türkçe`, and ART was mangling both accented
  letters on the way out of the archive, so the drawer landed *beside*
  the real one instead of on top of it — 36 catalogs installed, reported
  as a clean success, and invisible to the machine. The booted Amiga
  listed 20 drawers where Windows showed 21. Archive names on an Amiga
  are Latin-1, and ART now reads them that way, the same as it already
  did for CDs. The same fix covers 483 accented names across your whole
  archive collection, not just the Turkish pack.
- **An archive that stores its folders separately no longer collapses
  into one heap.** Some archives keep a file's name and the drawer it
  belongs in as two different pieces, and ART was reading only the
  first — so every file arrived at the top level, piled on top of the
  others. Measured against your own collection: 880 files across eight
  archives, including all 283 of `Update3.2.2.lha` and all 316 of
  `AmiSSL-v5-OS3.lha`. They now keep their folders.
- **File comments are no longer glued onto the file name.** An Amiga
  archive can store a comment right after the name, and ART was treating
  the whole thing as one name — 126 files in your collection. The name
  is now the name, and the comment is kept rather than thrown away.
- **A name Windows cannot store is escaped, and never quietly merged
  with another.** AmigaOS allows names Windows refuses — `AUX` is
  reserved, and `Prices: 1993` is illegal outright — so ART now writes
  those under a safe name and records the real one, putting the true
  Amiga name back when it copies to the card. Where two different Amiga
  names would end up as one Windows file, ART **stops and names both**
  rather than writing one and reporting two.
- **The size a build predicts now matches what it writes.** Building
  from a CD, the estimate counted folders as if they held bytes of their
  own — 6,108,319 predicted against 6,054,225 actually written.
- **A disc that is simply bigger than ART reads no longer reports as
  damaged.** It now says it hit ART's own limit, with its own error id,
  instead of telling you your disc is broken.
- **The staging screen says what it did not look at.** Folders nested
  deeper than ART walks were left out of the plan silently, and adding a
  folder *and* a file inside it counted that file twice and then
  collided with itself. Both are now shown, and neither blocks the
  build.
- **The sidebar collapses to icons when it should.** Under Application
  Size the rule was asking about the wrong window, so at 130% and 200% —
  exactly when the sidebar is widest — it never fired.

#### Added
- **Space on a folder now counts it.** Total Commander's `CountSpace`:
  marking a drawer with Space also totals what is inside it and replaces
  `<DIR>` in the Size column with the real figure, on both sides of the
  file manager — a local folder or a drawer inside a disk image. It runs
  in the background and can be stopped, and when it has to stop early
  the number is shown as "at least this much" rather than as a total.


### Update packages — and the AmigaOS 3.9 tree turns out to have been 3.5 (2026-08-19)

#### Fixed
- **A BoingBag no longer offers you a tick it cannot honour.** Both
  BoingBags sat in the update-packages list with a live checkbox, and
  ticking one and confirming got you an English error out of a ZIP
  reader — *"Password required to decrypt file"* — whatever language
  you had chosen, after you had already agreed to the change. The files
  inside a BoingBag are locked, and only the package's own Amiga-side
  `Updater` has the key. So ART now says that **on the row, before you
  tick anything**, in your own language, and the tick is refused. It
  does not say "Archive not found" — the archive is right there; what
  cannot be done is the placing, from Windows, at all. Put the archive
  on your Amiga and run its own `Updater` there, which is what every
  other distribution builder does too.
  *A pick you made in an earlier run and ART remembered still shows,
  still checked, and can still be unticked — a remembered choice is
  never quietly dropped, and never a dead end.*
- **An update archive that carries a suspicious file name now says so.**
  ART has always refused an entry name that tries to point outside the
  folder it belongs in (`..\..\Startup` and friends) — it just refused
  it silently, so an archive holding one looked exactly like an ordinary
  package. The names are now listed on the package's own row, before you
  commit to anything. Nothing about what gets placed has changed; what
  changed is that you can see it.
- **The AmigaOS 3.9 tree ART built was AmigaOS 3.5. It is now really 3.9.**
  Earlier the same day this changelog said "the tree boots", and it did —
  to a clean Workbench, with no error. What nobody had done was **ask the
  running system which Workbench it was**. Asked, it answered
  `Workbench 44.5 (18-Aug-00)`, which is AmigaOS 3.5; the published
  AmigaOS version history gives 45.1 for 3.9. The cause: your 3.9 CD
  carries **two** install folders, `Workbench3.5` and `Workbench3.9`, and
  the second is an *overlay* — only the files that changed — which is why
  a 3.9 disc has a 3.5 folder on it at all. ART was laying the first and
  stopping. It now lays both, in the right order, and the booted system
  answers **`Kickstart 40.68, Workbench 45.1 (13-Nov-00)`**. The tree
  grows from 1257 files to 1879 — all 622 of them things the base
  install never had at all: Locale, the new Preferences editors, the
  `xad` tools, AMPlifier, ViNCEd. Nineteen files it *does* replace are
  provably newer versions and none is older. The boot error that used
  to scroll past on the way in (`C:LoadMonDrvs: Unknown command`) is
  gone, and the desktop shows the real 3.9 icons instead of generic
  floppies.
  *Why this is written down rather than quietly corrected: a claim that
  was wrong and was caught by measuring is worth more to you than one
  that was always right. The copyright line on the Workbench screen
  proves the screen came from the tree — nothing more. It says the same
  thing on 3.5.*
- **The whole frontend test suite was reporting a pass and failing.**
  `pnpm test` printed "619 passed" and exited with an error code, which
  the build system treats as a failed build. Ten tests were leaving
  unfinished background work behind. Fixed, and the underlying pattern
  it was standing in front of — every screen that subscribes to
  background-job progress could leak a listener or swallow an error —
  was fixed across nine files rather than the two that showed symptoms.

#### Added
- **ART can now add an update package onto a distribution tree it (or
  you) already built** — without rebuilding the tree. Two ways in, and
  they produce exactly the same result: build the base *with* the
  packages in one pass, or add one to a tree that already exists. ART
  proves they agree by building both and comparing the files byte for
  byte.
- **Nothing is overwritten silently.** Before anything is added, ART
  reads what each file would land on and tells you which of five things
  it is: identical (not an overwrite at all, and not listed), an
  **upgrade** (`44.23 → 45.9`, read out of the files' own version
  strings), a **downgrade** (marked as one, with its own heading, its own
  word and its own badge — it does not just look different, it says so),
  the **same version with different bytes**, or a file where neither side
  states a version and only the sizes can be compared. It asks once for
  the whole set, not once per file — a real update package replaces
  hundreds of files, and asking every time teaches you to click through.
- **Update packages are read from their own folder**, kept separate from
  your install disks. ART looked through a real 58-item folder — a 171 MB
  RAR and a 248 MB 7z among them — and identified the 27 archives it
  could open, in under a third of a second, without unpacking anything.

#### Known — please read before you go looking for these
- **ART ships recipes for three packages and can place one of them.**
  It supports exactly the packages it knows by hand — BoingBag 3.9-1,
  BoingBag 3.9-2 and the Turkish catalog pack — and says so on screen.
  Working out for itself how an unknown archive maps onto a system
  volume is a separate piece of work that has not been started. If you
  point it at one of your other archives, it will refuse and tell you
  why, rather than half-installing it.
- **Neither BoingBag can be installed, and this is not going to be fixed
  by a bug fix.** The files inside a BoingBag are **password-protected**,
  and the password lives inside the BoingBag's own `Updater` — an Amiga
  program, meant to run on an Amiga. Every other tool that installs
  BoingBags (HstWB Installer, AmiKit, AmigaSYS, ClassicWB) does it by
  starting an emulator and running that `Updater`. ART places files from
  Windows and does not run anything on the Amiga side, so it cannot read
  them. **The decision, taken deliberately: ART will not break the
  password.** Running an Amiga-side install step properly is its own
  piece of work, and it is not happening next week.
- **A package whose installer is an Amiga Installer script cannot be
  placed, ever.** Not a gap — a boundary. Those scripts decide what to do
  while running on the real machine, and ART has nothing to reproduce
  that decision with. It refuses and says so.

### AmigaOS 3.9 joins 3.2 — build it from your own CD (2026-08-19)

#### Added
- **ART can now build the base of an AmigaOS 3.9 system from your own
  install CD**, the same way it already does from AmigaOS 3.2 floppy
  images: point it at your media, and it reads what your disc actually
  offers, checks for name collisions, and builds the tree. A new release
  picker on the OS Builder screen lets you choose which AmigaOS you're
  installing, and the component list you tick is that release's own — not
  another's; picking a release you don't have media for is refused by
  name, not silently swapped for another.
  **What "the base" means, precisely:** the 3.9 recipe ships **one**
  component today, `workbench-base`. It is roughly 6 MB of a 469 MB disc,
  and `Contribution/`, `Locale`, PowerPC and `Emergency-Boot` are all
  absent by design — the further components wait on a 3.9 tree actually
  being booted, which has now happened (see below). AmigaOS 3.2's recipe,
  by contrast, has 26.
- **A disc dropped on the "What can I do?" panel now offers the OS
  Builder**, alongside the actions it already offered for a disc image —
  so an install CD works the same way a floppy image already does: drop it
  in, and ART offers to build from it.
- Tested against a real AmigaOS 3.9 disc image — the owner's own 469 MB
  ISO file, not a synthetic fixture (no optical drive was involved; ART
  reads a disc image, not a drive). All 588 files and 75 directories that
  one component plans were built from it, start to finish.

#### Fixed
- **The component list now belongs to the release you picked.** Choosing
  "AmigaOS 3.9" planned from the 3.9 recipe but left AmigaOS 3.2's
  26-component checklist on screen, with 3.9's own base component labelled
  "Workbench3.2" — a floppy volume that has nothing to do with the disc
  being installed from. Nothing was ever installed wrongly, but ART was
  showing you one operating system's parts while building another's. The
  list is now read from whichever release's recipe you chose. Your ticks
  are remembered **per release**, so switching to 3.9 and back finds your
  3.2 selection exactly as you left it.
- **A disc whose folder names are in mixed case builds its tree in the
  right place.** On a disc pressed with Joliet long names — where the
  names read `OS-Version3.9` rather than `OS-VERSION3.9` — ART found the
  files but built the system three levels deep underneath itself, without
  saying anything was wrong. It now matches folder names the way AmigaOS
  does, when finding them and when placing them.
- Pointing ART at a *file* where a drawer was expected is now refused by
  name on a disc, as it already was on a floppy image, instead of quietly
  producing an install plan missing everything that rule was meant to copy.

- **The tree boots.** A 3.9 tree built this way was started under WinUAE
  with a licensed Kickstart 3.1 ROM and reached a clean Workbench desktop,
  with no error along the way — the same proof AmigaOS 3.2's tree was held
  to. The emulator configuration is the one ART writes itself, not a
  hand-made one.

#### Fixed after driving it by hand
- **ART now tells you when it will not install, instead of appearing to do
  nothing.** If a folder already exists where the tree would go, ART
  refuses — it never builds over what is already there, which is the right
  thing and has protected real data. But it was refusing *silently*: the
  button stopped saying "Installing", nothing else appeared, and the only
  record was in the operation log. Now an occupied folder is caught while
  you are choosing it, with a sentence explaining what to do, and any
  install that fails or is cancelled says so beside the button.
- **The install shows how far it has got.** A percentage, the file count
  behind it, and the name of the file being written — instead of the word
  "Installing" and twenty seconds of nothing changing.
- **The "Verify against a card" fields explain themselves.** "Amiga volume
  image" sat beside an empty box with nothing to say what belonged in it.
  It is the card or `.hdf` the tree was copied *onto* — not the ISO you
  installed from, and not the folder above it. That whole section is
  optional and only useful after you have written the tree to a real Amiga
  volume; it now says so.

## [0.8.5] - 2026-08-18

### A WHDLoad game from your own collection now starts (2026-08-18)

#### Added
- **Play now gives a WHDLoad launch enough memory to actually load the
  game.** A stock Amiga profile has 1 MB total and none of it Fast RAM, which
  is exactly enough for WHDLoad to start and not enough for it to load the
  game itself — reported as *DOS-Error #103, not enough memory available*.
  WHDLoad titles now get extra Fast RAM added on top of their machine
  automatically; the amount is a setting (*WHDLoad Fast RAM headroom*, 0–8
  MB, Settings → Play) rather than a fixed number you cannot change, and it
  is never applied to a floppy or a plain (non-WHDLoad) hardfile.
- **The confirmation screen now says what memory a launch will use**, not
  only the machine and the ROM, so you can see what will be tried before
  pressing Start rather than learning it afterward from WHDLoad's own error.

#### Confirmed
- **A WHDLoad game from your own collection now runs, start to finish, from
  the Collection screen.** `1000 Miglia` (a self-booting WHDLoad hardfile,
  one of 1697 catalogued the same way in this collection) was launched with
  one click and reached the game — Simulmondo's own title logo appeared in
  the emulator window. Along the way, three things ART chooses for you were
  each confirmed doing the right thing on this title: it picked Kickstart
  3.1 over an older 1.3 sitting in the same ROM folder, it mounted the
  hardfile with the geometry the file was actually built for, and it added
  the Fast RAM headroom above. It also honoured this title's own *allow
  writes* switch, mounting the image read-write because that switch was on.
  One title running is a strong sign the rest of this collection's
  self-booting WHDLoad hardfiles will too, since they share the same shape —
  it is not proof of each one individually, and it does not yet cover a
  bare `.adf`, an `.rp9`-packaged hardfile, a WHDLoad title paired with a
  separate system image (VHD or RDB), or whether a save survives with
  *allow writes* turned on. Those are still unverified and still worth
  trying.

### Pictures already on your disk, a detail panel, and Play (2026-08-18)

#### Added
- **The Collection now shows the pictures already inside your `.rp9` files.**
  Every `.rp9` carries its own screenshot; a new button — *use the pictures
  already in your files* — reads it straight off the package, with no network
  and nothing to confirm. It only fills gaps: a picture already found some
  other way is left alone.
- **You can attach your own picture to a title.** Pick a PNG or JPEG and it
  stays attached — a later refresh or online search will not replace it, and
  it survives a rescan of the folder. If a title has more than one picture,
  a switch lets you choose which one shows.
- **Click a title to open it.** A panel shows the picture, the disk order or
  the WHDLoad slave's name, the Kickstart it declares, where the file is on
  disk, and what ART knows versus what it guessed.
- **Play.** The panel's Play button hands a title to WinUAE: a floppy title
  boots directly, and a hardfile mounts read-only — except one ART itself
  unpacked from a `.rp9`, which is ART's own copy and stays writable so the
  game's saves survive. Most WHDLoad titles in a collection like this are a
  self-booting hardfile — no separate system needed at all, mounted and
  booted directly, same as any other hardfile; a switch lets one of these
  mount writable too, once you've been told that leaving it off means the
  game's own saves are not kept. A WHDLoad *drawer* — given a bootable
  system image you already have — either mounts it for you to start the game
  yourself, or, when your system supports it, boots straight into the game in
  one click; your original system image is never written to.

#### Fixed
- **A self-booting WHDLoad hardfile no longer asks for a system it does not
  need.** It was catalogued the same way as an unpacked drawer, so Play sent
  you looking for a bootable system for a title that already boots itself —
  landing at a bare AmigaDOS prompt rather than the game. This was most of
  this user's WHDLoad titles.
- **An older catalogue no longer breaks the Collection screen.** A catalogue
  written before that fix now reads as *stale, needs an update* rather than
  failing to open at all — press Update and it rebuilds itself. Your own
  title corrections and attached pictures are unaffected either way.
- **A WHDLoad title with no stated chipset or Kickstart no longer boots on
  whatever ROM happens to sort first.** Play now refuses to boot one of these
  on a Kickstart too old for WHDLoad to run at all, and picks the newest
  suitable ROM available instead of the first one alphabetically. Titles like
  this used to land on Kickstart 1.3 — which cannot run WHDLoad, and cannot
  mount a hardfile's filesystem either — and reported *not a DOS disk*
  against a perfectly good file.
- **A change meant to stop a hardfile from "losing its last cylinder" was
  itself wrong, and has been reverted (ART-149).** WinUAE's forced geometry
  for a plain hardfile (32 sectors, 1 surface) was briefly changed to present
  a file's exact byte size instead, on the theory that the old geometry
  silently dropped a file's last partial cylinder. That theory's mechanism
  was real but its conclusion was not: this collection's self-booting
  WHDLoad hardfiles were themselves built at the old 32-sector geometry, and
  the filesystem inside each one is sized to match — so presenting the exact
  byte size instead made AmigaDOS look for the root block in the wrong
  place, and titles that had mounted fine started reporting *not a DOS disk*
  instead. The original 32-sectors/1-surface geometry is restored; see
  ART-149 for the six-image measurement that settled it.

#### Notes
- **None of this has been run against your own files yet.** It is written and
  tested, but no session here has your `.rp9` collection, your emulator, or
  your `AmiKit.hdf` to try it against. Please run the three checks below and
  report back what you actually saw — not just whether something happened,
  but what the screen said if it didn't work as expected.

#### To verify yourself
1. Open the Collection on your `.rp9` folder, press *use the pictures
   already in your files*, and note the real numbers it reports — how many
   were written, how many were already there, how many it could not read.
2. Play a bare `.adf` title, then an `.rp9` title. Both should reach a
   running game.
3. Play a WHDLoad title against `E:\amiga\amikit\AmiKit.hdf` — try the
   one-click option first. If it does not reach the game, note exactly what
   the screen showed; that ART offers a fallback is not the same claim as
   that the fallback is needed, and which one actually happened is what's
   worth knowing.

### Colours you can actually read (2026-08-18)

#### Fixed
- **Every status badge is readable now, in both themes.** The green *OK*, the
  amber *CRC ERR* and the red error pills were coloured the same as the pill
  behind them — measured, the light theme's *OK* badge was 2.20:1, where
  readable text needs 4.5:1. Each colour now has a text version dark enough
  (or light enough) to be read on its own background.
- **File paths and secondary lines are no longer whispers.** The faintest text
  in ART sat at 2.85:1 on a light page and 2.58:1 on a highlighted row. Both
  levels of secondary text are now readable, with the difference between them
  kept.
- **The primary button's label.** White on the light theme's blue was 3.94:1;
  the blue is slightly deeper now, and it clears 4.5:1.
- **Input boxes look like input boxes.** Their edges are drawn with a border
  strong enough to see against the page — 3:1, the threshold for a control you
  have to find before you can use it.
- **The crash screen is readable on a light background.** It had the dark
  theme's colours written into it, so a stack trace appeared at 2.3:1.

#### Notes
- Contrast is checked by a script (`scripts/contrast-check.py`) on every build,
  not by looking at a screenshot. It measures all 90 colour pairs the program
  can render and fails the build if one drops below its threshold.

### Two labels that said more than ART knew (2026-08-18)

#### Fixed
- **ART no longer calls your accelerator ROMs broken.** Scanning a ROM folder
  used to mark anything that was not a Kickstart `CRC ERR` — a claim that the
  file is damaged. Of the 76 files in one real collection, 46 got it: Blizzard,
  CyberStorm, GVP, Apollo, A2630, A4091, and both halves of every split dump.
  They are all fine. They are simply not Kickstarts, and they carry no Kickstart
  checksum for ART to check. The badge now says **not a Kickstart** — or
  **encrypted**, for an Amiga Forever ROM whose `rom.key` is not beside it —
  and `CRC ERR` is kept for the one case it means something: a Kickstart whose
  checksum really does not add up.
- **And it stops calling them Kickstarts, too.** A 256 KB accelerator ROM was
  named *Generic Amiga 256KB ROM (Kickstart 1.x)* on its size alone. It now
  reads *Not a Kickstart image (256 KB)*.
- **A blank field says why it is blank.** *Compatible Amiga Models* was empty
  under the CDTV Extended 2.30 because ART's source names no machine for that
  dump. The screen says so now, rather than leaving a gap that looks like
  something missing.
- **Aminet's search and folder boxes follow the theme.** They came out white on
  a dark page — bare form controls taking the browser's own colours. Every input,
  dropdown and text area in ART now takes the theme, in both light and dark.

### Tidying up names ART could only read off a filename (2026-08-18)

#### Added
- **Edit any title, on the spot.** Every row in the Collection now has an
  **Edit** button. Type what the game is actually called, press Enter, and it
  stays that way through every rescan.
- **One-button fixes where ART is sure.** A file called `A-Train Disk 1.adf`
  offers **Fix title** — showing it as *A-Train*, one game rather than two
  entries — and **Rename file**, which tidies the file itself to
  `A-Train (Disk 1).adf`. Both are suggestions; nothing changes until you say
  so, and a title fix can be undone from the same place.
- **Multi-disk games are recognised by their neighbours.** `dune2-2.adf` is
  Dune II's second disk, and ART knows because `dune2-1.adf` is sitting beside
  it. On a real 847-file library this resolves to 523 games rather than 847
  entries.

#### Notes
- **ART will not guess at a name, deliberately.** Where the evidence runs out it
  says nothing and leaves the Edit button to you. `Turrican 2` next to
  `Turrican 3` looks exactly like a two-disk set and is not one; a folder of
  numbered disk-magazine issues looks the same again. Rather than be clever and
  occasionally wrong about what your games are called, ART only proposes what it
  can show a reason for.
- Renaming a file asks first, shows both names in full, and **refuses** if
  something of that name is already there. Your catalogue follows the renamed
  file — nothing is lost and nothing needs re-scanning.

### The Collection gets pictures (2026-08-17)

#### Added
- **Cover art, screenshots and icons for your titles.** A new **Fetch artwork**
  button on the Collection looks your library up in two places: the
  libretro thumbnail archive, and whdload.de's own icons. Pictures appear in
  both the grid and the list.
- **The sources are yours to change.** Settings now has an **Artwork sources**
  panel: switch either source off, or point it at a different address. Both
  ship switched on.
- **Nothing is fetched until you ask.** Opening the Collection reads what has
  already been saved and touches no network. The fetch runs in the background
  with a progress bar and a Stop, and it is deliberately unhurried — no more
  than four requests a second, because whdload.de is run by volunteers.
- **It never asks twice.** Pictures are saved per title, so one file serves
  every copy of a game you own, and titles nobody has a picture for are
  remembered as such. A second run over the same library fetches nothing.

- **Pictures appear as they arrive**, and the search box stays put. The filter
  bar is now stuck to the top of the Collection, so you can narrow a
  1700-title library without scrolling back up to reach it.

#### Fixed
- **An interrupted fetch no longer throws its work away.** The record of what
  had been downloaded was written only when a run finished, so stopping half
  way left the pictures on disk with nothing that knew they were there — they
  did not show, and the next run downloaded them all again. The record is now
  kept as the run goes, and a picture already on disk is used rather than
  fetched a second time.
- **Fetching is roughly forty times faster.** ART was asking GitHub's picture
  archive as slowly as it asks whdload.de — which is right for a server
  volunteers run, and needlessly cautious for a large one — and it was
  downloading four pictures per game when the list shows one.

#### Notes
- Matching is strict on purpose. A title either matches by name, or by the part
  before a subtitle — `1869` finds `1869 - Erlebte Geschichte Teil I` — or it
  gets no picture. ART does not guess, because a wrong cover you cannot explain
  is worse than none.
- **Chipset, genre and rating are still empty, and this is not an oversight.**
  There is no source ART can use for them: Lemon Amiga refuses automated
  requests outright, Hall of Light publishes only web pages, and OpenRetro —
  which has exactly the right data — documents no way in yet. Attaching a
  picture by hand is also not here; that needs the richer screen still to come.

### The Collection remembers, and asks before it deletes (2026-08-17)

#### Added
- **The Collection opens instantly and keeps its folders.** The catalogue is
  saved, so a library that took minutes to read is there the moment the screen
  opens — including after ART has been closed and reopened.
- **Update reads only what changed.** A file ART has already read, whose size
  and date have not moved, is not opened again. On a 1699-title library the
  second Update finishes at once.
- **More than one folder.** Keep games in as many places as you like; they
  appear as one library, and each folder is updated, rescanned or removed on
  its own.
- **A title whose file has moved is followed.** Rename a game outside ART and
  the next Update finds it under its new name rather than showing the old one
  as missing — it is recognised by its contents, not its filename.
- **A title whose file has really gone is kept and marked**, with its launch
  buttons disabled rather than hidden. Unplugging a drive does not empty your
  library.
- Corrections you make by hand are stored apart from what ART read, so a
  rescan never overwrites them, and the previous version is backed up.

#### Fixed
- **Confirmations now actually appear.** Deleting a file, discarding a
  modified file, deleting a PiStorm firmware set and removing a folder all
  asked for confirmation in the code and showed nothing on screen — the
  browser dialog ART relied on returns "yes" without opening in this kind of
  window. Thirteen confirmations were affected, four of them standing in front
  of a deletion.

### The Collection knows what a game is called, because it asks the game (2026-08-17)

#### Added
- **Titles now come from whatever actually states them, not from the
  filename.** A WHDLoad game carries its own name, copyright year and
  publisher inside its `.slave`; a Cloanto `.rp9` package carries a curated
  manifest. The Collection reads both. Against a real 1698-title library that
  means 1679 names, 1678 publishers and 1570 years came from the game itself
  rather than being guessed — `Lotus3HD` on disk is `Lotus 3` on screen.
- **Anything ART guessed is marked as a guess.** A value read from a filename
  gets a small `~guessed` badge naming where it came from. The distinction is
  real: a game whose slave declares it needs AGA is a different claim from one
  whose filename merely contains the letters "AGA", and the two used to look
  identical.
- **A game that asks for a particular Kickstart says so.** 758 of the 1698
  named an image such as `kick34005.a500`.
- Bootable single-game hardfiles — the shape most WHDLoad collections come in
  — are read from the inside rather than by their filename. All 1697 in the
  test library are read, none refused.

#### Changed
- A title whose chipset nothing declares now reads **Unknown** instead of
  being listed as OCS/ECS. The old default was a guess presented as a fact.

#### Fixed
- **Hard disk images whose filesystem is smaller than the file now open.** A
  hardfile's partition is a whole number of cylinders and its file need not
  be, so most WHDLoad game images are a few blocks larger than the volume
  inside them. ART placed the root block from the file's size and read
  nothing but noise. 1456 of 1697 real images were affected — in the
  Collection, in ADF Studio and in the file manager alike.
- The Collection no longer starts a second scan of the folder it was just
  asked to scan.
- A progress bar no longer rounds up to 100% while it is still working.

### ART warns before preparing a card whose Kickstart is older than the system needs (2026-08-17)

#### Added
- **Before formatting and filling a card, ART now says whether its Kickstart
  suits the AmigaOS about to go on it.** Some systems need a newer Kickstart
  than others — AmigaOS 3.2 without its own compatibility modules needs
  Kickstart 47, for instance — and putting one of those onto a card carrying
  an older ROM used to fail silently on the Amiga, with nothing on the
  preview screen to explain why. The confirmation now names both numbers
  when they disagree, before the partition is erased.
- Nothing is said when there is nothing to check: a build that carries its
  own compatibility modules suits any Kickstart, and a card or a tree ART
  cannot read a ROM record from is left alone rather than guessed at.

- **The warning is per folder, and named by drive.** A card can be given a
  different folder for each partition, so ART checks each one and says which
  drive a warning is about. A folder ART is happy with still says nothing.
- **"Checking…" while it checks.** An empty space above a checkbox that
  erases partitions must not be able to mean "the answer never came", so
  the check says while it is running, and says so too when it could not run
  at all. It never disables the checkbox: this warns, it never blocks.

#### Verified
- Against the pairing that actually failed under WinUAE with a licensed ROM
  on 2026-08-16 (real hardware untouched): a real AmigaOS 3.2 build requiring
  Kickstart 47 against a card carrying a real Kickstart 40 is reported as
  needing 47 and finding 40. The same build's own module-carrying
  counterpart, checked against the same card, is never flagged — and against
  a second card carrying the user's real Kickstart 47, which it was never
  built for, it is accepted on its own merits rather than on the ROM's
  version number.

### hst-imager is no longer required to prepare a card (2026-08-16)

#### Changed
- **Preparing a card's Amiga volumes no longer needs `hst-imager` installed.**
  ART now formats and fills PFS3 and FFS volumes with its own writer by
  default — no external tool, nothing launched. `hst-imager` stays as a
  named fallback for the two things ART's own writer cannot yet do:
  embedding a filesystem driver into a card's existing partition table in
  place, and an AmigaDOS name outside plain ASCII on a PFS3 volume. The
  choice is made per step, not for the whole run, so one accented folder
  name does not push an entire card onto the external tool.
- **Never silent about which one ran.** The result panel now lists every
  step alongside the tool that actually performed it, and why, whenever
  that was not ART's own writer. The preview says which writer is expected
  to run each step *before* the confirmation checkbox, since formatting a
  partition is destructive and the default writer changed under this
  release.

#### Fixed
- Two warning badges (the card-preparation screen's own, and the
  content-layout screen's) still told the user ART could not write PFS3
  itself, and the result panel's "not verified" wording implied the
  opposite of what a native run actually did. All three now describe the
  native-by-default writer honestly, including that neither writer's
  output is read back and checked within this operation, native or
  fallback alike.
- The result panel could print "by ART's own writer" for a run where every
  single step had actually gone through the fallback tool, contradicting
  the per-step list directly beneath it.
- A count-bearing string on the same screen (how many names are not ASCII)
  had no plural forms, unlike every other one.

#### Known
- The real ~4000-file AmigaOS 3.2 distribution tree built in the entry
  below was not re-run through this new fallback path end to end — the
  non-ASCII-name figures there remain the earlier, separate measurement.
  **Since re-run** — see the entry above it.

### Amiga Forever ROMs work like any other (2026-08-17)

#### Fixed
- **A licensed Amiga Forever ROM can now be used to build a card.** These
  files are the same Kickstart kept behind a header and a simple encoding,
  unlocked by the `rom.key` that came with your licence. ART used to copy the
  file onto the card exactly as it sits on disk — so the Amiga found encrypted
  bytes where its Kickstart should be and would not start, with nothing on the
  way there saying so. ART now decodes it with the `rom.key` sitting beside it,
  and the ROM is then recognised, named and matched to a machine like any
  other dump.
- **When the key is missing, ART says so and stops.** A card built from an
  encrypted ROM cannot boot, so the build is refused — naming the file, what
  would happen, and where to put the key — rather than written and hoped for.
- **The ROM screen no longer shows a green tick for a ROM it cannot read.**
  "Header stripped" was true and meaningless; it now says either that the ROM
  was decoded with your key, or that the key was not found beside it.

#### Verified
- Against a real Amiga Forever collection: 25 of the 41 ROMs in it are now
  named with the machine they belong to, each agreeing with Cloanto's own
  filename, while the boot and keyboard ROMs sitting beside them claim
  nothing. The decoding was checked with a genuine `rom.key` and a genuine
  ROM rather than a made-up pair.

### ART knows your Kickstarts now (2026-08-16)

#### Fixed
- **ART recognises real Kickstart ROMs.** Its ROM list held ten entries with
  no record of where they came from, and measured against a real collection of
  29 Kickstart dumps it recognised **none** of them — so the check that warns
  "this ROM is not for the machine you picked" had never once been able to
  fire. ART now identifies a ROM by a value the ROM itself stores, which is
  different for every build: it can finally tell the A1200 and A4000 versions
  of the same Kickstart apart, and it names 24 of that collection with the
  machine each one belongs to.
- **The list is no longer hand-written.** It is generated from an independent,
  open-source ROM database and re-checked against it on every build, so it
  cannot quietly go stale or grow an entry nobody verified.
- **A file's size no longer names a machine.** A 256 KB ROM image was
  described as suiting an A500 and an A2000 purely because of its length —
  which is what ART said about a CDTV ROM. The size still describes the
  image; it no longer claims a machine nothing measured.

### Honest numbers on the install report (2026-08-16)

#### Fixed
- **The build report now describes the folder it made, not the work it did.**
  Where two components legitimately write the same file — one deliberately
  replacing the other's copy — both were counted, so a real AmigaOS 3.2 build
  announced 4047 files when 3950 were there. More seriously, the record it
  writes beside the tree (`distribution.json`, the only account of which
  install disk each file came from once the disks are put away) listed 94
  files **twice**, crediting two different components for the same file. One
  of those two entries was always wrong. Each file is now recorded once, by
  whichever component's copy actually survived, and folders created on the
  way to somewhere else are counted rather than missed.
- **A size ART does not have is no longer shown as zero.** When the external
  tool does the copying it reports its own total rounded ("12.2 MB"), which
  is not a byte count — so the line simply leaves it out instead of claiming
  a twelve-megabyte copy moved nothing. A copy that genuinely moved nothing
  still says so.

### AmigaOS 3.2 boots from a disk ART prepared (2026-08-16)

#### Fixed
- **Hard disks ART creates can now be mounted by an Amiga at all.** When ART
  puts a filesystem driver (PFS3) onto a disk it creates, it also has to tell
  AmigaOS to use it — and it was filling in the wrong field, so every such
  disk ART has ever written was ignored: the drive did not appear, and trying
  to boot one crashed the machine with a Software Failure. The driver itself
  was always written correctly, which is why every tool that reads these
  disks reported them as fine. Only a real Kickstart could tell the
  difference, and now one has.
- **The AmigaOS 3.2 system ART builds now starts Workbench.** Three things
  were missing, each of which stopped the boot with a requester: two
  libraries the 3.2 A1200 ROM does not contain and the Workbench floppy does
  not carry (`icon.library` and `workbench.library`, both on the Install
  floppy — a disk ART had not been reading at all), and the desktop
  wallpapers, which had been left switched off until somebody could confirm
  where they belong. The running system named the place itself.

#### Verified
- AmigaOS 3.2 boots to a clean Workbench — wallpaper and all, no error
  requesters — from a PFS3 volume ART formatted and filled, under WinUAE with
  a licensed Kickstart. A smaller volume ART formatted with its own writer
  boots to an AmigaDOS prompt and runs its startup script. **These are the
  first hard disks ART has made that an Amiga has booted**, and the code that
  read them is AmigaOS's own — the Kickstart ROM and the real PFS3 handler,
  running. What is still untested is a card in a PiStorm: the partition
  table, the boot partition the Raspberry Pi side reads, and the firmware
  that starts it.

### The whole 3.2 tree reaches a volume in one run (2026-08-16)

#### Fixed
- **A volume is now prepared by one writer, start to finish.** Preparing a
  PFS3 volume and filling it from a folder with non-English names used to
  stop right after the format: ART formatted the partition itself and then
  handed the copy to hst-imager, which reported the empty disk as full.
  ART now decides *before* it formats anything, so whichever writer has to
  do the copying does the formatting too. The full AmigaOS 3.2 tree — 3933
  files, Spanish, Turkish, Austrian and French names included — now goes
  onto a volume in one run, unattended.
- **Nothing is erased for an operation that cannot then finish.** If the
  copy needs hst-imager and none is configured, ART says so before the
  partition is formatted rather than after.
- **When an external tool fails, ART now shows what went wrong.** A tool
  that crashes prints its message and then a page of internal call
  locations; ART was quoting the last of those instead of the message, so a
  failed card preparation said nothing usable at all.

#### Known
- A copy done by hst-imager reports **0 bytes** alongside its (correct) file
  and folder counts — the tool's own summary rounds the size to "12.2 MB",
  and ART would rather show nothing than invent an exact number from a
  rounded one. The fix is to leave the byte count out; it is not done yet.
  **Since fixed** — see the entry above.

### Install AmigaOS 3.2 from your own media (2026-08-16)

#### Added
- **The OS Builder can now build an AmigaOS 3.2 distribution tree from your
  own install floppies.** Point it at a folder of ADFs and your Kickstart:
  it works out which components apply — Workbench, Extras, every Locale disk,
  Fonts, Classes, GlowIcons, DiskDoctor, MMULibs, HDTools, Storage — and, if
  your Kickstart is older than V47, switches on the A1200 Modules disk for
  you, unasked, because 3.2 needs it and 3.9 does not. Media disks are found
  by the volume name recorded *inside* them, never by filename, so a renamed
  or reordered floppy image still resolves.
- **A manifest, not just a folder.** Every file the build places is recorded
  with which component put it there, which disk it came from, its SHA-256
  and its AmigaDOS protection bits — the only record of what an install did,
  since the original floppy images are not kept around afterwards.
- **The tree can be put onto a real PFS3 volume — a 128 MB `.hdf` file built
  by ART itself, not physical hardware — and read back independently.**
  Verification tells "ART did not check this" apart from "ART checked this
  and it's fine" — a filesystem ART cannot yet verify reports honestly
  rather than showing a false pass. (FFS volumes are also supported by the
  same code path but were not part of this run.)
- **Run for the first time against the user's own real 3.2 media** (36
  floppy images, a real Kickstart v3.1 rev 40.68): 26 components, 4030 files,
  11.9 MB, and the pre-V47 Modules component switching itself on exactly as
  designed. Two real defects the recipe's own data had — found only by real
  media, never by a fixture — are already fixed.

#### Known
- **Putting the *full* tree onto a volume does not yet work for every
  language.** A dependency this feature relies on mishandles any AmigaDOS
  filename outside plain ASCII, which is real content on the Spanish,
  French, Portuguese and Turkish locale disks (and the base Locale disk's
  own country list) — 24 directories, but excluding a directory excludes
  everything under it: **about a quarter of the whole tree** (969 of 4030
  files, 106 of 330 directories) does not yet reach a volume. The copy
  refuses loudly rather than writing anything wrong; it just does not yet
  succeed for those files. **Since fixed** — see "The whole 3.2 tree reaches a
  volume in one run" above: the copy falls back for those names and the whole
  tree lands.
- **No Amiga, real or emulated, has booted anything this feature has built.**
  The distribution tree and the volume it produces are proven independently
  of ART — but the WinUAE rung and the real-hardware rung are both still
  open. **The emulated rung has since been reached** — see "AmigaOS 3.2 boots
  from a disk ART prepared" above.
- **The install screen itself is only lightly verified.** Its top-level
  layout has been seen rendering correctly; the component checklist, the
  confirmation step, the file list and the verify results have not yet been
  driven end to end in a real browser session.

### Drop a pile of files, get an organised staging tree (2026-08-15)

#### Added
- **A new screen, `layout`, that answers "what goes where".** Drop files or
  folders on it and each one gets a proposed destination — `Games`,
  `Floppies`, `HardDisks`, `CDs`, `Unsorted` — in a tree on your PC that the
  OS Builder's preload screen then copies onto a card's Amiga volume.
- **A WHDLoad drawer is placed whole, not scattered.** Drop a folder holding
  fifty games and each one's drawer travels intact, with its icon placed
  beside the drawer rather than inside it — inside, the game would be on the
  disk and invisible on Workbench.
- **A ROM and a Commodore disk are refused, with a reason, rather than
  placed.** A ROM belongs on the card's boot partition, not an Amiga volume;
  a C64 disk has no business on one at all.
- **The preview is a table you can edit, not a rule ART enforces.** ART
  cannot tell a demo from a game — nothing in what it can detect says so —
  so it proposes only what it can justify, and you retarget one row or many
  before anything is written. Nothing overwrites, nothing touches the
  source, and stopping partway reports how much landed.

#### Known
- **Not yet driven against real material in the running application** — only
  in a headless browser against the built bundle.
- **No staging tree this builds has been carried onto a card.** The folder it
  produces is pointed at by hand from the preload screen; there is no
  automatic chaining between the two yet.

### Prepare a card's Amiga volumes, from the OS Builder (2026-08-15)

#### Added
- **The step after the card exists has a screen.** OS Builder's third choice:
  point ART at a card, tick the partitions to prepare, name each volume and
  optionally give it a folder of content, then preview what would happen
  before any of it does.
- **Nothing is ticked to start with, and the ticks are not remembered.** The
  card and the paths come back the way you left them, the way everything in
  ART does. What gets erased does not — coming back to a screen already armed
  to format two partitions is not the same kind of convenience.
- **Volume names are checked before the format, not during it.** The two rules
  AmigaDOS itself has: no `:` or `/`, and thirty characters, counted as
  characters.
- **It says what it cannot check.** Once a volume is formatted and filled, ART
  has no PFS3 reader to look inside it — so the result panel reports the tool's
  word as the tool's word, and points you at the Amiga.
- **`hst.imager`'s path is a setting**, beside the WinUAE path. ART does not
  ship the tool; you point it at your own copy, from Settings or from the
  screen itself, and it can be asked to say which version it is.

### Drag your files onto the card builder (2026-08-15)

#### Added
- **Drop the Emu68 archive and your Kickstart on the screen and the form
  fills itself.** The archive's name says which PiStorm board and which
  release line it is for, and ART says so back rather than guessing.
- **Everything else gets an answer too.** A game, a disk image, an AmigaOS CD
  or a folder is told it belongs on an Amiga volume — and that this card has
  none formatted yet. A `config_<name>.txt` is recognised as a distribution's
  Pi config and declared not-yet-used. A Commodore disk is told plainly it has
  no place on a PiStorm card.

### Check the card before you flash it (2026-08-15)

#### Added
- **One button that answers "is this card right".** It checks the partition
  table, where each Amiga disk landed, every partition table inside them,
  whether any partition names a filesystem the card does not carry, and
  whether the card still matches the manifest built with it.
- **It says what it could not check, and never as a tick.** ART writes the
  Pi's boot partition but cannot read one back, so the files on it are
  answered from the manifest — and on a card ART did not build, not at all.
  Those show as *not checked*, in their own mark and their own colour.
- **The steps only you can take are on the same page**: write the image to a
  card, plug HDMI in before powering the Amiga on, check the Pi in the machine
  is the one the card was built for, and expect the Amiga to offer to format
  its volumes.

#### Changed
- The manifest check added earlier the same day is now part of this one report
  rather than a separate button.

### A card says what it was built from (2026-08-15)

#### Added
- **Every card comes with a build manifest.** `card.img` now has a
  `card.img.manifest.json` beside it, recording which Emu68 archive and which
  Kickstart went in — with their checksums — where every partition landed, and
  a checksum for each of the files on the boot partition.
- **Check a card against its manifest, any time.** A button on the result panel
  re-reads the card and says whether it is still what was built. It also says
  plainly what it did *not* look at: ART writes the Pi's FAT32 partition but
  cannot read one back, so those files are recorded rather than re-checked.
  `python scripts/fat-oracle-check.py your-card.img` checks exactly those, with
  7-Zip.

#### Known
- Rebuilding a card *from* a manifest is not built. The manifest names the
  files it was made from; putting them back together is a later step.

### You can ask for a PiStorm card (2026-08-14)

#### Added
- **The OS Builder can build a boot-only card.** Point ART at the Emu68 release
  you downloaded and at your own Kickstart, choose a size and where the image
  goes, and it produces a card image: the Raspberry Pi's boot partition with
  the firmware and your ROM on it, and a partition table an Amiga can see. The
  engine could do this since the day before; nothing in the application could
  ask for it.
- **Preview before build.** The card is described first — where the boot
  partition and the Amiga disk land, which file the card boots, what your ROM
  is, and every file going onto the boot partition with its size. Nothing is
  written until you have seen that.
- **A destination that already exists is refused before the button**, not by a
  build that fails halfway.
- **Advanced settings on the same screen**: the board, the Emu68 release line,
  the FAT32 label, the boot partition's size and the Amiga disk's partition.
  The first four are the same settings the PiStorm studio uses — change them in
  either place and both agree.

#### Known
- The Amiga volumes on the card are **not formatted**. The Amiga sees them and
  offers to format them; installing AmigaOS onto them is not built yet, and the
  screen says so rather than leaving it to be discovered.
- One Amiga disk per card. Several disks with a boot menu is multiboot, which
  is not built yet.
- **No card ART has built has been flashed or booted.**

### Writing into a partition of a small hard disk image (2026-08-13)

#### Fixed
- **A small hard disk image could not be written into at all.** Anything under
  16 MB went down a path that treated the partition's block numbers as if they
  were the file's, so the first read failed with something unhelpful and the
  write was refused. It works now, and everything around the partition — the
  partition table, anything else on the image — is left byte-for-byte as it
  was. Larger images were never affected.

### Application Size cannot hide anything any more (2026-08-13)

#### Fixed
- **Content too wide for the window can be scrolled to.** Making the
  application bigger makes the room smaller, and anything that no longer fitted
  used to be cut off with no scrollbar, no wheel and no way to reach it. There
  is a scrollbar now when there needs to be one.

#### Known
- The right-hand-edge problem reported earlier could not be reproduced: the
  application was measured at 100 %, 130 % and 200 % across seven screens, in a
  window the size of the one it was reported on, and nothing runs off the edge.
  The two earlier attempts to fix it were both aimed at the wrong thing and
  both made it worse; they are gone. If you still see it, say so — there is a
  measuring tool now, and it beats another screenshot.
- The sidebar keeps its full width at large Application Sizes, where it was
  meant to shrink to icons. It costs room; nothing is unreachable.

### The Hard Disk screen opens a PiStorm card (2026-08-13)

#### Added
- **Open a card and see what is on it.** A PiStorm SD card is not one hard
  disk: it is a partition table with a FAT32 boot partition and one to three
  Amiga disks inside it, each with its own partitions, and ART now shows it
  that way — the four slots as the card's own documentation numbers them, then
  a section per Amiga disk with where it starts and what is on it.
- **The file system drivers are counted across the whole card**, which is what
  the Amiga does. A card whose second disk carries no PFS3 while its partitions
  are all PFS3 works perfectly, and ART no longer has any way to call those
  partitions broken.
- Any file can be opened this way: a plain hard disk image comes back as one
  disk starting at the beginning, exactly as before. Nothing depends on the
  file's extension.

Reading only — ART cannot write a card yet, and the screen says so rather than
leaving you to find out.

### Six from the open list (2026-08-13)

#### Fixed
- **Planning a batch of `.lha` archives no longer freezes the window.** ART has
  to unpack every archive to tell you what each one will create, and it used to
  do that with the interface stopped dead — no progress, no Stop, in the very
  step that exists so you can change your mind. It runs as a job now, with a
  bar and a Stop button like everything else.
- **Cancelling a copy into a large hard disk image says how many files already
  landed**, instead of just "cancelled". They are correctly still there: a large
  image is written file by file, each one finished before the next starts, so
  stopping cannot take back what is done. A small image is written whole and
  cancelling really does leave nothing — that still says plain "cancelled".
- **After a copy, the keyboard stayed where you were.** F5 refreshes the
  destination pane when the job finishes, and that used to move the keyboard
  there — so the next F-key press acted on the pane you were not looking at.
- **Stop now works in the middle of an archive**, not only between two. A batch
  of five whose third one is large used to sit there ignoring Stop for as long
  as that archive took to unpack.
- A pane with a filter that matches nothing says so. It could, in principle,
  have said "this folder is empty" instead — which reads as ART having failed
  to open the disk rather than as a filter doing its job.

### What you have open stays open (2026-08-13)

#### Fixed
- **Leaving a screen threw away the file you had open.** Step from ADF Studio
  over to the hard disk screen and back, and the floppy you were working on was
  gone — while the Dashboard's Recent list still had it at the top. Now it is
  still there: the ADF, the HDF, the archive, the file in the hex viewer, the
  package and target a WHDLoad install was being set up with, and the disks
  attached in the WinUAE screen. Closing ART still starts you fresh.
- The file is read again when you come back, so one that changed on disk in the
  meantime shows what it says now rather than what it said when you left.
- Turning on ADF Studio's hex panel used to switch itself back off every time
  the disk was loaded again.

### Hard disks ART creates now describe themselves the way real ones do (2026-08-13)

#### Fixed
- **A partition ART created told the driver nothing about how to transfer to
  it.** Two fields — the largest transfer it will accept, and which memory it
  may transfer into — were left at zero, and zero in the second one means "no
  memory is acceptable". Nothing refused it in practice, because the PiStorm's
  Emu68 is forgiving, but it is the kind of thing that is impossible to
  diagnose from the symptom. Both now carry the values every partition on two
  real cards uses.
- **The cache size was decided by a number typed into a screen.** New
  partitions were created with 100 buffers whatever the engine thought,
  because the Hard Disk studio was sending its own figure — and the studio has
  never asked you for one. It no longer names a number at all; the engine's
  600 (300 KB, on a machine with a PiStorm's hundreds of megabytes in it) is
  what you get, and a partition made before this keeps whatever it was made
  with.

### About, and the name on the installer (2026-08-13)

#### Fixed
- **The PiStorm screen no longer goes grey without saying why.** Everything on
  it edits files on a card, so everything waits for you to choose the card's
  folder — it says that now, instead of leaving you with buttons that look
  broken.

#### Known
- Above 100 %, Application Size still cuts the right-hand edge off. Two
  attempted fixes made it worse and were reverted; it is being tracked rather
  than guessed at again.

#### Added
- **An About panel in Settings**: the version the build actually carries, the
  author, where the source lives, the licence, and the notice the GPL asks an
  interactive program to display — that it comes with no warranty and that you
  are welcome to redistribute it. Static: no version check, no update button,
  nothing that reaches the network.

#### Fixed
- The installer's Publisher line said nothing; it says `tolon` now, as do the
  package metadata and the copyright the bundle carries.
- The licence inventory had been listing ART itself as MIT since months after
  the project moved to the GPL.

### ART can read a PiStorm card (2026-08-13)

#### Fixed
- **ART could not open a real PiStorm card at all.** It looked for the Amiga's
  partition table in the first few blocks of the file; on every card those are
  the MBR and the FAT32 boot partition, and the Amiga's own table is about a
  gigabyte further in. Two real distributions were read to find this out, and
  both now open.

#### Added
- **A card can hold more than one Amiga disk**, and ART reads them all.
  MultibootOS keeps two, with different geometries and seventeen partitions
  between them.
- **A filesystem driver belongs to the card, not to one of its tables.** One of
  those two tables carries no PFS3 while every partition in it is PFS3 — and
  the card works perfectly. Asking the wrong question would have told you
  fifteen working partitions were broken.

A plain HDF still reads exactly as before. Reading a card from the Hard Disk
screen is not wired up yet — this is the engine underneath it.

### Two that were owed (2026-08-13)

#### Fixed
- **A write-protected file is no longer replaced without a word.** The delete
  half of this landed yesterday; the *overwrite* half is what AmigaDOS actually
  governs with the W bit, and nothing checked it. Fixing it also caught a side
  effect of yesterday's change: copying over a file that was delete-protected
  but perfectly writable had started being refused for the wrong reason.

#### Added
- **A named firmware set can be deleted**, and ART keeps a copy when it goes.
  The set the card is currently running is refused — deleting that one would
  take away the only copy of the configuration it boots from, so make another
  active first.

### Four fixes (2026-08-13)

#### Fixed
- **A file the Amiga itself protects is no longer deleted without a word.**
  ART asked, but only on one screen — the writer underneath removed it either
  way. It refuses now, names the file, and only goes ahead when you have
  actually been asked. Moving one asks *before* the copy, so a refusal cannot
  leave you with a duplicate and an error.
- **`Docs` and `docs` are the same drawer on an Amiga**, and ART now says so up
  front — "rename one of these" — instead of quietly dropping the second one
  and its whole contents.
- **A selection of nothing but shortcuts said it had copied everything.** It
  had copied nothing. The report says which ones it declined, and why.
- **"1 weeks ago"** in the Aminet listing.

### The OS Builder knows the distributions (2026-08-13)

#### Added
- **A new screen: OS Builder.** The AmigaOS distributions that run on a
  PiStorm — CaffeineOS, CoffinOS, AmiKit, and ART's own Baseline — each one
  leading with its licence, because that decides what you have to do before ART
  can help. It tells you what you must supply, checks whether the Kickstart you
  picked is from the family that distribution's base actually wants, and
  whether your card is big enough — before a copy, not two thirds of the way
  through one.
- **ART downloads no distribution, and never will.** The link goes to the
  project's own page; you come back with a file. The same rule ART already
  follows for Kickstart ROMs. And both free distributions ship the same
  sentence, so ART does too: if you bought this from somebody, ask for your
  money back — building the card yourself is what this screen is for.

#### Not built
ART cannot prepare a card yet, and every profile says so. What a real
distribution's card looks like has to be read off a genuine download first;
guessing at it is how a tool ends up writing a card that quietly does not boot.

### The PiStorm screen knows which Kickstart you have (2026-08-13)

#### Added
- **Every ROM on the card is identified, not just listed.** ART checks it by
  checksum and tells you which Kickstart it is, its revision, and the machines
  it is for. A ROM it does not recognise is *labelled* and stays perfectly
  usable — it may be custom, byte-swapped, or newer than ART's table.
- **Choose a Kickstart from anywhere on your PC** and ART copies it onto the
  card under a name you confirm. It never replaces a file without being asked,
  and the one it replaces is kept.
- **The kernel says which Emu68 it is**, read from the version string Emu68's
  own build puts in the image. If the image says nothing, so does ART.
- **Named firmware sets**, the way MultibootOS does it: one `config.txt` per
  system, created from the card or from the settings on screen, duplicated,
  renamed, and made active — each one showing you the change first.

#### Fixed
- **ART named an Emu68 download that has never existed.** For the PiStorm16 it
  said `Emu68-pistorm16.zip`; no Emu68 release has ever contained a file by that
  name. Worse, the name that *does* exist means different things in different
  releases: `Emu68-pistorm.zip` is the classic board's firmware in 1.0.x and the
  PiStorm32-lite's and PiStorm16's in the 1.1 alpha. ART now asks which release
  line you are building for, and says plainly when a board has no download in it
  at all — as the PiStorm16 has none in any stable release.
- A card forcing an HDMI mode ART had no name for selected nothing at all, and
  saving would quietly have removed the forcing.
- The Kickstart picker was shown to everyone *except* Power Users. Power Mode
  now adds free typing beside it instead of taking it away.

#### Not built
Fetching a kernel update from GitHub. Named in the issues list rather than
offered as a button that does nothing.

### The PiStorm screen tells the truth now (2026-08-12)

#### Fixed
- **Four controls that did nothing are gone**, because the things they claimed
  to switch do not exist: Emu68 *is* a JIT engine and cannot be turned off; it
  emulates no MMU, so WHDLoad runs in NOMMU mode; it maps Fast RAM itself, so
  there is no size to set; and the storage driver is called `brcm-sdhc.device`
  or `brcm-emmc.device`, never `emu68-sd.device`. ART was writing three options
  Emu68 has never read. A card written by an older ART has them removed the
  next time you save.
- **The profile cards no longer quote numbers nobody measured.** Each one now
  lists the exact options it sets, and you can read them before you apply it.

#### Added
- **Your hardware is three answers, not one**: which Amiga, which PiStorm
  board, which Raspberry Pi — each narrowing the next. ART then tells you which
  Emu68 build to use, what your storage driver is called, and what is worth
  knowing about that combination, from the CM4's eMMC to why a poor power
  supply looks exactly like a slow PiStorm.
- **Every option, with the name of what it writes** beside it, and the whole
  `cmdline.txt` line beneath. Your own boot parameters are shown too,
  read-only — so you can see for yourself that they survive.
- **Show me the change** before saving: both files, before and after, in full.
- Options that mean nothing on your machine are not shown. The slow-RAM ones on
  an A1200 are not a harmless extra — they are the documented cause of a wrong
  memory report.

Not yet booted on real hardware. What ART writes is what the Emu68
documentation says; whether a given machine likes a given set of options is a
separate claim, and not one ART is making.

### The whole program can be made bigger, and it remembers everything (2026-08-12)

#### Added
- **Application Size.** One control in Settings scales the *entire* program —
  text, search boxes, icons, buttons — on every screen, from 70 % to 250 %.
  Ctrl and the mouse wheel does the same anywhere outside the file listing,
  along with Ctrl+plus, Ctrl+minus and Ctrl+0. The listing keeps its own text
  size, because it is the one wall of text dense enough to want its own answer.
  Most of the people this program is for are past fifty and wear reading
  glasses; this is not a preference.
- **Every choice you make is remembered.** Not just the Files screen's tabs:
  the Collection's view and filters, Aminet's sort, narrowing and download
  subfolder, the HDF wizard's size, template, filesystem and driver, PiStorm's
  configuration, the Gotek folder, your WinUAE machine and ROM, how ADF Studio
  makes disks. Nothing changes unless you change it.

#### Fixed
- A setting changed in the first moment after launch could be silently undone
  by the settings file arriving a beat later.

### A PFS3 disk ART makes now actually mounts (2026-08-12)

#### Added
- **The New HDF wizard carries the filesystem.** Pick PFS3 or SFS and ART asks
  for the driver — `pfs3aio`, usually — and writes it *inside* the disk, where
  Kickstart looks for it. Until now ART wrote the name of a filesystem it did
  not put there, and an Amiga ignored the partition in silence (ART-084).
- **ART reads the version off the driver itself**, out of the `$VER:` string
  every Amiga binary carries, so there is no number to type. If the driver says
  nothing, ART asks rather than guessing: a version of 0.0 loses to whatever is
  already loaded, and the driver would never run.
- **The Hard Disk studio lists what a disk carries**, and names any partition
  whose filesystem is not there — the same question, asked of a disk somebody
  else made.
- **The Aminet download folder is in Settings now**, beside the other paths and
  with the same Browse button. ART checks the folder before remembering it, and
  tells you where downloads are actually going.

#### Verified
- Round-tripped through **amitools**, not just through ART: `rdbtool` reads an
  image ART built and reports `PDS3 version=19.2 size=59120`, and pulls the
  driver back out byte-for-byte identical to the file that went in.
  `hst-imager` reads the same image. ART's own reader agreeing with ART's own
  writer would have proved nothing.

### A real Amiga booted a disk ART wrote (2026-08-12)

#### Verified
- **On real hardware, not an emulator.** A disk ART made — boot code and all —
  was put on a **Gotek** and started an **Amiga 500 / 500+** with **Kickstart
  3.9** from cold, straight to an AmigaDOS prompt. Every earlier check ran
  under WinUAE with licensed ROMs; this one is real silicon, and it is the
  first time ART's own boot code has run on a real 68000.
- **Still not claimed:** a Gotek is not a mechanical drive. Nothing ART writes
  has yet been put on a physical floppy and read back by a real drive head.

### Enter opens the disk (2026-08-12)

#### Added
- **Press Enter on an ADF, and you are inside it.** The pane becomes the disk —
  path bar and all — and Backspace brings you back out with the cursor sitting
  on the file you came from. The same for HDF partitions, CD images, LHA / ZIP
  / 7z archives and Commodore disks. What a file *is* comes from its bytes, so
  a floppy image called `.img` opens as a floppy. Alt+Left and Alt+Right walk
  the history, and going back into an image lands you in the folder you left.
- **Tabs, one row per pane.** Ctrl+T for another, Ctrl+W to close, Ctrl+Tab to
  cycle, middle-click to close. A tab can live inside a disk image. **Your
  tabs, paths, sort orders and filters come back when you reopen ART.**
- **The keyboard reaches everything.** Space marks where you are, Insert marks
  and moves down, the numpad marks by pattern and inverts, and simply typing a
  name jumps the cursor to it. F2 refreshes, Alt+F1 and Alt+F2 open the source
  boxes.
- **File colours you choose.** Settings takes a list of "pattern → colour",
  first match wins, and ART starts you off with three: the things it can open,
  the things it can unpack, and ROMs.
- **A history on the command line**, and a confirmation before deleting a file
  the Amiga itself protects.

#### Known limitations
- Space marks a folder but does not add up what is inside it yet.
- The delete-protection warning is a question, not a lock: only the file
  manager asks.

### The Files screen loses its clutter, and F6 means Move (2026-08-12)

#### Added
- **F6 moves.** It copies first, then goes and looks in the destination for
  every name that was supposed to land there, and only then removes anything
  from the source. Stop it halfway and the worst you are left with is a
  duplicate — never a missing file. Shift+F6 still renames in place.
  If a name is already taken at the other end, the move is refused outright
  rather than offering to overwrite or to skip: skipping would leave the copy
  undone and remove the original anyway. Icons come along, if you say so —
  a drawer that arrives without its `.info` is invisible on Workbench.
- **Each pane header is a source box, a path and a filter.** The source box
  lists the drives you actually have — nothing is assumed — plus Folder, ADF,
  HDF, Disc, Archive and C64. The old row of buttons is a Settings switch now,
  off unless you turn it on.
- **The command line works.** Type a full path to go there, `cd ..` to go up,
  or a mask like `*.adf` to filter the pane. It does not run programs, and it
  says so plainly rather than doing nothing.
- **F2 and Ctrl+R re-read a pane.**

#### Changed
- **One line of status, at the bottom.** The red errors, the green messages
  and the "busy…" line used to stack above the panes and shove them down the
  screen; they are one row in the bottom bar now, and the panes never move
  because something had to be said.
- **"Both panes are local folders" stopped shouting.** It is ART declining a
  question, not something breaking, and it no longer looks like a failure.
- **The "when a name is already taken" row is gone from the screen.** The copy
  dialog asks when there is actually a name in the way, and Settings holds
  what it starts from.
- **The function keys are one row that stays one row.** At narrow widths the
  labels give way to the key names instead of wrapping onto a second line.
- **The selected-items line merged into each pane's own status line**, which
  already counted them.

#### Known limitations
- Nothing can be moved *off* a folder on your own disk: ART does not delete
  files there, by design. F6 says so and points at F5.
- Between two images, F6 moves a folder; a single file has to be copied for
  now.

### ART opens CDs, ZIPs, 7z archives and C64 disks — and decides what a file is by looking inside it (2026-08-12)

#### Added
- **A file is what it actually is, not what it is called.** ART now reads the
  first bytes of anything you drop on it and decides from those. An `.img`
  that holds a floppy is a floppy, an `.adf` that holds a CD image is a CD
  image, and an archive somebody renamed to `.dat` still opens. The extension
  is a fallback for files nothing else recognised.
- **CD images open in the file manager.** ISO9660 with Joliet names, including
  raw 2352-byte track dumps in both Mode 1 and Mode 2/XA — the layout CD32 and
  mixed-mode discs use. Walk the disc, copy a folder out to your disk, or copy
  it straight into an Amiga volume.
- **Archives open in the file manager too — LHA, ZIP and 7z.** An archive has
  no folders of its own, only names like `Tools/Shell.lha`, so ART builds the
  folders from the names and lets you walk into them. Copy a folder out to
  your disk, or into an Amiga volume, where it is unpacked and written through
  the same tested writer an install already uses.
- **Commodore 64 disks and tapes.** `.d64`, `.d71`, `.d81` and `.t64` open as
  panes and their files copy out to a folder, with the Commodore file type as
  the extension. 40-track disks (SpeedDOS, DolphinDOS) are read as well as the
  usual 35-track ones. `.tap`, `.prg` and `.crt` are identified and described:
  a TAP holds a tape signal with no directory inside it, so there is nothing
  to list and ART says so rather than pretending.
- **A disc, an archive and a Commodore disk are all read-only, and say so.**
  Each refuses writes with its own sentence instead of a pane that quietly
  does nothing.

#### Fixed
- **A real LHA archive was only ever recognised by its file extension.** The
  signature was being looked for in the wrong place, so an archive renamed to
  something else came back as "unknown" — and the test that was supposed to
  cover it used a fixture no LHA tool would produce.
- **"Open in the file manager" opened nothing.** Choosing that action from the
  drop panel took you to the Files screen and left whatever was already there;
  the screen never read the file it had been sent.
- **A raw CD track in Mode 2/XA was not recognised**, and would have been
  misread by two layers at once if it had been.
- **Copying a folder out of an image** now builds the destination path itself
  instead of accepting one built for it, and honours your overwrite choice on
  a disc as well as on a floppy. Picking a single file out of a disc copies
  that file, not the folder around it.

#### Changed
- **Machine profiles cover the whole classic line** — A1000, A500, A500+,
  A600, A2000, A3000, A1200, A4000, CDTV and CD32 — rather than six of them.
- Building ART now needs **Rust 1.93** (it was 1.77), so that 7z support uses
  a maintained decompressor rather than one several years behind.

### The Files screen became a real commander, and ART writes a disk an Amiga boots (2026-08-11)

#### Added
- **Select several things at once.** Shift-click a range, Ctrl-click to add or
  remove one entry, Insert to mark it and move on, Ctrl+A for everything in
  the pane. Tab moves keyboard focus between the two panes, and focus is now a
  real, visible thing rather than wherever a selection happened to be.
- **Copy or delete a whole selection in one step.** Copying several files and
  folders from your disk into an image, or from an image onto your disk, and
  deleting several entries from an image, are each one operation: a cancelled
  batch copy commits nothing, and a batch delete that cannot fully go through
  — a name that is not there any more, a folder that still has something in
  it — deletes nothing rather than half the selection.
- **Drop several `.lha` archives on a disk at once.** Each gets its own
  drawer, the way installing one archive already worked, staged into a single
  write so a cancelled multi-archive install cannot leave two games
  half-installed.
- **One sort order everywhere, and you can change it.** Local folders, ADF
  volumes and HDF partitions used to list their contents three different
  ways — one of them not sorted at all. All three now share one floor
  (folders first, name otherwise) and clicking a column header in the Files
  screen sorts by name, size or date, on top of that floor, per pane.
- **A filename filter**, Total Commander-style: type `*.info` or `Read?e.txt`
  in the path row and the pane narrows to what matches. Changing the filter
  clears the pane's selection, so a selection never quietly keeps an entry
  the filter just hid.
- **The Files screen looks like a file manager now** — restyled after two
  Total Commander reference screenshots, with file-type icons and colour and
  an Attr column showing each entry's protection bits at a glance.
- **ART writes a disk a real Amiga boots from.** The "bootable" option used
  to write a boot block that looked valid to every check ART or an
  independent tool (`xdftool`) could run, and did nothing when a real machine
  tried to use it — it returned immediately instead of handing control to
  AmigaDOS. ART now assembles real 68000 boot code from the documented
  Kickstart contract, and a disk made with the option checked boots to a
  command line. (It boots the disk, not Workbench — `Startup-Sequence` and
  the commands behind it are AmigaOS content ART does not ship.)

#### Verified
- **For the first time, on hardware rather than only in tests.** Under a
  licensed Kickstart and Workbench (Amiga Forever, WinUAE — not bare metal;
  that came later, on 2026-08-12, see above), in both an Amiga 1200 and an
  Amiga 500+ configuration: a disk ART wrote
  mounted, its one file listed and read back correctly, and a second disk —
  identical except for its boot code — booted. Two earlier checks already
  existed (ART agreeing with itself, and with an independent implementation,
  amitools); this is the first time an actual Amiga has been asked.

#### Known gaps, recorded rather than hidden
- **Volume-to-volume multi-select refuses** rather than batching — copy one
  at a time between two images for now (ART-064).
- **Volume-to-local multi-select works, but as several operations running
  together**, not the one atomic operation the other three copy directions
  give (ART-065).
- Installing several archives plans the whole batch before showing you
  anything, and Stop cannot interrupt one archive's own extraction — only
  between archives (ART-066, ART-067).
- **Nothing in this release has been looked at on a running screen.** Every
  change above is covered by an automated test and nothing more — including
  the restyled Files screen, which nobody has opened yet — and no language
  has been checked visually either (carried over from the previous release,
  ART-062).

### ART now speaks Turkish (2026-08-10)

#### Added
- **Turkish.** Every screen, every shared dialog, and every message ART
  builds in the frontend now has a Turkish translation alongside the English
  one. Switch languages in Settings — the choice is remembered across
  restarts, and each language names itself in its own tongue in the switcher
  (`English` / `Türkçe`).
- A build-time check fails if the two language files ever drift apart: a
  missing key, an empty translation, or a Turkish string that drops a
  placeholder the English one uses all now stop the build rather than
  shipping a half-translated screen.

#### Changed
- **Error messages coming from ART's Rust core — including the WHDLoad
  install screen's refusal text — are still English**, regardless of which
  language is selected. Translating those means either moving them into the
  frontend's language files or giving the Rust core its own, and that
  decision has not been made yet (tracked as ART-060).

#### Removed
- **Nine code paths nothing in the application could reach were deleted** —
  an old ADF-extraction command, a folder-copy planner and a raw
  block-writing command superseded by the current file-manager writer, a
  background-extraction command with no caller, and a placeholder screen. No
  user-visible behaviour changes; these were unreachable before this release.

### Stopping an install now really stops it (2026-08-10)

#### Fixed
- **Cancelling a WHDLoad or Aminet install leaves the disk untouched.** It used
  to write whatever had been copied so far, then report the install as
  finished — so a game could end up on the disk without the one file that
  starts it, with nothing saying anything was missing. Cancelling now ends as
  "cancelled", and the image is exactly as it was.
- **"ART will not install this" no longer contradicts itself.** When an archive
  turns out not to be a WHDLoad package, the screen no longer offers to create
  a drawer with no name and write nothing into it, or complain that the package
  has no icon. And the advice under the refusal now fits the reason: copying it
  by hand is suggested only when that would actually help, rather than to
  someone whose disk is full.
- **The sidebar no longer cuts the page off on a short window.** With the
  window shorter than the full list of tools, the bottom of every page — and
  the bottom of its scrollbar — was clipped. The tool list now scrolls on its
  own instead of stretching the window.

### A write that would break a disk image is refused (2026-08-10)

#### Fixed
- **The finished image is checked before it replaces your file** — its
  bootblock, checksum, block count and root block. ART checked only the
  blocks an operation touched, so a change that got those four things wrong
  on the disk as a whole was written out anyway. Now they are checked on the
  whole result first, and if it does not hold up nothing is written — the
  file on disk is left exactly as it was, and the message says so. This does
  not yet check the free-space bitmap or a directory's hash chains, so a
  double allocation can still commit undetected.
- **HD floppies and hard disk images are measured against themselves.** ART's
  health check compared every image with a standard 880 KB floppy and flagged
  anything else as suspect. It now reads each image's own geometry, so a 1.76 MB
  floppy or a hard disk image is no longer reported as odd — and is not refused
  by the check above.

### The window scrolls, and ADF Studio opens real disks (2026-08-10)

#### Fixed
- **ADF Studio could not open a bootable disk.** It looked for the root block
  in a place the Amiga does not keep one — in the boot code itself — so any
  disk that could actually start a machine was refused as damaged. It now
  works out where the root block is, the way every other Amiga tool does.
- **1.76 MB disks now work**, and report their real size rather than half of
  it.
- **Long pages scroll.** The window was cutting content off at the bottom with
  no scrollbar at all.
- **The window scales.** Narrow it and the sidebar collapses to icons instead
  of squeezing the panes; widen it and content stays centred instead of
  clinging to the left.
- **Buttons that cannot be used now look it.** A greyed button is a button you
  know not to click. The folder names and the path bar in ADF Studio follow the
  same rule: while an operation is running they no longer look clickable.
- **"ART will not do this" no longer looks like a crash.** Being told an
  archive is not a WHDLoad package is an answer, not a fault, and it no longer
  arrives in red with an error code.
- **Installing a WHDLoad game or an Aminet package now refuses up front if it
  will not fit**, instead of discovering that partway through and leaving
  whatever landed — a game missing its `.slave` because it did not fit used to
  be a broken result with no warning.
- **A write could very rarely be reported as failed right after it succeeded.**
  Confirming a write by re-opening the file could race an external program
  (an antivirus scan, a search indexer) briefly locking it the moment ART let
  go. Confirmation now happens without releasing the file first, so that race
  is gone.

#### Changed
- **The second AmigaDOS writer is retired.** ADF Studio and the two-pane file
  manager now go through exactly one writer, so they can no longer disagree
  about what is on a disk. Everything it did — five previously-fixed defects
  among them — moved to the surviving writer with its tests; nothing was
  quietly dropped.

### Put a WHDLoad game on a hard disk, in one step (2026-08-09)

#### Added
- **Install WHDLoad** — a new screen. Point it at a `.lha` and a hard disk
  image, and it puts the game on the disk: the drawer, everything in it, and
  the icon that makes it show up on Workbench.
- It **checks before it writes**. What it found in the archive and how sure it
  is, which drawer it will create and where, how many blocks that needs and how
  many are free. The button is only live once all of that is settled.
- It **refuses rather than guesses**, and says which of these it is:
  the archive does not look like a WHDLoad package; it holds an `Install`
  script, so the game has not been installed yet and needs an Amiga to do it;
  a game of that name is already on the disk; it will not fit; it holds names
  AmigaDOS cannot store; or the archive did not unpack completely.
- The readme that ships alongside a package is **not** installed — it is not
  part of the game — and the screen says so rather than leaving you to notice.
- Dropping a `.lha` on ART now offers this as the first suggestion.

### The Files screen writes (2026-08-09)

#### Added
- **Hard disk images are no longer read-only.** Copy into a partition, rename,
  move, delete, make folders, edit attributes — the same operations a floppy
  has always had, on any volume ART can read.
- **Function keys, the way a file manager should have them.** F3 view · F4 edit
  · F5 copy · F6 rename · F7 new folder · F8 delete · F9 attributes, on the
  keyboard and on a bar under the panes.
- **Copy between two disk images**, in either direction, with every file
  checked after it lands.
- **Before a folder copy, ART tells you what it will cost**: how many blocks,
  how much room is left, which names AmigaDOS cannot store and what it would
  call them instead, and which names are already taken. One report, one
  decision — instead of a copy that stops on file 37 and again on file 52.
- **Edit a file inside an image (F4).** It comes out to a working copy, your
  own editor opens it, and you put it back when you are done. Nothing is
  written back unless the file actually changed, so opening a file and closing
  it cannot touch the image. If a Windows editor saved Windows line endings
  into an Amiga text file, ART offers to convert them — and never does it
  without asking.
- **Protection bits and comments are editable** (Power User Mode), with a line
  explaining what each of the eight does. They are also preserved when you copy
  files out to your disk and back, through a small `.uaem` file next to each
  one — the same format WinUAE uses, so folders round-trip between the two.
- **Icons travel with what they belong to.** Renaming or deleting `Game` offers
  to do the same to `Game.info`, and a copy warns when a folder is going in
  without its icon — which is what makes a drawer invisible on Workbench.
- **Install an Aminet package into a hard disk partition**, not just a floppy.
- **Free space, the volume name and the filesystem** are always in the pane
  footer. A volume ART will not write to keeps a lock badge and says why on
  hover, rather than a pane that quietly refuses everything.

#### Changed
- **A large image is no longer copied to back it up.** Floppies and small
  hardfiles keep the pipeline they always had — read whole, verify, replace
  atomically. Anything above 16 MB is written in place, with a record of every
  block before it changes, so a rename on a 2 GB disk takes a moment instead of
  minutes.
- **If ART is interrupted mid-write, it says so the next time you open the
  image** and offers to put it back exactly as it was. Until you decide, that
  image is read-only.

#### Fixed
- **Names with accented characters were refused as too long.** ART counted the
  bytes a name takes in its own memory rather than the characters AmigaDOS
  stores, so a perfectly legal name like `Grüße vom Süden` was rejected — and
  the more accents, the worse the count.

### Whole folders, Aminet settings that stick (2026-08-09)

#### Added
- **Copy a whole folder into an ADF.** Subfolders are recreated on the disk,
  and you are told up front how many files and how much space it needs. Names
  a floppy cannot hold, symlinks and anything nested too deep are listed as
  skipped rather than quietly dropped.
- **"Show in Collection"** on a downloaded Aminet package indexes your download
  folder in the Collection screen.
- **The mirror list is editable** in Power User Mode: reorder it, change an
  address, add or remove one, or go back to the mirrors ART ships with. Mirrors
  are tried top to bottom, so putting the one nearest you first makes syncing
  faster.

#### Fixed
- **Your Aminet download folder is remembered.** It was kept only while the app
  was running, so every restart quietly went back to the default — downloads
  then landed somewhere other than the folder you had chosen.
- **A custom mirror order is remembered too**, for the same reason.

### Hard disk images can be browsed — and four compatibility bugs fixed (2026-08-09)

#### Fixed
These four made ART's disks unusable outside ART. All were found by checking
ART's output with `amitools`, an independent implementation; none was visible
to ART's own tests, because its reader and writer shared each mistake.

- **Every ADF ART wrote had invalid checksums** (ART-033). AmigaDOS uses one
  algorithm for the boot block and a different one for every other block; ART
  used the boot block's everywhere. AmigaOS and WinUAE rejected the result while
  ART called it healthy.
- **Files added to a disk were zero bytes on a real Amiga** (ART-034). The file
  header's size field was written two longwords early, into unused space.
- **The free-space bitmap was laid out the wrong way round** (ART-035). AmigaOS
  would have treated ART's occupied blocks as free and written over them.
- **RDB partitions reported no filesystem** (ART-032). DosType and boot priority
  were each one longword early.

If you created disks with an earlier build, re-create them: the images
themselves are wrong, not just how ART reads them.

#### Added
- **Hard disk images open in the Files screen.** An HDF shows its partitions;
  open one and browse it like a floppy, with folders and file sizes, and copy
  files out to your disk. Bare hardfiles without a partition table work too.
- Partitions ART cannot read — PFS3, SFS, long filenames — are still listed with
  their name, size and the exact reason, so a healthy disk never looks broken.
- A partition that claims more space than the file holds is read as far as it
  really goes, and says so.
- Copying *into* a hard disk image is not implemented and refuses with the
  reason rather than appearing to work.

### Files: a two-pane manager, and more control over downloads (2026-08-09)

#### Added
- **Files** — a new screen with two panes, Norton Commander style. Each pane
  independently shows a local folder, an ADF image or an HDF image. Copy
  between local folders and floppy images in both directions, by button or by
  dragging; make folders and delete files inside an image; and **drag a file
  out to Explorer**. Every write to an image goes through the existing
  backup-and-validate pipeline, so the previous image is always kept.
- The **HDF pane shows partitions, sizes and filesystem types — not files**,
  and says so. ART cannot read inside a hard disk image's partitions yet, and
  an empty file list would have implied the disk was empty.
- **Aminet: sorting and filters.** Order by best match, newest, oldest,
  largest, smallest or name. Narrow by name-only search, upload age, size range
  and file type.
- **Aminet: you choose where downloads go.** Pick a download folder, put each
  package in a subfolder of your choosing, and move it later. Files keep their
  real names; the verified copy still lives in the cache so nothing is
  downloaded twice.
- **Aminet: install a downloaded package into an ADF.** It refuses up front if
  it will not fit, with real numbers, rather than failing part-way. "Install to
  HDF" is shown as *coming later* rather than hidden.

#### Fixed
- The Aminet screen could be left waiting forever if a sync or download failed:
  the button stayed disabled and a readme spun indefinitely. It now watches the
  job's own outcome and shows the error with its ID.

### Aminet — Software Sources Engine, Stage A (2026-08-09)

#### Added
- **Aminet Studio** (spec §41.5), a new screen. Sync the Aminet catalogue once
  and search, browse by category and read package descriptions **entirely
  offline** — 85 000+ packages, no connection needed after the first sync.
  Downloading fetches into a cache that lives outside every disk image;
  installing remains a separate step you ask for.
- Every download goes through the §41.5.3 trust pipeline: size check against
  the catalogue, SHA-256, and structural validation with the existing LHA
  engine. A package that fails any gate is discarded rather than cached.
- Sync, download and readme all run as background jobs, so the window never
  waits on a mirror. Mirrors fail over in order, and the error names every
  attempt rather than saying "download failed".
- Package facts carry their provenance: a version read from a readme is
  labelled as such and rated lower than one from the index, and a `Requires:`
  line is shown as the readme's claim, not as something ART checked (§14, §34).
- Two error IDs: `ART-MIRROR-UNREACHABLE`, `ART-INTEGRITY-MISMATCH`.
- Power User Mode shows the mirror order, index path, cache location, download
  hashes and the sync's unreadable-line report. Beginner Mode hides all of it
  and works identically.

#### Fixed
- **LHA archives with level 2 or 3 headers could not be opened at all**
  (ART-031). That is the format most modern tools write and what Aminet hosts,
  so LHA Studio could not list or extract a typical Aminet download. Level 0
  and 1 archives are unaffected and their filenames are unchanged.
- Mirror failover could concatenate a failed mirror's partial response onto the
  next mirror's (ART-030), producing a file assembled from two sources that
  parsed and hashed cleanly. A truncated response is now refused outright.

#### Notes
- A sync that comes back too damaged to trust **keeps your existing catalogue**
  and says so, rather than replacing it with a short one.
- Tests: 180 → 326 (368 after the Files work).

### Background jobs & Beginner/Power mode (2026-08-09)

#### Added
- **Job Queue** (spec §54, §55). Long operations run on background threads and
  report progress the UI can watch, with a Stop button. Collection scanning and
  LHA extraction are jobs; a global job bar in the app shell keeps work visible
  from any screen. Cancellation is checked between whole units of work, so
  stopping never leaves a half-written file.
- **Beginner / Power User mode** (spec §47, §48) is now actually applied. In
  Beginner mode the raw-data studios (Hex Tools, PiStorm), the Advanced action
  group and block-level numbers are hidden. Nothing is disabled, and Settings
  explains what the switch changes.
- `CoreError::Cancelled` (`ART-CANCELLED`) so a stopped operation reads as
  cancelled rather than as a failure.

#### Changed
- `collection_scan` returns a job id and delivers its titles in a
  `collection-scan-result` event instead of blocking until the walk finishes.
- New `lha_extract_job` runs extraction in the background; the synchronous
  `lha_extract` remains for small archives.
- Tests: 169 → 180.

### Operation Log & error IDs (2026-08-09)

#### Added
- **Operation Log** (spec §53). Every operation that changes user data is now
  recorded: what was done, the source and destination, where the previous
  version was backed up, whether verification passed, and — on failure — the
  error ID. Stored as append-only JSON Lines beside the application log.
- Settings gains an **Operation Log** section listing recent operations and
  exporting the full history as readable text.
- **Error IDs** (spec §68). Every error carries a stable `ART-*` identifier
  (`ART-SAFETY-REFUSED`, `ART-FORMAT-MALFORMED`, …) shown to the user and stored
  in the log, so a failure can be quoted rather than described.
- `OperationOrigin` distinguishes user actions from workflow runs and, ahead of
  §45.5, from AI-generated plans.

#### Changed
- Command errors reaching the frontend now end with `Error ID: ART-…`.
- Tests: 157 → 169.

### Hard disk & collection audit (2026-08-09)

#### Fixed
- **Creating or opening a hard disk image allocated the whole thing in memory.**
  A 4 GB HDF needed 4 GB of RAM. Images are now created sparsely (only the RDB
  blocks are written, then the file is extended), and opening one reads a 1 MB
  header window instead of the entire file.
- **Creating an image could silently destroy an existing one.** Both the HDF and
  ADF creation paths called `fs::write`, replacing whatever was already there.
  Creation now refuses to overwrite and cleans up a partially written file.
- **RDSK blocks described a zero-capacity disk.** `HiCylinder` and `CylBlocks`
  were never written and other logical-drive fields were at the wrong offsets,
  so disks ART created were self-consistent but wrong for AmigaOS and every
  other Amiga tool. Verified against `amitools`.
- **Empty RDB block lists pointed at block 0.** `BadBlockList`,
  `FileSysHeaderList` and `DriveInit` now use the `-1` sentinel; `BlockBytes`
  is set to 512.
- **Oversized partitions were silently truncated** and could leave the partition
  chain pointing at an unwritten block. Impossible layouts are now refused with
  the sizes involved.
- RDB checksums honour the block's own `SummedLongs` instead of assuming 128.
- Folder scans are depth-limited and no longer follow symlinks — a cyclic
  Windows junction used to overflow the stack and close the application.
- A request for an absurdly small image aborted the process instead of erroring.
- ROM identification checks file size before reading.

#### Changed
- `create_rdb_image` → `create_rdb_layout`, returning the leading blocks plus
  the intended total size rather than a full image buffer.
- Tests: 143 → 157.

### Data-safety & correctness hardening (2026-08-09)

#### Added
- **`core/safety`** — the single gate every write now passes through.
  `atomic_write` (temp file → `sync_all` → rename) means a write can never
  leave a half-finished image; `backup_file` keeps generational copies under
  `.art-backup/` (3 for disk images, 5 for config files, off for large HDFs).
- `OverwritePolicy` for LHA extraction (`Skip` by default, plus `Overwrite`
  and `Rename`), so extracting over a folder no longer destroys existing files.
- `MutationOutcome` / `GotekSaveOutcome` / `PistormSaveOutcome` carry the backup
  path back to the UI, so the user is always told where the previous version went.
- Read-only mounting option for HDFs passed to WinUAE.

#### Fixed
- **ADF hash function was AmigaDOS-incompatible.** `name_hash` omitted the
  `& 0x7ff` mask applied after every character, so entries were written to the
  wrong hash buckets. Images ART produced were readable by ART but not by
  AmigaDOS, WinUAE or any other Amiga tool. Now matches `adfGetHashValue`, with
  reference values pinned by tests, and honours the volume's international flag.
- **ADF edits overwrote the original in place with no backup.** The mutation
  path is now `read → mutate → validate → backup → atomic commit`; a failed or
  corrupting mutation leaves the on-disk image untouched.
- **Invalid block numbers from the UI crashed the whole application.** Bare
  indexing in `mutate.rs` panicked, and the release profile aborts on panic.
  All block access is now bounds-checked through `blocks::block_slice*`.
- `rename_entry` silently continued when an entry was missing from its parent's
  hash chain, linking it in twice and corrupting the directory.
- Files and directories could be created with names already in use.
- A file header could be passed where a directory was expected, corrupting it.
- Hash and file-extension chains could loop forever on a malformed image.
- **FlashFloppy `FF.CFG` was regenerated from scratch**, discarding every
  hand-tuned setting ART does not manage (spec §39). It is now edited in place.
- **PiStorm `cmdline.txt` was regenerated from scratch**, dropping `root=` and
  leaving the SD card unbootable. `config.txt` and `cmdline.txt` are now merged.
- HD floppy geometry never reported: the two size checks in `analysis.rs` were
  the same number (`901_120` and `880 * 1024`). Caught by clippy, hidden by CI.
- Multiple HDFs generated bare `hardfile=` lines with no device names, making
  every drive after the first unreachable. Now emits `hardfile2=` per device.
- WinUAE detection used hard-coded `D:\WinUAE` paths; it now reads the real
  Program Files locations from the environment.
- Media paths containing line breaks could inject arbitrary `.uae` directives.
- Zip-bomb guard could be bypassed by an archive declaring a size that overflowed
  the running total (`checked_add`), and aborted extractions left truncated files.
- A fixed `art_launch.uae` temp name made concurrent launches clobber each other.

#### Changed
- CI no longer runs clippy with `continue-on-error` — it hid the geometry bug.
- `lib.rs` narrowed its blanket `allow` from four lints to `dead_code` only.
- Tests: 90 → 133, all passing. Clippy: clean at `-D warnings`.

### Phase 0 — Foundation (2026-08-08)

#### Added
- Tauri 2 + React 19 + TypeScript + Vite application shell.
- Platform-independent Rust core engine with module skeletons:
  `adf`, `hdf`, `lha`, `rdb`, `rom`, `binary`, `analysis`, `recovery`,
  `compatibility`, `hashing`, `conversion`, `validation`.
- Format detection (`core/detect`) — extension + size + signature based,
  with confidence levels. Supports ADF, ADZ, DMS, HDF, HDZ, LHA, ROM,
  directories.
- SHA256 hashing (`core/hashing`) — streaming, memory-safe for large images.
- Workflow Engine (`core/workflow`) — trait-based `Workflow` + registry +
  engine that turns a detected object into a ranked `Plan` of candidate
  workflows. Ships two built-in workflows: Inspect, Compute SHA256.
- Universal Drag & Drop Manager — single global webview listener; dropped
  paths are forwarded to the Workflow Engine for analysis.
- SQLite database (`tauri-plugin-sql`) with initial migration
  (`settings`, `recent_files`, `jobs` tables).
- Structured logging (`tauri-plugin-log`) — stdout (dev) + log dir (release).
- JSON key/value settings (`tauri-plugin-store`) — theme, UX mode, language,
  paths.
- i18n architecture (`react-i18next`) — English locale, ready for more.
- Dark/light theme with subtle Amiga-inspired accent.
- Dashboard with drop target, recent files, quick actions.
- Settings page (appearance, general, paths).
- "Coming Later" placeholders for all not-yet-implemented modules.
- Windows CI pipeline (GitHub Actions): type-check, fmt, clippy, test, build.
- `cargo-deny` license/advisory policy (`deny.toml`).
- Documentation: architecture, product-vision, roadmap, format-support-matrix,
  security-model, drag-drop-workflows, testing, licenses.

#### Build
- `pnpm tauri build` produces MSI + NSIS installers for Windows x64.

#### Tests
- 14 Rust unit tests covering detection, hashing, and the workflow registry.
