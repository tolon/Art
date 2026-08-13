#!/usr/bin/env python3
"""Measure ART's layout at several Application Sizes, in a real browser.

ART-099 is the reason this exists. Two fixes for "Application Size cuts the
right-hand edge off" were written from screenshots, both were wrong, and the
second shipped for an hour. What was missing was not care — it was a number.
Editing one line and taking another picture is not a reproduction.

So this drives the **running application** in headless Chrome and prints what
the shell and the content area actually measure at each size. What it reports:

    client   how much room the content column has, in CSS pixels
    scroll   how much room the content wants
    over     scroll - client. Anything above zero is content the user cannot
             see, and before ART-099 it was content they could not reach either
    shell    the shell's rendered width. It must equal the window at every
             size: `width: auto` resolves in the parent's coordinate space and
             the zoom applies to both alike, which is why every attempt to
             "correct" the width made things worse

Usage:

    pnpm dev                     # in another terminal
    python scripts/zoom-check.py

Requires Chrome (or Edge) and nothing else — no Playwright, no extra
dependency in package.json. The page measures itself and Chrome's `--dump-dom`
prints the result, which is enough and installs nothing.

Not in CI: it needs a browser and a dev server, and it answers a question about
layout rather than about correctness. Run it when touching the shell.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

DEV_URL = "http://127.0.0.1:1420"
ROOT = Path(__file__).resolve().parent.parent

# Every screen that is a plain page. The Files screen opts out of the centred
# column (`.app-content-wide`) and is measured by its own pane rules, so it is
# not part of this question.
ROUTES = ["#/", "#/settings", "#/hard-disk", "#/pistorm", "#/aminet", "#/rom", "#/collection"]
ZOOMS = [1, 1.3, 2]

# The user's own window, maximised, from ART-099. Measuring at a small default
# would miss what a real screen does.
WINDOW = (2575, 1407)

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
]

MEASURE_JS = """
(async () => {
  const out = [];
  const n = (x) => Math.round(x * 100) / 100;
  for (const route of __ROUTES__) {
    location.hash = route;
    await new Promise((r) => setTimeout(r, 400));
    for (const z of __ZOOMS__) {
      document.documentElement.style.setProperty("--app-zoom", String(z));
      document.body.offsetHeight;
      await new Promise((r) => setTimeout(r, 60));

      const shell = document.querySelector(".app-shell");
      const content = document.querySelector(".app-content");
      if (!shell || !content) { out.push(route + " z=" + z + " NO-SHELL"); continue; }

      const win = document.documentElement.clientWidth;
      let worst = null;
      for (const el of content.querySelectorAll("*")) {
        const r = el.getBoundingClientRect();
        if (r.width > 0 && r.right > win + 1 && (!worst || r.right > worst.right)) {
          worst = { right: r.right, tag: el.tagName, cls: String(el.className).slice(0, 30) };
        }
      }
      out.push([
        route, "z=" + z, "window=" + win,
        "shell=" + n(shell.getBoundingClientRect().width),
        "client=" + content.clientWidth,
        "scroll=" + content.scrollWidth,
        "over=" + (content.scrollWidth - content.clientWidth),
        worst ? "offscreen=" + worst.tag + "." + worst.cls + "@" + n(worst.right) : "offscreen=none",
      ].join(" "));
    }
  }
  document.documentElement.style.removeProperty("--app-zoom");
  return out.join("\\n");
})()
"""

PROBE_HTML = """<!doctype html>
<html lang="en"><head><meta charset="UTF-8" /><title>zoom-check</title></head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
    <pre id="zoom-check">not run</pre>
    <script type="module">
      const measure = await fetch("/__zoom-check.js").then((r) => r.text());
      // The application has to mount before there is anything to measure.
      await new Promise((done) => setTimeout(done, 2500));
      document.getElementById("zoom-check").textContent = String(await eval(measure));
    </script>
  </body>
</html>
"""


def find_browser() -> str:
    for candidate in CHROME_CANDIDATES:
        if Path(candidate).exists():
            return candidate
    print("No Chrome or Edge found. Looked in:")
    for candidate in CHROME_CANDIDATES:
        print(f"  {candidate}")
    sys.exit(2)


def dev_server_is_up() -> bool:
    try:
        urllib.request.urlopen(DEV_URL, timeout=2).read(1)
        return True
    except (urllib.error.URLError, OSError):
        return False


def main() -> int:
    if not dev_server_is_up():
        print(f"No dev server at {DEV_URL}. Start one with `pnpm dev` and run this again.")
        return 2

    browser = find_browser()

    # Served by Vite from the project root, and removed afterwards whatever
    # happens: a probe page left behind would be the next person's confusion.
    html = ROOT / "__zoom-check.html"
    js = ROOT / "__zoom-check.js"
    html.write_text(PROBE_HTML, encoding="utf-8")
    js.write_text(
        MEASURE_JS.replace("__ROUTES__", repr(ROUTES)).replace("__ZOOMS__", repr(ZOOMS)),
        encoding="utf-8",
    )

    try:
        result = subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                f"--window-size={WINDOW[0]},{WINDOW[1]}",
                "--virtual-time-budget=30000",
                "--dump-dom",
                f"{DEV_URL}/__zoom-check.html",
            ],
            capture_output=True,
            text=True,
            # The DOM is UTF-8; Python would otherwise decode it in the
            # console's code page and fall over on the first Turkish string.
            encoding="utf-8",
            errors="replace",
            timeout=180,
        )
    finally:
        html.unlink(missing_ok=True)
        js.unlink(missing_ok=True)

    match = re.search(r'<pre id="zoom-check">(.*?)</pre>', result.stdout, re.S)
    if not match or match.group(1).strip() == "not run":
        print("The probe did not report. The application may have failed to mount.")
        print(result.stderr[-2000:])
        return 1

    body = match.group(1).strip()
    print(f"Window {WINDOW[0]}x{WINDOW[1]}\n")
    print(body)

    # `over` above zero is the whole point: content the user cannot see. Since
    # ART-099 it is reachable — `.app-content` scrolls sideways — but a screen
    # that needs a sideways scrollbar to be read is still a screen to fix.
    overflowing = [line for line in body.splitlines() if re.search(r"over=[1-9]", line)]
    if overflowing:
        print(f"\n{len(overflowing)} measurement(s) overflow the content column:")
        for line in overflowing:
            print(f"  {line}")
        return 1

    print("\nNothing overflows its column at any size.")
    return 0


if __name__ == "__main__":
    os.chdir(ROOT)
    sys.exit(main())
