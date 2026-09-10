// @vitest-environment jsdom
//
// The Files screen's header menu — design § 5.1 and § 5.7. A checkbox per
// optional column, never one for Name, each named by its column (ART-240:
// never a repeated "Show"), and a menu a keyboard can leave.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import "@/i18n";
import { ColumnMenu } from "@/components/files/ColumnMenu";
import { DEFAULT_COLUMNS, toggleColumn } from "@/lib/columns";

afterEach(cleanup);

function renderMenu(layout = DEFAULT_COLUMNS) {
  const onToggle = vi.fn();
  const onReset = vi.fn();
  const onClose = vi.fn();
  render(
    <div>
      <button>outside</button>
      <ColumnMenu
        x={10}
        y={20}
        layout={layout}
        onToggle={onToggle}
        onReset={onReset}
        onClose={onClose}
      />
    </div>
  );
  return { onToggle, onReset, onClose };
}

describe("ColumnMenu", () => {
  it("offers a checkbox for each optional column and none for Name", () => {
    renderMenu();
    const boxes = screen.getAllByRole("menuitemcheckbox");
    expect(boxes.map((box) => box.textContent?.trim())).toEqual(["Ext", "Size", "Date", "Attr"]);
    expect(screen.queryByRole("menuitemcheckbox", { name: "Name" })).toBeNull();
    // Each is named by its own column, never a repeated word.
    expect(screen.getByRole("menuitemcheckbox", { name: "Date" })).toBeTruthy();
  });

  it("says which columns are shown", () => {
    renderMenu(toggleColumn(DEFAULT_COLUMNS, "date"));
    expect(screen.getByRole("menuitemcheckbox", { name: "Date" }).getAttribute("aria-checked")).toBe(
      "false"
    );
    expect(screen.getByRole("menuitemcheckbox", { name: "Ext" }).getAttribute("aria-checked")).toBe(
      "true"
    );
  });

  it("toggles the column that was clicked, and resets on Reset", async () => {
    const { onToggle, onReset } = renderMenu();
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: "Size" }));
    expect(onToggle).toHaveBeenCalledWith("size");
    await userEvent.click(screen.getByRole("menuitem", { name: "Reset columns" }));
    expect(onReset).toHaveBeenCalledTimes(1);
  });

  it("takes the focus when it opens, and lets go on Escape or a click outside", async () => {
    const { onClose } = renderMenu();
    expect(document.activeElement).toBe(screen.getByRole("menuitemcheckbox", { name: "Ext" }));
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByText("outside"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
