// @vitest-environment jsdom
//
// ART-070: refreshing a pane must not move the keyboard.
//
// `FileManager.tsx` calls Tauri on mount, and when this was written nothing
// could render it (ART-069, closed since — see `FileManager.test.tsx`, which
// mocks that surface and renders the real page). Following
// `FileManagerFilter.test.tsx`, this remains a harness
// wired the way the real screen is rather than a render of it: two panes, a
// `focused` side, six `open*` functions that each end with `setFocused(side)`
// because that is right when the *user* opens something, and a `refresh(side)`
// built exactly as the real one now is — read the focused pane from a ref,
// re-open, put the keyboard back.
//
// The failure it pins is F5's: the copy path refreshes the **destination**
// when the job result lands, so focus jumped out of the pane the user was
// working in and the next F-key press acted on the pane they were not looking
// at. Total Commander leaves focus on the source.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useCallback, useRef, useState } from "react";

afterEach(cleanup);

type Side = "left" | "right";

/** The slice of `FileManager` this is about: focus, opening, refreshing. */
function Harness({ keepFocus = true }: { keepFocus?: boolean }) {
  const [focused, setFocused] = useState<Side>("left");
  const [listings, setListings] = useState<Record<Side, number>>({ left: 0, right: 0 });

  const focusedRef = useRef<Side>(focused);
  focusedRef.current = focused;

  // Stands in for `openLocal`/`openAdf`/`openVolume`/…: each re-reads the
  // pane and ends by focusing it, which is what makes a *refresh* steal the
  // keyboard when it calls one of them.
  const openLocal = useCallback(async (side: Side) => {
    setListings((l) => ({ ...l, [side]: l[side] + 1 }));
    setFocused(side);
  }, []);

  const refresh = useCallback(
    async (side: Side) => {
      const keepFocusOn = focusedRef.current;
      await openLocal(side);
      // `keepFocus: false` is the old behaviour, for the mutation check below.
      if (keepFocus) setFocused(keepFocusOn);
    },
    [openLocal, keepFocus]
  );

  return (
    <div>
      <p>focused: {focused}</p>
      <p>
        listings: {listings.left}/{listings.right}
      </p>
      <button onClick={() => setFocused("right")}>focus right</button>
      <button onClick={() => void openLocal("right")}>open right</button>
      <button onClick={() => void refresh("right")}>refresh right</button>
      <button onClick={() => void refresh("left")}>refresh left</button>
    </div>
  );
}

describe("refreshing a pane", () => {
  it("leaves the keyboard in the pane the user was working in", async () => {
    render(<Harness />);
    expect(screen.getByText("focused: left")).toBeTruthy();

    // F5 to the right-hand pane: the job result refreshes the destination.
    await userEvent.click(screen.getByText("refresh right"));

    expect(await screen.findByText("focused: left")).toBeTruthy();
    // …and it really did re-read that pane, rather than being a no-op that
    // trivially left focus alone.
    expect(screen.getByText("listings: 0/1")).toBeTruthy();
  });

  it("is a no-op for focus when the refreshed pane is the focused one", async () => {
    render(<Harness />);
    await userEvent.click(screen.getByText("refresh left"));

    expect(await screen.findByText("focused: left")).toBeTruthy();
    expect(screen.getByText("listings: 1/0")).toBeTruthy();
  });

  it("still follows the user when they open a pane themselves", async () => {
    // The distinction the fix rests on: *opening* is the user's instruction
    // and moves the keyboard; *refreshing* is ART's own and must not.
    render(<Harness />);
    await userEvent.click(screen.getByText("open right"));

    expect(await screen.findByText("focused: right")).toBeTruthy();
  });

  it("without the fix, the keyboard follows the refresh — which is the bug", async () => {
    render(<Harness keepFocus={false} />);
    await userEvent.click(screen.getByText("refresh right"));

    expect(await screen.findByText("focused: right")).toBeTruthy();
  });
});
