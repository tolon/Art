// @vitest-environment jsdom
//
// ART-085, proved the way it failed: by leaving the screen.
//
// The studios call Tauri on mount and pull in most of `lib/*`, so — following
// `FileManagerFilter.test.tsx` — this is a harness wired the same way they are
// rather than a render of the real pages: the real `useOpenObject` hook, and
// the same "load what router state names, else what is already open, and treat
// a null parse as 'nothing loaded here'" effect that `AdfBrowser`,
// `HardDiskStudio`, `LhaBrowser` and `HexTools` now share. Unmounting the
// harness is navigating away; rendering it again is coming back.
//
// Against the old code — `useState<string | null>(null)` — the third assertion
// below is the one that fails: the remounted screen showed its empty
// "open an .adf to begin" page while the Dashboard's Recent list still named
// the file that had been open a second earlier.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, useState } from "react";

import { resetOpenObjects, useOpenObject, type OpenKind } from "./openObjectStore";

afterEach(cleanup);
beforeEach(resetOpenObjects);

// Named rather than written inline: a path inside a JSX *attribute* is literal
// text, so `"C:\\disks\\x.adf"` there is two backslashes while the same
// characters in a JS string are one. Constants keep both sides the same path.
const DF0 = "C:\\disks\\df0.adf";
const OTHER = "C:\\disks\\other.adf";

/** Stands in for `adfOpen`/`hdfOpen`/`lhaOpen`: reads the file, returns a parse. */
const read = vi.fn(async (p: string) => `contents of ${p}`);

beforeEach(() => read.mockClear());

/**
 * One studio's shape: an open file that belongs to the application, a parse
 * that belongs to this mount, and the effect that reconciles them.
 */
function Studio({ kind, navPath }: { kind: OpenKind; navPath?: string }) {
  const [path, setPath] = useOpenObject(kind);
  const [parsed, setParsed] = useState<string | null>(null);

  useEffect(() => {
    const target = navPath ?? path;
    if (target && (parsed === null || target !== path)) void load(target);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navPath]);

  async function load(p: string) {
    setParsed(await read(p));
    setPath(p);
  }

  return (
    <div>
      <button onClick={() => void load(DF0)}>open</button>
      <p>{parsed ?? "nothing open"}</p>
    </div>
  );
}

describe("a studio's open file", () => {
  it("is nothing before the user opens something", () => {
    render(<Studio kind="adf" />);
    expect(screen.getByText("nothing open")).toBeTruthy();
    expect(read).not.toHaveBeenCalled();
  });

  it("is what the user opened", async () => {
    render(<Studio kind="adf" />);
    await userEvent.click(screen.getByText("open"));

    expect(await screen.findByText(`contents of ${DF0}`)).toBeTruthy();
  });

  it("is still open after leaving the screen and coming back", async () => {
    const first = render(<Studio kind="adf" />);
    await userEvent.click(screen.getByText("open"));
    await screen.findByText(`contents of ${DF0}`);

    first.unmount(); // navigating away
    render(<Studio kind="adf" />); // and back

    expect(await screen.findByText(`contents of ${DF0}`)).toBeTruthy();
    // Read again rather than shown from a cached parse: a file that changed on
    // disk while the user was elsewhere must not come back stale.
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("does not leak into a screen that opens a different kind of thing", async () => {
    const adf = render(<Studio kind="adf" />);
    await userEvent.click(screen.getByText("open"));
    await screen.findByText(`contents of ${DF0}`);
    adf.unmount();

    render(<Studio kind="harddisk" />);
    expect(screen.getByText("nothing open")).toBeTruthy();
  });

  it("gives way to the file router state names, and that becomes the open one", async () => {
    const first = render(<Studio kind="adf" />);
    await userEvent.click(screen.getByText("open"));
    await screen.findByText(`contents of ${DF0}`);
    first.unmount();

    // The drop panel or the Dashboard sending the user here with a file wins
    // over what was open — that is the whole point of arriving that way.
    const second = render(<Studio kind="adf" navPath={OTHER} />);
    expect(await screen.findByText(`contents of ${OTHER}`)).toBeTruthy();

    second.unmount();
    render(<Studio kind="adf" />);
    expect(await screen.findByText(`contents of ${OTHER}`)).toBeTruthy();
  });
});
