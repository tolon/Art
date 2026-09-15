#!/usr/bin/env python3
"""Measure which visible Turkish text clips or overflows, in a real browser,
against the same screen in English at the same window width.

ART-062 is the reason this exists: several Turkish strings are substantially
longer than their English originals and sit in tight controls, and nobody had
opened a running instance and looked. Editing `tr.json` and re-reading the
JSON is not the same claim as reading a screen — this drives the **running
application** in headless Chrome, the way `scripts/zoom-check.py` (ART-099)
already does, and reports what actually clips.

One Chrome process per (route, width) rather than one process looping every
route: `src/main.tsx` wraps the whole app in one global `ErrorBoundary`
(`src/components/ErrorBoundary.tsx`), and its `hasError` state never resets
on a hash change — one route crashing during mount would poison every route
measured afterwards in the same page load. Width needs its own process
regardless (`--window-size` is a Chrome launch flag); language does not —
`i18next.changeLanguage` re-renders the already-mounted tree in place, so
Turkish and the English control are measured in the same process/mount for a
comparable pair.

How Turkish is forced: `src/App.tsx` reads `useSettingsStore().settings.language`
and calls `changeLanguage()` (`src/i18n/index.ts`) once `loadSettings()`
resolves. Off Tauri, `src/lib/settings.ts::getSettings()` catches the failed
`invoke` and returns `DEFAULT_SETTINGS` (`language: "en"`) — there is no
localStorage key and no query parameter that picks the language, so a bare
headless page always starts in English. This probe dynamically imports the
same module the app itself imports (`import("/src/i18n/index.ts")`, exactly
what the `@/i18n` alias resolves to per `vite.config.ts`, so the same
i18next singleton) and calls its exported `changeLanguage("tr")` directly,
before navigating to the route.

Detection rule: every element with a *direct* (non-descendant) non-blank
text node, visible (`display != none`, non-zero rect), is a hit if either:
(a) `scrollWidth > clientWidth + 1` **and** the element's own computed style
is `overflow`/`overflow-x: hidden|clip` or `text-overflow: ellipsis` —
genuinely clipped text; or (b) its right edge passes the viewport, or passes
the right edge of its nearest `button`, `[role=tab]`, `[class*=badge]`,
`[class*=chip]`, `[class*=tab]`, `.tc-fkey` or `a` ancestor. A hit is
reported as **Turkish-only** when the same path+kind does not also appear in
the English control run of the same (route, width) — a pre-existing layout
issue would hit in both languages and is not this task's question.

Usage:

    pnpm dev                        # in another terminal
    python scripts/tr-overflow-check.py

Requires Chrome (or Edge) and nothing else — no Playwright, no extra
dependency in package.json, same as `zoom-check.py`. A throwaway probe page
(`__tr-overflow-check.html` / `.js`) is written into the repo root, because
Vite serves the project root and a file outside it is not servable by the
dev server, and removed again in a `finally` whatever happens — no tracked
file is touched and nothing is left behind.

Not in CI: it needs a browser and a dev server, and it answers a question
about the Turkish catalogue's layout rather than about correctness. What it
cannot see: any screen or control that only renders with real backend data
(a loaded disk image, a scanned USB drive, a running job) — those need a
live Tauri session and are outside what a plain browser can drive.
"""

from __future__ import annotations

import html
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

DEV_URL = "http://127.0.0.1:1420"
ROOT = Path(__file__).resolve().parent.parent

# Every top-level route in `src/App.tsx`'s router. `os-builder` has its own
# sub-routes (`osBuilderRoutes()`); only the parent path is hit here, the
# same scope decision `zoom-check.py` already makes for it.
ROUTES = [
    "/",
    "/settings",
    "/disk-tools",
    "/archive-tools",
    "/winuae",
    "/rom",
    "/hard-disk",
    "/gotek",
    "/pistorm",
    "/os-builder",
    "/layout",
    "/tools",
    "/collection",
    "/aminet",
    "/files",
    "/whdload",
]

# 1280x800 and 1024x768 are ordinary desktop sizes; 960 is the shell's own
# floor (`src-tauri/tauri.conf.json`'s `"minWidth": 960`) — the narrowest
# width ART itself claims to support.
WIDTHS = [(1280, 800), (1024, 768), (960, 768)]

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
]

MEASURE_JS = r"""
(async () => {
  const route = __ROUTE__;
  const out = { route, results: {} };
  try {
    const i18nMod = await import('/src/i18n/index.ts');

    function isVisible(el) {
      const cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') return false;
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    }

    function shortPath(el) {
      const parts = [];
      let cur = el;
      for (let i = 0; i < 4 && cur && cur !== document.body; i++) {
        let s = cur.tagName.toLowerCase();
        if (cur.id) s += '#' + cur.id;
        const cls = (cur.className && typeof cur.className === 'string')
          ? cur.className.trim().split(/\s+/).filter(Boolean).slice(0, 2).join('.') : '';
        if (cls) s += '.' + cls;
        parts.unshift(s);
        cur = cur.parentElement;
      }
      return parts.join(' > ');
    }

    // Every element with a *direct* (non-descendant) non-blank text node, so
    // a card is not double-counted through every ancestor that also
    // "contains" its label's text.
    function measure() {
      const hits = [];
      const win = window.innerWidth;
      const all = document.body.querySelectorAll('*');
      for (const el of all) {
        let hasDirectText = false;
        for (const n of el.childNodes) {
          if (n.nodeType === 3 && n.textContent.trim().length > 0) { hasDirectText = true; break; }
        }
        if (!hasDirectText) continue;
        if (!isVisible(el)) continue;
        const text = el.textContent.trim().slice(0, 140);
        if (!text) continue;

        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        const ellipsis = cs.textOverflow === 'ellipsis';
        const hiddenX = cs.overflowX === 'hidden' || cs.overflowX === 'clip' ||
                        cs.overflow === 'hidden' || cs.overflow === 'clip';
        // (a) clipped: scrollWidth > clientWidth while overflow is
        // hidden/clip or text-overflow is ellipsis.
        const clippedSelf = el.scrollWidth > el.clientWidth + 1 && (ellipsis || hiddenX);
        // (b) the text box extends past the viewport...
        const overflowsViewport = r.right > win + 1;
        // ...or past its own button/badge/tab.
        let overflowsControl = false, controlOverPx = 0, controlSel = '';
        const ctrl = el.closest('button, [role="tab"], [class*="badge"], [class*="chip"], [class*="tab"], .tc-fkey, a');
        if (ctrl && ctrl !== el) {
          const cr = ctrl.getBoundingClientRect();
          if (r.right > cr.right + 1) {
            overflowsControl = true; controlOverPx = r.right - cr.right; controlSel = ctrl.tagName.toLowerCase();
          }
        }

        if (clippedSelf || overflowsViewport || overflowsControl) {
          let overPx = 0; const kinds = [];
          if (clippedSelf) { overPx = Math.max(overPx, el.scrollWidth - el.clientWidth); kinds.push('clip'); }
          if (overflowsViewport) { overPx = Math.max(overPx, r.right - win); kinds.push('viewport'); }
          if (overflowsControl) { overPx = Math.max(overPx, controlOverPx); kinds.push('control:' + controlSel); }
          hits.push({ path: shortPath(el), text, overPx: Math.round(overPx * 100) / 100, kind: kinds.join('+') });
        }
      }
      return hits;
    }

    async function run(lang) {
      await i18nMod.changeLanguage(lang);
      document.body.offsetHeight;
      await new Promise((r) => setTimeout(r, 500));
      const content = document.querySelector('.app-content');
      const bodyText = document.body.innerText || '';
      const crashed = bodyText.indexOf('ART failed to render') !== -1 ||
                      bodyText.indexOf('ART aray') !== -1; // "ART arayüzü oluşturulamadı"
      return {
        textLen: content ? content.innerText.trim().length : 0,
        crashed,
        hits: measure(),
      };
    }

    // Start in Turkish (the whole point of this probe), navigate, measure;
    // then switch the same mounted tree to English as the control.
    await i18nMod.changeLanguage('tr');
    location.hash = '#' + route;
    await new Promise((r) => setTimeout(r, 1500));
    out.results['tr'] = await run('tr');
    out.results['en'] = await run('en');
  } catch (e) {
    out.error = String(e && e.stack || e);
  }
  return JSON.stringify(out);
})()
"""

PROBE_HTML = """<!doctype html>
<html lang="en"><head><meta charset="UTF-8" /><title>tr-overflow-check</title></head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
    <pre id="overflow-check">not run</pre>
    <script type="module">
      const measure = await fetch("/__tr-overflow-check.js").then((r) => r.text());
      await new Promise((done) => setTimeout(done, 2000));
      document.getElementById("overflow-check").textContent = String(await eval(measure));
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


def run_one(browser: str, route: str, width: int, height: int) -> dict:
    # Served by Vite from the project root, and removed afterwards whatever
    # happens: a probe page left behind would be the next person's confusion.
    html_path = ROOT / "__tr-overflow-check.html"
    js_path = ROOT / "__tr-overflow-check.js"
    html_path.write_text(PROBE_HTML, encoding="utf-8")
    js_path.write_text(MEASURE_JS.replace("__ROUTE__", json.dumps(route)), encoding="utf-8")
    try:
        result = subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                f"--window-size={width},{height}",
                "--virtual-time-budget=9000",
                "--dump-dom",
                f"{DEV_URL}/__tr-overflow-check.html",
            ],
            capture_output=True,
            text=True,
            # The DOM is UTF-8; Python would otherwise decode it in the
            # console's code page and fall over on the first Turkish string.
            encoding="utf-8",
            errors="replace",
            timeout=60,
        )
    finally:
        html_path.unlink(missing_ok=True)
        js_path.unlink(missing_ok=True)

    match = re.search(r'<pre id="overflow-check">(.*?)</pre>', result.stdout, re.S)
    if not match or match.group(1).strip() == "not run":
        return {"route": route, "width": width, "error": "no-report", "stderr": result.stderr[-1500:]}
    body = html.unescape(match.group(1).strip())
    try:
        data = json.loads(body)
    except json.JSONDecodeError as e:
        return {"route": route, "width": width, "error": f"bad-json: {e}", "raw": body[:1500]}
    data["width"] = width
    return data


def main() -> int:
    if not dev_server_is_up():
        print(f"No dev server at {DEV_URL}. Start one with `pnpm dev` and run this again.")
        return 2
    browser = find_browser()

    all_results = []
    total = len(ROUTES) * len(WIDTHS)
    n = 0
    for (w, h) in WIDTHS:
        for route in ROUTES:
            n += 1
            print(f"[{n}/{total}] {route} @ {w}x{h}", file=sys.stderr)
            all_results.append(run_one(browser, route, w, h))

    errors = [r for r in all_results if "error" in r]
    for r in errors:
        print(f"ERROR {r['route']} @ {r['width']}: {r['error']}")

    crashed = []
    turkish_only = []
    both_languages = []
    for r in all_results:
        if "error" in r:
            continue
        for lang in ("tr", "en"):
            if r["results"][lang]["crashed"]:
                crashed.append((r["route"], r["width"], lang))

        tr_hits = r["results"]["tr"]["hits"]
        en_hits = r["results"]["en"]["hits"]
        en_keys = {(h["path"], h["kind"]) for h in en_hits}
        for h in tr_hits:
            entry = (r["route"], r["width"], h["path"], h["text"], h["kind"], h["overPx"])
            if (h["path"], h["kind"]) in en_keys:
                both_languages.append(entry)
            else:
                turkish_only.append(entry)

    print(f"\n{len(all_results)} (route x width) run(s), {len(errors)} error(s), {len(crashed)} crash(es).")

    if crashed:
        print(f"\n{len(crashed)} crash(es) — the ErrorBoundary fallback was seen:")
        for route, width, lang in crashed:
            print(f"  {route} @ {width} [{lang}]")

    if both_languages:
        print(f"\n{len(both_languages)} both-languages hit(s) (pre-existing layout, not this task's question):")
        for route, width, path, text, kind, over_px in both_languages:
            print(f"  {route} @ {width} [{kind}] {over_px}px  {path}  {text!r}")

    if turkish_only:
        print(f"\n{len(turkish_only)} Turkish-only hit(s):")
        for route, width, path, text, kind, over_px in turkish_only:
            print(f"  {route} @ {width} [{kind}] {over_px}px  {path}  {text!r}")
        return 1

    if errors or crashed:
        return 1

    print("\nTurkish-only list is empty at every route and width measured.")
    return 0


if __name__ == "__main__":
    os.chdir(ROOT)
    sys.exit(main())
