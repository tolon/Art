// @vitest-environment jsdom
//
// What each ticked update would replace, asked one row at a time — and asked
// **with the file the readout resolved** (ART-289, four-tab design § 3.2 and
// its guard in § 7).
//
// The defect this closes is not a crash: two lanes of one wizard asked one
// question about one archive and got two answers, because one of them passed
// the user's own `(slot, path)` choice and the other did not. So the first
// case below is the guard, and it asserts the whole call — the destination,
// the folder **that row's** file is in, the one package, and the override —
// because dropping any one of the four is the same defect wearing a different
// hat.
//
// Mocked at the usual boundary: the `@/lib/osinstall` wrapper around
// `invoke`, never `@tauri-apps/api`. `errorText` is the real one, so a
// rejected row's sentence is the sentence a user would read.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { folderOf } from "@/lib/buildRun";
import type { CollisionReport } from "@/lib/osinstall";

const collisionsMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallCollisions: collisionsMock,
}));

const { useUpdatesPreview } = await import("@/lib/useUpdatesPreview");

const DEST = "E:\\amiga\\Amigatolon\\sonuclar";

/** Two rows whose archives sit in **different folders** — which is the whole
 *  reason the folder is taken per row from the file the slot resolved, rather
 *  than from one "archives folder" for the build. */
const BB1 = {
  packageId: "boingbag-39-1",
  slotId: "package:boingbag-39-1",
  name: "BoingBag 3.9-1",
  // The owner's own case: two copies of BoingBag 1 in one folder, and this is
  // the one they chose (`amigaInstall.archive.boingbag-39-1`).
  file: "E:\\amiga\\Amigatolon\\os39\\BoingBag39-1 (1).lha",
};
const BB2 = {
  packageId: "boingbag-39-2",
  slotId: "package:boingbag-39-2",
  name: "BoingBag 3.9-2",
  file: "E:\\amiga\\arsiv\\BoingBag39-2.lha",
};

function report(path: string): CollisionReport {
  return {
    path,
    collision: { kind: "upgrade", from: "44.1", to: "45.2" },
    declared: true,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  collisionsMock.mockReset().mockResolvedValue([]);
});

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

describe("useUpdatesPreview", () => {
  it("asks the preview with the readout's file for each ticked update", async () => {
    // **ART-289's guard.** Two ticked rows, two archives, two folders, and
    // the user's own choice for the first — one call per row, in the chain's
    // order, each one carrying that row's `(slot, path)` as the override.
    collisionsMock.mockImplementation((_tree: string, _folder: string, packages: string[]) =>
      Promise.resolve(packages[0] === BB1.packageId ? [report("C/Format")] : [])
    );

    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: DEST, destinationIsTree: true, updates: [BB1, BB2] })
    );

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(collisionsMock).toHaveBeenCalledTimes(2);
    expect(collisionsMock.mock.calls[0]).toEqual([
      DEST,
      folderOf(BB1.file),
      [BB1.packageId],
      [[BB1.slotId, BB1.file]],
    ]);
    expect(collisionsMock.mock.calls[1]).toEqual([
      DEST,
      folderOf(BB2.file),
      [BB2.packageId],
      [[BB2.slotId, BB2.file]],
    ]);
    // Two different folders, taken from the two files — not one folder for
    // the build.
    expect(collisionsMock.mock.calls[0][1]).not.toBe(collisionsMock.mock.calls[1][1]);

    // …and the answers come back in the same order, each on its own row.
    expect(result.current.previews.map((row) => row.packageId)).toEqual([
      BB1.packageId,
      BB2.packageId,
    ]);
    expect(result.current.previews[0].collisions).toEqual([report("C/Format")]);
    expect(result.current.previews[0].error).toBeNull();
  });

  it("asks one row at a time, and does not start the second before the first answers", async () => {
    // Sequential on purpose: each ask extracts a real archive into scratch,
    // and two at once is two extractions competing for one disk. The claim is
    // worth a test because nothing else on screen would show it.
    const first = deferred<CollisionReport[]>();
    collisionsMock
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => Promise.resolve([]));

    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: DEST, destinationIsTree: true, updates: [BB1, BB2] })
    );

    await waitFor(() => expect(collisionsMock).toHaveBeenCalledTimes(1));
    expect(collisionsMock).toHaveBeenCalledTimes(1);
    // Still one, and the totals are not stated while a row is unanswered.
    expect(result.current.totals).toBeNull();
    expect(result.current.loading).toBe(true);

    first.resolve([]);
    await waitFor(() => expect(collisionsMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.loading).toBe(false));
  });

  it("asks nothing about a fresh destination", async () => {
    // `osinstall_apply` refuses a destination with anything in it, so a build
    // that has a tree phase lands on an empty folder: there is nothing there
    // to collide with, and asking would be a job for a question with no
    // content.
    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: DEST, destinationIsTree: false, updates: [BB1, BB2] })
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(result.current.previews).toEqual([]);
    // Nothing is replaced, and that is a fact rather than a missing answer.
    expect(result.current.totals).toEqual({ replaced: 0 });
  });

  it("says nothing at all when there is no destination to ask about", async () => {
    // Different from the case above: an unanswered question, not an answer of
    // zero. A total here would be a claim about a folder nobody has named.
    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: null, destinationIsTree: true, updates: [BB1] })
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(result.current.totals).toBeNull();
  });

  it("drops the answers to a list the user has already changed", async () => {
    // ART-089's mechanism, and the reason the effect is keyed on primitives
    // (ART-178): an answer that lands after its question was replaced is a
    // count of a selection nobody is looking at any more.
    const first = deferred<CollisionReport[]>();
    collisionsMock.mockImplementationOnce(() => first.promise).mockResolvedValue([]);

    const { result, rerender } = renderHook(
      (props: { updates: typeof BB1[] }) =>
        useUpdatesPreview({
          destination: DEST,
          destinationIsTree: true,
          updates: props.updates,
        }),
      { initialProps: { updates: [BB1] } }
    );
    await waitFor(() => expect(collisionsMock).toHaveBeenCalledTimes(1));

    // The user unticks BoingBag 1 and ticks BoingBag 2 instead.
    rerender({ updates: [BB2] });
    await waitFor(() => expect(collisionsMock).toHaveBeenCalledTimes(2));

    // …and the superseded answer arrives late. It must not land.
    first.resolve([report("C/Format"), report("C/Version")]);
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.previews.map((row) => row.packageId)).toEqual([BB2.packageId]);
    expect(result.current.totals).toEqual({ replaced: 0 });
  });

  it("does not re-ask when the same list arrives as a fresh array", async () => {
    // ART-178: `updates` is rebuilt by its own hook on every render, so an
    // array identity in the dependency list is an endless re-preview — one
    // that extracts real archives.
    const { rerender } = renderHook(
      (props: { updates: typeof BB1[] }) =>
        useUpdatesPreview({
          destination: DEST,
          destinationIsTree: true,
          updates: props.updates,
        }),
      { initialProps: { updates: [{ ...BB1 }] } }
    );
    await waitFor(() => expect(collisionsMock).toHaveBeenCalledTimes(1));

    rerender({ updates: [{ ...BB1 }] });
    rerender({ updates: [{ ...BB1 }] });
    await new Promise((r) => setTimeout(r, 0));
    expect(collisionsMock).toHaveBeenCalledTimes(1);
  });

  it("adds the rows up, and states no total until every row has answered", async () => {
    const second = deferred<CollisionReport[]>();
    collisionsMock
      .mockImplementationOnce(() => Promise.resolve([report("C/Format"), report("C/Version")]))
      .mockImplementationOnce(() => second.promise);

    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: DEST, destinationIsTree: true, updates: [BB1, BB2] })
    );

    // One row answered, one outstanding: **no total**. A sum over half the
    // rows looks like an answer and is not one.
    await waitFor(() => expect(result.current.previews.length).toBe(1));
    expect(result.current.totals).toBeNull();

    second.resolve([report("Libs/version.library")]);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.totals).toEqual({ replaced: 3 });
  });

  it("names a row ART could not preview, and states no total over it", async () => {
    // The screen may not out-claim the core: a row whose preview threw has no
    // count, so the total it would have been part of is not stated either.
    collisionsMock
      .mockImplementationOnce(() => Promise.reject(new Error("scratch is unusable")))
      .mockImplementationOnce(() => Promise.resolve([report("C/Format")]));

    const { result } = renderHook(() =>
      useUpdatesPreview({ destination: DEST, destinationIsTree: true, updates: [BB1, BB2] })
    );
    await waitFor(() => expect(result.current.loading).toBe(false));

    // Both rows were asked — one row failing does not stop the rest.
    expect(collisionsMock).toHaveBeenCalledTimes(2);
    expect(result.current.previews[0].collisions).toBeNull();
    expect(result.current.previews[0].error).toBe(
      i18n.t("errors.verbatimNoId", { sentence: "scratch is unusable" })
    );
    expect(result.current.previews[1].collisions).toEqual([report("C/Format")]);
    expect(result.current.totals).toBeNull();
  });
});
