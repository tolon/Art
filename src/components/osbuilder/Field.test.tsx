// @vitest-environment jsdom
//
// ART-237: `Field`'s optional `ariaLabel` used to sit as a bare `aria-label`
// on the wrapping `<div>` — an element with no ARIA role, so the HTML-ARIA
// mapping strips its accessible name and nothing was ever announced. The
// fix moves it onto the button that is the row's actual control. These
// tests are the mutation guard: putting the attribute back on the div,
// or dropping it altogether, must fail one of them.

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Field } from "@/components/osbuilder/Field";

afterEach(cleanup);

describe("Field", () => {
  it("puts ariaLabel on the choose button's own accessible name, not on a bare div", () => {
    const { container } = render(
      <Field
        label="AmigaOS 3.2 media"
        ariaLabel="AmigaOS 3.2 media"
        value={null}
        empty="None chosen"
        choose="Browse…"
        onChoose={() => {}}
      />,
    );

    // The control a screen reader can actually reach: a button whose own
    // accessible name says which field it browses for, not just "Browse…"
    // repeated once per row.
    const button = screen.getByRole("button", { name: /Browse…\s+AmigaOS 3\.2 media/ });
    expect(button).toBeInstanceOf(HTMLButtonElement);

    // The regression this test exists to catch: no element in the row may
    // carry `aria-label` without also being a real, focusable control —
    // a `<div aria-label>` is exactly ART-237's own defect, silently
    // ignored by every screen reader because a bare div carries no role.
    const labelledDivs = Array.from(container.querySelectorAll("div[aria-label]"));
    expect(labelledDivs).toHaveLength(0);
  });

  it("also names the clear button, when there is one", () => {
    render(
      <Field
        label="AmigaOS 3.2 media"
        ariaLabel="AmigaOS 3.2 media"
        value="D:\\media"
        empty="None chosen"
        choose="Browse…"
        onChoose={() => {}}
        clear="Clear"
        onClear={() => {}}
      />,
    );

    expect(
      screen.getByRole("button", { name: /Clear\s+AmigaOS 3\.2 media/ }),
    ).toBeInstanceOf(HTMLButtonElement);
  });

  it("with no ariaLabel, the choose button still has a real accessible name — its own visible text", () => {
    render(
      <Field
        label="AmigaOS install media folder"
        value={null}
        empty="None chosen"
        choose="Browse…"
        onChoose={() => {}}
      />,
    );

    // No `ariaLabel` prop at all is the ordinary, unlayered-screen case.
    // The button must still be reachable by its own plain text — an
    // `aria-label` is never required for a button whose visible text is
    // already its accessible name.
    expect(screen.getByRole("button", { name: "Browse…" })).toBeInstanceOf(HTMLButtonElement);
  });
});
