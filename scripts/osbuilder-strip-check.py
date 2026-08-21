#!/usr/bin/env python3
"""Measure the OS Builder's progress strip, in a real browser, in both languages.

**Why this exists.** Wave 1 of the flow round turned the OS Builder's one
scrolling column into steps with a strip of links above them. Every test that
covers it is jsdom, and **jsdom does no layout at all** — it cannot say how wide
the strip renders, whether it wraps, or whether the Turkish labels (which run
measurably longer than the English) still fit. That is exactly the gap this
project's own record says the expensive defects live in.

It follows `zoom-check.py` in shape, and for the same reason: a screenshot and
an opinion are not a reproduction. This prints numbers.

What it reports, per language and per build kind:

    steps     how many links the strip offers — must match `stepsFor(kind)`
    labels    the rendered text of each, so a raw `osBuilder.step.*` key or an
              unrendered `{{…}}` is visible rather than inferred
    strip     the strip's own client/scroll width. `over` above zero is a strip
              wider than its box
    body      the document's scrollWidth against the window. Anything above
              zero is horizontal scroll on the page itself, which no screen
              here may have
    tallest   the strip's rendered height, which is how wrapping shows up: one
              row of buttons or two

Usage:

    pnpm dev                              # in another terminal
    python scripts/osbuilder-strip-check.py
"""

from __future__ import annotations

import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEV_URL = "http://127.0.0.1:1420"

# The size the owner actually runs the application at is unknown, so this uses
# the same window `zoom-check.py` does — one number, comparable between runs.
WINDOW = (1280, 900)

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
]

MEASURE_JS = r"""
(async () => {
  const out = [];
  const n = (x) => Math.round(x * 100) / 100;
  const settle = (ms) => new Promise((r) => setTimeout(r, ms));

  // The same module instance the application uses — Vite serves it, so this
  // is the real catalogue and the real switch, not a stand-in.
  //
  // **In the running application the language is changed from Settings.** That
  // path cannot be driven here: `settingsStore.update()` awaits `saveSettings`,
  // which is a Tauri command with no IPC bridge in a plain browser, so the
  // `changeLanguage` call after it is never reached. This calls the function
  // the store itself would have called — the same one, one layer in.
  const i18n = await import("/src/i18n/index.ts");
  const t = (k) => i18n.default.t(k);

  const kindRows = () =>
    [...document.querySelectorAll("section.card div[style*='cursor: pointer']")];

  /** Choose a build kind the way a user does: by clicking its row on step 1. */
  async function chooseKind(titleKey) {
    location.hash = "#/os-builder/hedef";
    await settle(700);
    const want = t(titleKey);
    const row = kindRows().find((r) => {
      const strong = r.querySelector("strong");
      return strong && strong.textContent.trim() === want;
    });
    if (!row) {
      out.push("NO-ROW for " + titleKey + " (" + want + ")");
      return false;
    }
    row.click();
    await settle(700);
    return true;
  }

  async function measure(tag) {
    await settle(500);
    // NOT `document.querySelector("nav")` — the shell's sidebar is a `nav`
    // too and comes first, so that measured the sidebar and reported a
    // comfortable zero. The strip is the one holding step links; the
    // sidebar's own OS Builder link is `#/os-builder` with no trailing
    // segment, so it cannot match.
    const strip = [...document.querySelectorAll("nav")].find((el) =>
      el.querySelector('a[href*="/os-builder/"]')
    );
    if (!strip) {
      out.push(tag + " NO-STRIP route=" + location.hash);
      return;
    }
    const links = [...strip.querySelectorAll("a")];
    const labels = links.map((a) => a.textContent.trim()).join(" | ");
    const raw = /osBuilder\.step\./.test(labels) || /\{\{/.test(labels);
    const doc = document.documentElement;
    out.push(
      [
        tag,
        "steps=" + links.length,
        "strip_client=" + strip.clientWidth,
        "strip_scroll=" + strip.scrollWidth,
        "over=" + (strip.scrollWidth - strip.clientWidth),
        "strip_height=" + n(strip.getBoundingClientRect().height),
        "body_over=" + (doc.scrollWidth - doc.clientWidth),
        "raw_or_unrendered=" + raw,
        "route=" + location.hash,
        "labels=[" + labels + "]",
      ].join(" ")
    );
  }

  for (const lng of ["en", "tr"]) {
    await i18n.changeLanguage(lng);
    await settle(500);

    location.hash = "#/os-builder/hedef";
    await settle(700);
    out.push(lng + " picker_rows=" + kindRows().length);

    // The default kind, wherever it is.
    await measure(lng + " kind=default");

    // Then each kind, chosen by clicking its own row, measured on the step
    // that kind actually lands on.
    if (await chooseKind("osBuilder.what.bootCard")) {
      await measure(lng + " kind=boot-card");
    }
    if (await chooseKind("osBuilder.what.prepareVolumes")) {
      await measure(lng + " kind=prepare-volumes");
    }
    // Install is the widest: four steps.
    if (await chooseKind("osBuilder.what.install")) {
      await measure(lng + " kind=install    ");
      location.hash = "#/os-builder/paketler";
      await measure(lng + " kind=install@paketler");

      // **Application Size.** It exists because most of the people using this
      // are over fifty, and CLAUDE.md's rule is that a new screen inherits it
      // from the shell rather than fighting it. The widest strip (four steps)
      // in the longer language is the case that would break first, so it is
      // the one measured at every size.
      for (const z of [1, 1.3, 2]) {
        document.documentElement.style.setProperty("--app-zoom", String(z));
        document.body.offsetHeight;
        await measure(lng + " kind=install z=" + z);
      }
      document.documentElement.style.removeProperty("--app-zoom");
    }
  }

  await i18n.changeLanguage("en");
  return out.join("\n");
})()
"""

PROBE_HTML = """<!doctype html>
<html lang="en"><head><meta charset="UTF-8" /><title>strip-check</title></head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
    <pre id="strip-check">not run</pre>
    <script type="module">
      const measure = await fetch("/__strip-check.js").then((r) => r.text());
      await new Promise((done) => setTimeout(done, 2500));
      document.getElementById("strip-check").textContent = String(await eval(measure));
    </script>
  </body>
</html>
"""


def find_browser() -> str:
    for candidate in CHROME_CANDIDATES:
        if Path(candidate).exists():
            return candidate
    print("No Chrome or Edge found.")
    sys.exit(2)


def dev_server_is_up() -> bool:
    try:
        urllib.request.urlopen(DEV_URL, timeout=2).read(1)
        return True
    except (urllib.error.URLError, OSError):
        return False


def main() -> int:
    if not dev_server_is_up():
        print(f"No dev server at {DEV_URL}. Start one with `pnpm dev`.")
        return 2

    browser = find_browser()
    html = ROOT / "__strip-check.html"
    js = ROOT / "__strip-check.js"
    html.write_text(PROBE_HTML, encoding="utf-8")
    js.write_text(MEASURE_JS, encoding="utf-8")

    try:
        result = subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                f"--window-size={WINDOW[0]},{WINDOW[1]}",
                "--virtual-time-budget=40000",
                "--dump-dom",
                f"{DEV_URL}/__strip-check.html",
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=180,
        )
    finally:
        html.unlink(missing_ok=True)
        js.unlink(missing_ok=True)

    match = re.search(r'<pre id="strip-check">(.*?)</pre>', result.stdout, re.S)
    if not match or match.group(1).strip() == "not run":
        print("The probe did not report. The application may have failed to mount.")
        print(result.stderr[-3000:])
        return 1

    print(f"Window {WINDOW[0]}x{WINDOW[1]}\n")
    print(match.group(1).strip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
