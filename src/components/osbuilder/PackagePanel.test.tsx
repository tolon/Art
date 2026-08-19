// @vitest-environment jsdom
//
// Task 7 (SD-2 · G5's content layer, packages): the screen that puts an
// official (or unofficial) update package onto an AmigaOS distribution tree
// that already exists. Mocked at the same boundary `OsInstall.test.tsx`
// mocks at — the `@/lib/osinstall` wrappers around `invoke`, not
// `@tauri-apps/api` itself.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

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

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallPackages: packagesMock,
  osinstallCollisions: collisionsMock,
  osinstallAddPackage: addPackageMock,
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

beforeEach(() => {
  packagesMock.mockReset().mockResolvedValue([] as PackageSummary[]);
  collisionsMock.mockReset();
  addPackageMock.mockReset().mockResolvedValue(1);
  onJobProgressMock.mockReset().mockResolvedValue(() => {});
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
  render(<PackagePanel treeRoot="E:/tree" chosen={["boingbag-39-1"]} />);
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
    // Three today — `osinstall.packages.scopeNote`'s own `{{count}}`,
    // falling back to the shipped count while the real catalogue has not
    // loaded (no `packageFolder` was given in this render).
    expect(document.body.textContent).toContain("3");
  });
});
