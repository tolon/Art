// @vitest-environment jsdom
//
// Task 7 (SD-2 · G5's content layer, packages): the screen that puts an
// official (or unofficial) update package onto an AmigaOS distribution tree
// that already exists. Mocked at the same boundary `OsInstall.test.tsx`
// mocks at — the `@/lib/osinstall` wrappers around `invoke`, not
// `@tauri-apps/api` itself.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Side-effecting import: initialises the real i18next instance
// synchronously, the same way `OsInstall.test.tsx` gets one — without it,
// `useTranslation` inside the component has nothing to read from and every
// string renders as its own raw key.
import "@/i18n";
import type { CollisionReport, PackageSummary } from "@/lib/osinstall";

const packagesMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());
const addPackageMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const onAddPackageResultMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallPackages: packagesMock,
  osinstallCollisions: collisionsMock,
  osinstallAddPackage: addPackageMock,
  onOsInstallAddPackageResult: onAddPackageResultMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const { PackagePanel } = await import("@/components/osbuilder/PackagePanel");
const { osinstallCollisions: osinstallCollisionsMocked } = await import("@/lib/osinstall");

afterEach(() => {
  cleanup();
});

/** The real shipped catalogue's own shape — three packages, matching what
 *  `osinstall_packages` actually answers. Used as the default mock instead
 *  of an empty list: an empty list is not a real backend answer (there are
 *  always exactly three), and using one here would silently mask the F9/F1
 *  fix-round assertion below that reads the count straight from what
 *  loaded. */
const PACKAGES: PackageSummary[] = [
  { id: "boingbag-39-1", name: "BoingBag 3.9-1", requires: [], requiresComponents: [], available: true },
  {
    id: "boingbag-39-2",
    name: "BoingBag 3.9-2",
    requires: ["boingbag-39-1"],
    requiresComponents: [],
    available: true,
  },
  {
    id: "locale-turkish",
    name: "Türkçe catalogs (BoingBag 3.9-2)",
    requires: [],
    requiresComponents: ["locale-base"],
    available: true,
  },
];

beforeEach(() => {
  packagesMock.mockReset().mockResolvedValue(PACKAGES);
  collisionsMock.mockReset().mockResolvedValue([]);
  addPackageMock.mockReset().mockResolvedValue({ outcome: "started", job_id: 1 });
  onJobProgressMock.mockReset().mockResolvedValue(() => {});
  onAddPackageResultMock.mockReset().mockResolvedValue(() => {});
});

const REPORTS: CollisionReport[] = [
  {
    path: "Libs/x.library",
    declared: true,
    collision: { kind: "upgrade", from: "44.1", to: "45.9" },
  },
  {
    path: "C/Assign",
    declared: true,
    collision: { kind: "downgrade", from: "45.9", to: "37.4" },
  },
  {
    path: "Devs/y.device",
    declared: false,
    collision: { kind: "unversioned", fromBytes: 1024, toBytes: 2048 },
  },
];

function renderPanel() {
  vi.mocked(osinstallCollisionsMocked).mockResolvedValue(REPORTS);
  // `packageFolder` is required from Task 7's own fix round (F3/F5): the
  // preview no longer fires against an unset folder (it used to ask the
  // backend to scan `""`), so a test exercising the preview has to supply
  // one, the same way a real screen would once the user browsed for it.
  render(
    <PackagePanel treeRoot="E:/tree" packageFolder="E:/packages" chosen={["boingbag-39-1"]} />
  );
}

it("lists downgrades before upgrades", async () => {
  renderPanel();
  const rows = await screen.findAllByTestId("collision-row");
  expect(rows[0].textContent).toContain("C/Assign");
});

it("renders every collision the core reported and invents none", async () => {
  // `Identical` never reaches the panel — the core excludes it — so the
  // panel must render exactly what it was given. A filter here would be a
  // second place for that rule to live, and the two would drift.
  renderPanel();
  expect(await screen.findAllByTestId("collision-row")).toHaveLength(3);
});

it("asks once for the whole set, not once per file", async () => {
  renderPanel();
  await screen.findAllByTestId("collision-row");
  expect(screen.getAllByRole("checkbox", { name: /confirm/i })).toHaveLength(1);
});

it("marks a collision the recipe did not declare", async () => {
  renderPanel();
  const rows = await screen.findAllByTestId("collision-row");
  const row = rows.find((r) => r.textContent?.includes("Devs/y.device"));
  expect(row?.textContent).toMatch(/undeclared/i);
});

describe("the empty selection", () => {
  it("asks for nothing and shows no preview until a package is chosen", () => {
    render(<PackagePanel treeRoot="E:/tree" chosen={[]} />);
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(screen.queryAllByTestId("collision-row")).toHaveLength(0);
  });
});

describe("what ART does not do", () => {
  it("names the count of shipped packages, on screen", async () => {
    renderPanel();
    await screen.findAllByTestId("collision-row");
    // Three today — `osinstall.packages.scopeNote`'s own `{{count}}`, read
    // straight from the loaded catalogue (`PACKAGES` above, three entries),
    // not a hardcoded number this test could pass for the wrong reason.
    expect(document.body.textContent).toContain("3");
  });
});

// ---------------------------------------------------------------------------
// Fix round 1 (Task 7's own review)
// ---------------------------------------------------------------------------

describe("F1 — a downgrade is marked as one, not just coloured", () => {
  it("gives the downgrade its own heading and its own word, not an upgrade's", async () => {
    renderPanel();
    const rows = await screen.findAllByTestId("collision-row");
    const downgradeRow = rows.find((r) => r.textContent?.includes("C/Assign"));
    const upgradeRow = rows.find((r) => r.textContent?.includes("Libs/x.library"));

    expect(downgradeRow?.textContent).toMatch(/downgrade/i);
    expect(upgradeRow?.textContent).not.toMatch(/downgrade/i);
    expect(upgradeRow?.textContent).toMatch(/upgrade/i);

    // Real group headings, not a flat list — "Downgrades" and "Upgrades"
    // both appear as their own section labels.
    expect(screen.getByText(/^Downgrades/)).toBeTruthy();
    expect(screen.getByText(/^Upgrades/)).toBeTruthy();
  });
});

describe("F2 — a refused add shows the real sentence, never starts a job", () => {
  it("renders the typed refusal instead of Rust debug text", async () => {
    addPackageMock.mockResolvedValue({
      outcome: "refused",
      refusals: [
        { refusal: "package-component-missing", package: "locale-turkish", component: "locale-base" },
      ],
    });
    renderPanel();
    await screen.findAllByTestId("collision-row");

    const confirm = screen.getByRole("checkbox", { name: /confirm/i }) as HTMLInputElement;
    await userEvent.click(confirm);
    const run = screen.getByRole("button", { name: /add the chosen packages/i });
    await userEvent.click(run);

    expect(await screen.findByText(/This selection cannot be added/i)).toBeTruthy();
    // The real sentence — never `{:?}` debug output like `PackageComponentMissing { … }`.
    expect(document.body.textContent).not.toContain("PackageComponentMissing");
    expect(document.body.textContent).toMatch(/locale-base/);
  });
});

describe("F3 — a remembered pick whose archive vanished can still be unticked", () => {
  it("does not disable the checkbox for a checked-but-unavailable package", async () => {
    packagesMock.mockResolvedValue([
      { id: "boingbag-39-1", name: "BoingBag 3.9-1", requires: [], requiresComponents: [], available: false },
    ]);
    render(
      <PackagePanel treeRoot="E:/tree" packageFolder="E:/packages" chosen={["boingbag-39-1"]} />
    );

    const box = (await screen.findByRole("checkbox", {
      name: /BoingBag 3\.9-1/,
    })) as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(box.disabled).toBe(false);
    expect(screen.getByText(/no longer in the update packages folder/i)).toBeTruthy();
  });
});

describe("F3/F5 — a chosen set previews nothing until a package folder is chosen", () => {
  it("says so instead of asking the backend to scan an empty folder", () => {
    render(<PackagePanel treeRoot="E:/tree" chosen={["boingbag-39-1"]} />);
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(screen.getByText(/Choose the update packages folder above/i)).toBeTruthy();
  });
});

describe("F10 — the add job's own outcome reaches the screen", () => {
  it("shows file/directory/byte counts once the job's result event arrives", async () => {
    renderPanel();
    await screen.findAllByTestId("collision-row");

    await userEvent.click(screen.getByRole("checkbox", { name: /confirm/i }));
    await userEvent.click(screen.getByRole("button", { name: /add the chosen packages/i }));

    // The handler `onOsInstallAddPackageResult` was subscribed with —
    // invoked directly, the same way `OsInstall.test.tsx` drives
    // `onOsInstallResult`'s own mock.
    const handler = onAddPackageResultMock.mock.calls.at(-1)?.[0] as (r: unknown) => void;
    handler({ job_id: 1, outcome: { root: "E:/tree", files: 211, directories: 12, bytes: 3_200_000 } });

    expect(await screen.findByText(/211 files, 12 directories/)).toBeTruthy();
  });
});
