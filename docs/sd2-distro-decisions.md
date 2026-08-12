# SD-2 distro decisions

Answers to two of the open questions in `ART-research-distro-profiles.md` §8,
each closed with evidence rather than parked. The other four stay parked
deliberately — see the bottom of this page.

Researched 2026-08-13/14.

---

## §8.3 — P96: the free Aminet Picasso96 is enough

**Decided: route (b).** ART can build against the **free Picasso96 from
Aminet**. iComp's P96 is an optional upgrade the user supplies; ART neither
needs nor bundles it.

The evidence is the Emu68 project's own RTG tutorial, which names Aminet as a
source in as many words:

> You can find older version on Aminet, but if you prefer a more recent one
> with several bug fixes and improvemends, go to the iComp website.
>
> — <https://michalsc.github.io/Emu68/tutorials/P96_Setup.html>

So the commercial P96 is *newer*, not *required*. That settles the question the
research left open, and it matters: a baseline that needed a paid component
would not be a baseline anybody could reproduce.

### What ART has to do differently for the Aminet version

The PiStorm project's version of the same tutorial adds a step that applies
**only** to the free one:

> As of version 1.2.1 of Videocore, an additional step is required if you are
> using the Aminet version of P96. You also need to add the `VC4_LEGACY_ID`
> tooltype.
>
> — <https://pistorm.github.io/tutorials/p96setup/>

Added to the monitor icon's tooltypes, as its own line, and **the order of the
lines matters** — the tutorial says so explicitly.

Three more things the same pages pin down, all of which the OS Builder will
have to get right when it places files rather than describing them:

- The P96 installer must be run with **at least one graphics card selected**,
  or it never creates the file in `DEVS:Monitors` that everything else depends
  on. Any card will do; the real driver replaces it afterwards.
- The card file goes into `LIBS:Picasso96/` — `copy RAM:videocore.card
  LIBS:Picasso96/`. Older material calls it `emu68-vc4.card`; newer Emu68-tools
  releases call it `videocore.card`. **Check the name in the release actually
  being installed**, exactly as the kernel archive turned out to need
  ([ART-091](ISSUES.md#open)).
- `emu68-vc4.card` does not reconfigure the HDMI port; the VPU does that once,
  at power-on. A machine started with nothing plugged into HDMI has no RTG that
  session. That is a first-boot note for the user, not something ART can fix.

**Consequence for ART Baseline:** the RTG stack is reproducible from free
components, and the Aminet fetch is something ART's §41.5 engine already does.
The `VC4_LEGACY_ID` tooltype is a step ART must perform when it installs the
Aminet version — and it is version-dependent (Videocore 1.2.1 onward), so it
belongs in the recipe with its condition attached, not as an unconditional
line.

---

## §8.4 — HstWB package format, and the half of it ART can use

**The format**, read off `henrikstengaard/classicwb-lite-package` (MIT):

```
package/
  hstwb-package.json     the manifest
  Install                an AmigaDOS Installer script (~26 KB)
  classicwb_lite_v28.zip the content
  README, README.info
  Patches/, Temp/
```

The manifest is small and entirely declarative:

```json
{
  "contentIds": ["classicwb", "amigaos"],
  "name": "ClassicWB LITE",
  "version": "28.2.1",
  "priority": 1,
  "assigns": [{ "name": "SYSTEMDIR" }],
  "amigaOsVersions": ["3.1", "3.1.4", "3.2"]
}
```

- `contentIds` — what the package contributes, so an installer can tell a
  Workbench layer from OS files.
- `priority` — the order packages are applied in.
- `assigns` — the named assigns the install expects to exist.
- `amigaOsVersions` — which bases it supports. Note ClassicWB LITE lists
  `3.1`, `3.1.4` and `3.2` and **not** `3.9`; the OS39 flavour is a separate
  package.

### The finding that shapes SD-2b

**`Install` is an Amiga-side script.** It is written in the AmigaDOS Installer
language and runs on the Amiga — ART cannot execute it on Windows. So "ART
consumes HstWB packages" splits cleanly in two, and only one half is available
without an Amiga in the loop:

| | Usable from ART on Windows |
|---|---|
| `hstwb-package.json` — name, version, priority, assigns, supported OS versions | **yes**, today |
| The content archive | **yes** — a zip ART's archive reader already opens |
| `Install` — the layout logic | **no**, not without running it on an Amiga |

Two honest routes, and the choice belongs to SD-2b's design rather than to this
page:

1. **Run the script where it runs** — under WinUAE, which the research already
   wants as the build oracle (§1.1.5). ART prepares a volume, boots it, lets
   the real Installer do the work. Slow, and it needs a licensed ROM, but it is
   the packages' own mechanism and cannot drift from it.
2. **Read the manifest, place the content, write the layout ourselves.** Fast
   and offline, but ART would be reimplementing each package's install logic —
   and a package that changes its script would silently stop matching.

Route 1 is the one that stays true as the catalogue changes. Route 2 is
tempting and is how a tool ends up quietly diverging from the thing it claims
compatibility with. **Recorded, not decided** — but the finding that makes it a
real question is that `Install` is 26 KB of Amiga script, and no amount of
manifest reading substitutes for it.

Licence: the packages are MIT and hold only redistributable content; the user
supplies the OS. That is the model ART Baseline already uses, which is why the
interop is worth having.

---

## §8.2 — CaffeineOS: what the 9318 preview folder yields, and what it does not

The user pointed at a second Drive folder,
`drive.google.com/drive/folders/1VIK8ZyRmd04HMy5OArwUQRAfZvOYZnNe`, titled
**`9318_Preview`**, alongside a Google Doc, *CaffeineOS_Storm ReadMe + Log*.

**It does not unblock the adapter.** Drive serves the folder's *listing* but
refuses the files, and the Doc's body needs a signed-in session — the export
endpoint redirects to a host that answers 400 without one. So §8.2 stands
exactly as the research left it: the distribution has to be downloaded by hand
and inspected with ART's own tools before an adapter is written.

### What the listing does establish (fetched, not inferred)

| Item | |
|---|---|
| `Caffeinerom.txt` | 7 KB — the custom ROM, described |
| `Dopus_CaffeineEdition/` | its own DOpus build |
| `WiFiPi.device-1.4.readme` | the Emu68 WiFi driver |
| `wifipi.mni-1.2.readme`, `genet.mni-1.3.readme` | `.mni` modules |
| `ChangeWPA-2.4.readme` | a WPA credentials tool |
| `sanautil-0.40`, `netio-1.33r4`, `netsummary-1.0` | SANA-II utilities |

Two things follow that are worth carrying into the design.

**The version line runs past 9317.** The research recorded 9281→9317; a 9318
preview exists. A profile that pinned a version would already be stale, which
is why the registry pins none — it describes the distribution, not a release.

**Where WiFi credentials live is stack-dependent, and ART must not assume.**
For the Roadshow-based path — AmiKit's, and Emu68's own — the SSID and password
are in `DEVS:NetInterfaces/WiFiPi`, edited as a text file
(<https://www.amikit.amiga.sk/wifipi>). That is the location the research names
in §3.3, and it is correct *for that stack*.

But CaffeineOS's folder lists `.mni` modules and a `ChangeWPA` tool, and the
research independently records it as shipping **Miami DX**. Miami keeps its own
configuration and does not read `DEVS:NetInterfaces`. **Inference, flagged as
such** — the readmes are behind the same Drive wall — but the shape of the
evidence is clear enough to act on defensively: G14 must ask *which TCP/IP
stack* a built volume runs before it writes a credential anywhere, and a
`DEVS:NetInterfaces/WiFiPi` written onto a Miami system is a file nothing
reads.

### What is still needed, and from whom

The adapter needs a real card to read, not a folder of readmes. Concretely:
the distribution image itself, mounted and inspected with ART's own tools —
partition map, FAT32 contents, `cmdline.txt` and `config.txt` as shipped, the
Kickstart's name on the card, and what differs between the per-board variants.
That is the adaptation checklist, and every line of it has to be read rather
than guessed.

---

## Still parked, deliberately

- **§8.1 OnyxSoft** — the site answered 503 during both research passes.
- **§8.2 CaffeineOS layout** — still blocked; see the section above for what the
  9318 preview folder did and did not yield. **This is what blocks SD-2a**, and
  it is a deliberate block: guessing at a card layout is how a tool comes to
  write something that quietly does not boot.
- **§8.5 PiMiga's PiStorm lineage** — revisited when its adapter is scheduled.
- **§8.6 AmigaOS "3.3"** — watch what it turns out to be; add to ROM Manager's
  per-OS table when it is real rather than named.
