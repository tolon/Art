// Aminet Studio (spec §41.5.6).
//
// Search, browse and version comparison read the LOCAL catalog, so this page
// is fully usable with nothing connected. Only three actions touch the network
// — sync, download and readme — and each of them runs as a job, so the window
// never blocks on a mirror that has stopped answering.
//
// Beginner Mode hides mirrors, the cache path and the raw sync report. It hides
// only: syncing and downloading work exactly the same in both modes (§47, §48).

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { onJobProgress } from "@/lib/jobs";
import { getSettings, saveSettings } from "@/lib/settings";

import {
  ageIsCapped,
  describeSource,
  describeUpdate,
  emptyFilters,
  formatAge,
  formatSize,
  onSourcesResult,
  sourcesCheckUpdates,
  sourcesFetch,
  sourcesForgetDownload,
  sourcesInstallAdf,
  sourcesInstallVolume,
  sourcesLibrary,
  sourcesProvider,
  sourcesReadme,
  sourcesResetMirrors,
  sourcesSearch,
  sourcesSetLibraryRoot,
  sourcesSetMirrors,
  sourcesStats,
  sourcesSync,
  type CatalogStats,
  type Claim,
  type FetchOutcome,
  type InstallOutcome,
  type LibraryInfo,
  type MirrorInfo,
  type OverwritePolicy,
  type PackageMeta,
  type PackageUpdate,
  type Placement,
  type ProviderInfo,
  type ReadmeFields,
  type SearchFilters,
  type SortOrder,
  type SyncOutcome,
} from "@/lib/sources";
import { volumeScan } from "@/lib/volume";
import { usePowerMode } from "@/lib/uxmode";

/** Ordering choices, in the words a user would use. */
const SORTS: Array<{ value: SortOrder; label: string }> = [
  { value: "relevance", label: "Best match" },
  { value: "newest", label: "Newest first" },
  { value: "oldest", label: "Oldest first" },
  { value: "largest", label: "Largest first" },
  { value: "smallest", label: "Smallest first" },
  { value: "name", label: "By name" },
];

/**
 * Age choices in weeks. Aminet's column is weeks-ago and saturates at 999, so
 * these are the honest granularity — offering "since March" would be inventing
 * precision the data does not have.
 */
const AGE_CHOICES: Array<{ weeks: number | null; label: string }> = [
  { weeks: null, label: "Any age" },
  { weeks: 4, label: "Last month" },
  { weeks: 26, label: "Last 6 months" },
  { weeks: 52, label: "Last year" },
  { weeks: 260, label: "Last 5 years" },
];

const SIZE_CHOICES: Array<{ bytes: number | null; label: string }> = [
  { bytes: null, label: "Any size" },
  { bytes: 100 * 1024, label: "100 KB" },
  { bytes: 1024 * 1024, label: "1 MB" },
  { bytes: 10 * 1024 * 1024, label: "10 MB" },
  { bytes: 100 * 1024 * 1024, label: "100 MB" },
];

/** How many filters are narrowing the search, for the "More filters" badge. */
function activeFilterCount(filters: SearchFilters): number {
  return (
    (filters.name_only ? 1 : 0) +
    (filters.max_age_weeks !== null ? 1 : 0) +
    (filters.min_size_bytes !== null ? 1 : 0) +
    (filters.max_size_bytes !== null ? 1 : 0) +
    (filters.extensions.length > 0 ? 1 : 0)
  );
}

/** The categories Aminet actually uses, for one-click browsing. */
const CATEGORIES = [
  "util",
  "game",
  "gfx",
  "comm",
  "dev",
  "mus",
  "text",
  "disk",
  "biz",
  "misc",
];

export function AminetStudio() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const navigate = useNavigate();

  const [stats, setStats] = useState<CatalogStats | null>(null);
  const [provider, setProvider] = useState<ProviderInfo | null>(null);
  const [query, setQuery] = useState("");
  const [directory, setDirectory] = useState<string | null>(null);
  const [results, setResults] = useState<PackageMeta[] | null>(null);
  const [selected, setSelected] = useState<PackageMeta | null>(null);

  const [sort, setSort] = useState<SortOrder>("relevance");
  const [filters, setFilters] = useState<SearchFilters>(emptyFilters());
  const [advanced, setAdvanced] = useState(false);

  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [subfolder, setSubfolder] = useState("");
  const [overwrite, setOverwrite] = useState<OverwritePolicy>("skip");

  const [readme, setReadme] = useState<{ text: string; fields: ReadmeFields } | null>(
    null
  );
  const [readmeLoading, setReadmeLoading] = useState(false);
  const [downloads, setDownloads] = useState<
    Record<string, { outcome: FetchOutcome; placement: Placement }>
  >({});
  const [updates, setUpdates] = useState<PackageUpdate[] | null>(null);
  const [sync, setSync] = useState<SyncOutcome | null>(null);
  const [installed, setInstalled] = useState<InstallOutcome | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshStats = useCallback(() => {
    sourcesStats().then(setStats).catch((e) => setError(String(e)));
  }, []);

  const refreshLibrary = useCallback(() => {
    sourcesLibrary().then(setLibrary).catch(() => setLibrary(null));
  }, []);

  // Local-only and fast (one pass over the record file), so it re-runs after
  // every sync and every download rather than making the user ask.
  const refreshUpdates = useCallback(() => {
    sourcesCheckUpdates()
      .then(setUpdates)
      .catch(() => setUpdates(null));
  }, []);

  // The Rust side holds the download folder and the mirror order only for the
  // lifetime of the process, so what the user chose is put back before
  // anything reads it. Both restores are best-effort: a folder that has since
  // been deleted or a mirror URL that no longer parses must not stop the
  // screen from opening — it falls back to the shipped defaults and says so.
  useEffect(() => {
    void (async () => {
      const saved = await getSettings();

      if (saved.aminetRoot) {
        try {
          setLibrary(await sourcesSetLibraryRoot(saved.aminetRoot));
        } catch {
          setError(
            `Your download folder (${saved.aminetRoot}) is no longer reachable — choose it again.`
          );
          refreshLibrary();
        }
      } else {
        refreshLibrary();
      }

      if (saved.aminetMirrors && saved.aminetMirrors.length > 0) {
        try {
          setProvider(await sourcesSetMirrors(saved.aminetMirrors));
        } catch {
          sourcesProvider().then(setProvider).catch(() => setProvider(null));
        }
      } else {
        sourcesProvider().then(setProvider).catch(() => setProvider(null));
      }
    })();

    refreshStats();
    refreshUpdates();
  }, [refreshStats, refreshLibrary, refreshUpdates]);

  // Which jobs this screen is waiting on. A ref rather than state: the two
  // listeners below are registered once, and re-subscribing on every id change
  // would race with the async unlisten.
  const pending = useRef<{ action: number | null; readme: number | null }>({
    action: null,
    readme: null,
  });

  // One listener for every background result (§54): the sync, downloads and
  // readmes all arrive here.
  useEffect(() => {
    const unlisten = onSourcesResult((result) => {
      switch (result.kind) {
        case "sync":
          setSync(result.outcome);
          pending.current.action = null;
          setBusy(null);
          refreshStats();
          // A fresh catalogue is exactly what the comparison is against.
          refreshUpdates();
          break;
        case "fetch":
          setDownloads((current) => ({
            ...current,
            [result.path]: { outcome: result.outcome, placement: result.placement },
          }));
          pending.current.action = null;
          setBusy(null);
          // A download may have created a subfolder that was not there before.
          refreshLibrary();
          // And it has just been recorded, so it belongs in the list.
          refreshUpdates();
          break;
        case "install":
          setInstalled(result.outcome);
          pending.current.action = null;
          setBusy(null);
          break;
        case "readme":
          setReadme({ text: result.text, fields: result.fields });
          pending.current.readme = null;
          setReadmeLoading(false);
          break;
      }
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [refreshStats, refreshLibrary, refreshUpdates]);

  // A job that fails or is cancelled emits no result, so the result listener
  // alone would leave this screen waiting forever — the button stays disabled
  // and the readme spins. The job's own terminal state is what actually says
  // "stop waiting", and it carries the error ID worth showing (§68).
  useEffect(() => {
    const unlisten = onJobProgress((job) => {
      if (job.state.state === "running") return;

      if (job.id === pending.current.action) {
        pending.current.action = null;
        setBusy(null);
        if (job.state.state === "failed") {
          setError(`${job.state.message} (${job.state.error_code})`);
        }
      }

      if (job.id === pending.current.readme) {
        pending.current.readme = null;
        setReadmeLoading(false);
        // A missing readme is ordinary — plenty of packages have none — so it
        // is reported by the empty state, not as an error.
      }
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const runSearch = useCallback(
    async (
      text: string,
      dir: string | null,
      order: SortOrder,
      narrow: SearchFilters
    ) => {
      setError(null);
      setQuery(text);
      setDirectory(dir);
      setSort(order);
      setFilters(narrow);
      try {
        setResults(
          await sourcesSearch(text, {
            directory: dir,
            sort: order,
            filters: narrow,
          })
        );
      } catch (e) {
        setError(String(e));
        setResults(null);
      }
    },
    []
  );

  /** Re-run the current search with one thing changed. */
  const rerun = useCallback(
    (changes: {
      dir?: string | null;
      order?: SortOrder;
      narrow?: SearchFilters;
    }) => {
      void runSearch(
        query,
        changes.dir !== undefined ? changes.dir : directory,
        changes.order ?? sort,
        changes.narrow ?? filters
      );
    },
    [runSearch, query, directory, sort, filters]
  );

  async function chooseDownloadFolder() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setError(null);
    try {
      setLibrary(await sourcesSetLibraryRoot(picked));
      // Only remembered once Rust has accepted it, so a folder that failed
      // validation is not the one waiting at the next launch.
      await saveSettings({ aminetRoot: picked });
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * Replace the mirror order (Power User Mode).
   *
   * Validation lives in Rust — `sources_set_mirrors` runs every entry through
   * `Mirror::new` — so a bad URL comes back as a sentence to show, and nothing
   * is saved. Passing an empty list means "go back to the ones ART ships
   * with": the command refuses an empty list outright, so it is handled here.
   */
  async function applyMirrors(edited: MirrorInfo[]) {
    setError(null);
    try {
      if (edited.length === 0) {
        const defaults = await sourcesResetMirrors();
        setProvider(defaults);
        await saveSettings({ aminetMirrors: null });
        return;
      }
      setProvider(await sourcesSetMirrors(edited));
      await saveSettings({ aminetMirrors: edited });
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }

  async function startSync() {
    setError(null);
    setSync(null);
    setBusy("Syncing the catalogue…");
    try {
      pending.current.action = await sourcesSync();
    } catch (e) {
      setError(String(e));
      setBusy(null);
      pending.current.action = null;
    }
  }

  async function select(pkg: PackageMeta) {
    setSelected(pkg);
    setReadme(null);
    setReadmeLoading(true);
    try {
      pending.current.readme = await sourcesReadme(pkg.reference.path);
    } catch {
      // No readme is not a failure: plenty of packages have none.
      setReadmeLoading(false);
      pending.current.readme = null;
    }
  }

  async function download(pkg: PackageMeta) {
    setError(null);
    setBusy(`Downloading ${pkg.name}…`);
    try {
      pending.current.action = await sourcesFetch(pkg.reference.path, {
        subfolder,
        overwrite,
      });
    } catch (e) {
      setError(String(e));
      setBusy(null);
      pending.current.action = null;
    }
  }

  /**
   * Open the package behind an update row, so "something is newer" leads
   * straight to the readme and the Download button rather than to a search box.
   */
  async function openFromUpdate(row: PackageUpdate) {
    setError(null);
    const meta = row.current;
    if (!meta) {
      setError(`${row.name} is no longer in the catalogue, so there is nothing to open.`);
      return;
    }
    setInstalled(null);
    await select(meta);
  }

  async function forgetDownload(row: PackageUpdate) {
    setError(null);
    try {
      await sourcesForgetDownload(row.reference.path);
      refreshUpdates();
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * Install a downloaded package into a hard disk partition.
   *
   * The partition is picked here rather than assumed: an HDF holds several,
   * and installing into the wrong one is not something a user would notice
   * until the game did not start.
   */
  async function installToHdf(pkg: PackageMeta) {
    const download = downloads[pkg.reference.path];
    if (!download) {
      setError("Download it first — then it can be installed.");
      return;
    }

    const image = await open({
      multiple: false,
      filters: [{ name: "Amiga hard disk image", extensions: ["hdf", "hda", "img"] }],
    });
    if (typeof image !== "string") return;

    setError(null);
    setInstalled(null);
    setBusy(`Looking at ${image}…`);

    let choice = 0;
    try {
      const found = await volumeScan(image);
      const usable = found.volumes
        .map((volume, index) => ({ volume, index }))
        .filter(({ volume }) => volume.unsupported === null);

      if (usable.length === 0) {
        setError(
          found.volumes.length === 0
            ? "ART found no volumes in that image."
            : `ART cannot write to any partition in that image: ${found.volumes
                .map((volume) => `${volume.name} (${volume.unsupported})`)
                .join(", ")}`
        );
        setBusy(null);
        return;
      }

      if (usable.length > 1) {
        const lines = usable.map(
          ({ volume, index }) => `${index}: ${volume.name} — ${volume.filesystem}`
        );
        const answer = window.prompt(
          ["Which partition?", ...lines].join("\n"),
          String(usable[0].index)
        );
        if (answer === null) {
          setBusy(null);
          return;
        }
        const parsed = Number(answer);
        if (!usable.some(({ index }) => index === parsed)) {
          setError(`${answer} is not one of the partitions ART can write to.`);
          setBusy(null);
          return;
        }
        choice = parsed;
      } else {
        choice = usable[0].index;
      }
    } catch (e) {
      setError(String(e));
      setBusy(null);
      return;
    }

    setBusy(`Installing ${pkg.name}…`);
    try {
      pending.current.action = await sourcesInstallVolume(
        download.placement.path,
        image,
        choice
      );
    } catch (e) {
      setError(String(e));
      setBusy(null);
      pending.current.action = null;
    }
  }

  /**
   * Hand the download folder to the Collection screen (§41.5.6).
   *
   * The whole root goes over, not just the subfolder this package landed in:
   * the user's mental model is one library with folders inside it, and
   * indexing only `browser/` would hide everything they downloaded before.
   */
  function showInCollection() {
    if (!library) return;
    navigate("/collection", { state: { path: library.root } });
  }

  /**
   * Install a downloaded package into a floppy image the user picks.
   *
   * Only a package that has actually been downloaded can be installed — the
   * archive has to be on disk first, so a failure is unambiguously either the
   * download or the install, never "something went wrong somewhere".
   */
  async function installToAdf(pkg: PackageMeta) {
    const download = downloads[pkg.reference.path];
    if (!download) {
      setError("Download it first — then it can be installed.");
      return;
    }

    const adf = await open({
      multiple: false,
      filters: [{ name: "Amiga floppy image", extensions: ["adf"] }],
    });
    if (typeof adf !== "string") return;

    setError(null);
    setInstalled(null);
    setBusy(`Installing ${pkg.name}…`);
    try {
      pending.current.action = await sourcesInstallAdf(download.placement.path, adf);
    } catch (e) {
      setError(String(e));
      setBusy(null);
      pending.current.action = null;
    }
  }

  const empty = (stats?.total ?? 0) === 0;

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>{t("nav.aminet")}</h1>
      <p className="muted" style={{ marginTop: 4 }}>
        Aminet is the store. ART is the courier and the customs officer — every
        download is checked before it is kept, and nothing is installed until
        you ask.
      </p>

      <section className="card" style={{ margin: "12px 0" }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <button className="btn btn-primary" onClick={startSync} disabled={busy !== null}>
            {empty ? "Download the catalogue" : "Update the catalogue"}
          </button>
          <span className="muted" style={{ fontSize: 13 }}>
            {stats === null
              ? "Checking…"
              : empty
              ? "No catalogue yet. One sync and search works offline."
              : `${stats.total.toLocaleString()} packages, searchable offline`}
          </span>
        </div>

        {busy && (
          <div className="muted" style={{ fontSize: 12, marginTop: 8 }}>
            {busy} — progress is in the job bar, and you can stop it there.
          </div>
        )}

        {sync && <SyncSummary outcome={sync} powerMode={powerMode} />}

        {powerMode && provider && (
          <MirrorEditor provider={provider} onApply={applyMirrors} />
        )}
      </section>

      <UpdateView
        updates={updates}
        onRefresh={refreshUpdates}
        onOpen={openFromUpdate}
        onForget={forgetDownload}
      />

      <section className="card" style={{ marginBottom: 12 }}>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            rerun({});
          }}
          style={{ display: "flex", gap: 8, flexWrap: "wrap" }}
        >
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search the catalogue — e.g. AmiSSL"
            style={{ flex: 1, minWidth: 220 }}
            disabled={empty}
          />
          <select
            value={sort}
            onChange={(event) => rerun({ order: event.target.value as SortOrder })}
            disabled={empty}
            aria-label="Sort results"
          >
            {SORTS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <button className="btn" type="submit" disabled={empty}>
            Search
          </button>
        </form>

        <div style={{ display: "flex", gap: 6, marginTop: 10, flexWrap: "wrap" }}>
          <button
            className={`btn${directory === null ? " btn-primary" : ""}`}
            style={{ fontSize: 12 }}
            onClick={() => rerun({ dir: null })}
            disabled={empty}
          >
            All
          </button>
          {CATEGORIES.map((category) => (
            <button
              key={category}
              className={`btn${directory === category ? " btn-primary" : ""}`}
              style={{ fontSize: 12 }}
              onClick={() => rerun({ dir: category })}
              disabled={empty}
            >
              {category}/
            </button>
          ))}
          <button
            className="btn"
            style={{ fontSize: 12, marginLeft: "auto" }}
            onClick={() => setAdvanced((shown) => !shown)}
            disabled={empty}
          >
            {advanced ? "Hide filters" : "More filters"}
            {activeFilterCount(filters) > 0 && ` (${activeFilterCount(filters)})`}
          </button>
        </div>

        {advanced && (
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(190px, 1fr))",
              gap: 10,
              marginTop: 12,
              fontSize: 12,
            }}
          >
            <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={filters.name_only}
                onChange={(event) =>
                  rerun({ narrow: { ...filters, name_only: event.target.checked } })
                }
              />
              Search names only
            </label>

            <label>
              <div className="muted">Uploaded</div>
              <select
                value={String(filters.max_age_weeks ?? "")}
                onChange={(event) =>
                  rerun({
                    narrow: {
                      ...filters,
                      max_age_weeks: event.target.value
                        ? Number(event.target.value)
                        : null,
                    },
                  })
                }
                style={{ width: "100%" }}
              >
                {AGE_CHOICES.map((choice) => (
                  <option key={choice.label} value={choice.weeks ?? ""}>
                    {choice.label}
                  </option>
                ))}
              </select>
            </label>

            <label>
              <div className="muted">At least</div>
              <select
                value={String(filters.min_size_bytes ?? "")}
                onChange={(event) =>
                  rerun({
                    narrow: {
                      ...filters,
                      min_size_bytes: event.target.value
                        ? Number(event.target.value)
                        : null,
                    },
                  })
                }
                style={{ width: "100%" }}
              >
                {SIZE_CHOICES.map((choice) => (
                  <option key={`min-${choice.label}`} value={choice.bytes ?? ""}>
                    {choice.label}
                  </option>
                ))}
              </select>
            </label>

            <label>
              <div className="muted">At most</div>
              <select
                value={String(filters.max_size_bytes ?? "")}
                onChange={(event) =>
                  rerun({
                    narrow: {
                      ...filters,
                      max_size_bytes: event.target.value
                        ? Number(event.target.value)
                        : null,
                    },
                  })
                }
                style={{ width: "100%" }}
              >
                {SIZE_CHOICES.map((choice) => (
                  <option key={`max-${choice.label}`} value={choice.bytes ?? ""}>
                    {choice.label}
                  </option>
                ))}
              </select>
            </label>

            <label>
              <div className="muted">File type</div>
              <input
                value={filters.extensions.join(", ")}
                placeholder="lha, lzh, adf"
                onChange={(event) =>
                  setFilters({
                    ...filters,
                    extensions: event.target.value
                      .split(",")
                      .map((part) => part.trim())
                      .filter(Boolean),
                  })
                }
                onBlur={() => rerun({})}
                style={{ width: "100%" }}
              />
            </label>

            <div style={{ alignSelf: "end" }}>
              <button
                className="btn"
                style={{ fontSize: 12 }}
                onClick={() => rerun({ narrow: emptyFilters() })}
              >
                Clear filters
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="card" style={{ marginBottom: 12 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <strong style={{ fontSize: 13 }}>Downloads go to</strong>
          <code style={{ fontSize: 12, wordBreak: "break-all" }}>
            {library?.root ?? "…"}
          </code>
          <button className="btn" style={{ fontSize: 12 }} onClick={chooseDownloadFolder}>
            Change…
          </button>
        </div>

        <div
          style={{
            display: "flex",
            gap: 8,
            marginTop: 10,
            flexWrap: "wrap",
            alignItems: "end",
          }}
        >
          <label style={{ fontSize: 12, flex: 1, minWidth: 200 }}>
            <div className="muted">Subfolder (optional)</div>
            <input
              value={subfolder}
              onChange={(event) => setSubfolder(event.target.value)}
              placeholder="e.g. browser — leave empty for the folder itself"
              list="aminet-subfolders"
              style={{ width: "100%" }}
            />
            <datalist id="aminet-subfolders">
              {(library?.subfolders ?? []).map((folder) => (
                <option key={folder} value={folder} />
              ))}
            </datalist>
          </label>

          <label style={{ fontSize: 12 }}>
            <div className="muted">If the file is already there</div>
            <select
              value={overwrite}
              onChange={(event) => setOverwrite(event.target.value as OverwritePolicy)}
            >
              <option value="skip">Keep what is there</option>
              <option value="rename">Keep both</option>
              <option value="overwrite">Replace it</option>
            </select>
          </label>
        </div>

        {(library?.subfolders.length ?? 0) > 0 && (
          <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
            {library?.subfolders.slice(0, 12).map((folder) => (
              <button
                key={folder}
                className={`btn${subfolder === folder ? " btn-primary" : ""}`}
                style={{ fontSize: 11 }}
                onClick={() => setSubfolder(folder)}
              >
                {folder}
              </button>
            ))}
          </div>
        )}
      </section>

      {error && (
        <div className="badge badge-err" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: selected ? "1fr 1fr" : "1fr", gap: 12 }}>
        <section className="card">
          {results === null ? (
            <p className="muted" style={{ margin: 0 }}>
              {empty
                ? "Download the catalogue to start searching."
                : "Search, or pick a category."}
            </p>
          ) : results.length === 0 ? (
            <p className="muted" style={{ margin: 0 }}>
              Nothing matched. Every word has to match, so try fewer of them.
            </p>
          ) : (
            <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
              {results.map((pkg) => (
                <li
                  key={pkg.reference.path}
                  className="recent-item"
                  style={{
                    cursor: "pointer",
                    borderLeft:
                      selected?.reference.path === pkg.reference.path
                        ? "2px solid var(--accent)"
                        : "2px solid transparent",
                    paddingLeft: 6,
                  }}
                  onClick={() => void select(pkg)}
                >
                  <div>
                    <span className="recent-name">{pkg.name}</span>{" "}
                    <span className="faint" style={{ fontSize: 11 }}>
                      {pkg.directory} · {formatSize(pkg.size_bytes)} · {formatAge(pkg)}
                    </span>
                  </div>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {pkg.short}
                  </div>
                  {downloads[pkg.reference.path] && (
                    <div className="badge badge-ok" style={{ fontSize: 11 }}>
                      Saved to{" "}
                      {downloads[pkg.reference.path].placement.subfolder ||
                        "the download folder"}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>

        {selected && (
          <PackagePanel
            pkg={selected}
            readme={readme}
            readmeLoading={readmeLoading}
            download={downloads[selected.reference.path] ?? null}
            installed={installed}
            powerMode={powerMode}
            onDownload={() => void download(selected)}
            onInstall={() => void installToAdf(selected)}
            onInstallHdf={() => void installToHdf(selected)}
            onCollect={showInCollection}
            onClose={() => setSelected(null)}
            busy={busy !== null}
          />
        )}
      </div>
    </div>
  );
}

function SyncSummary({
  outcome,
  powerMode,
}: {
  outcome: SyncOutcome;
  powerMode: boolean;
}) {
  // A sync that was not applied is a success that deliberately changed
  // nothing. Saying "done" here would be a lie the user acts on.
  if (!outcome.applied) {
    return (
      <div className="badge badge-warn" style={{ marginTop: 8, display: "block" }}>
        {outcome.mirror} answered, but the catalogue it sent could not be read
        properly ({outcome.report.parsed} readable entries,{" "}
        {outcome.report.skipped} unreadable lines). Your existing catalogue has
        been kept. Try another mirror.
      </div>
    );
  }

  return (
    <div style={{ marginTop: 8 }}>
      <span className="badge badge-ok">
        {outcome.report.parsed.toLocaleString()} packages from {outcome.mirror}
      </span>
      {powerMode && (
        <div className="faint" style={{ fontSize: 11, marginTop: 6 }}>
          <div>
            {outcome.index_bytes.toLocaleString()} bytes of index ·{" "}
            {outcome.report.skipped} unreadable lines
            {outcome.report.truncated && " · index was longer than ART will read"}
          </div>
          {outcome.report.examples.slice(0, 5).map((example) => (
            <div key={example.line_number} style={{ wordBreak: "break-all" }}>
              line {example.line_number}: {example.reason} — {example.excerpt}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function PackagePanel({
  pkg,
  readme,
  readmeLoading,
  download,
  installed,
  powerMode,
  onDownload,
  onInstall,
  onInstallHdf,
  onCollect,
  onClose,
  busy,
}: {
  pkg: PackageMeta;
  readme: { text: string; fields: ReadmeFields } | null;
  readmeLoading: boolean;
  download: { outcome: FetchOutcome; placement: Placement } | null;
  installed: InstallOutcome | null;
  powerMode: boolean;
  onDownload: () => void;
  onInstall: () => void;
  onInstallHdf: () => void;
  onCollect: () => void;
  onClose: () => void;
  busy: boolean;
}) {
  return (
    <section className="card">
      <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
        <strong style={{ wordBreak: "break-all" }}>{pkg.name}</strong>
        <button className="btn" style={{ fontSize: 12 }} onClick={onClose}>
          Close
        </button>
      </div>

      <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        {pkg.directory} · {formatSize(pkg.size_bytes)} · {formatAge(pkg)}
        {ageIsCapped(pkg) && " (Aminet does not record exactly how much older)"}
      </div>

      <p style={{ fontSize: 13 }}>{pkg.short}</p>

      <ClaimLine label="Version" claim={pkg.version} />
      <ClaimLine label="Author" claim={pkg.author} />
      <ClaimLine label="Distribution" claim={pkg.distribution} />

      {pkg.requires.length > 0 && (
        <div className="badge badge-warn" style={{ display: "block", fontSize: 12 }}>
          The readme says this needs:{" "}
          {pkg.requires.map((claim) => claim.value).join(", ")}. ART does not
          install dependencies for you — this is what the readme claims, not
          something ART has checked.
        </div>
      )}

      <div style={{ display: "flex", gap: 8, margin: "12px 0", flexWrap: "wrap" }}>
        <button className="btn btn-primary" onClick={onDownload} disabled={busy}>
          {download ? "Download again" : "Download"}
        </button>
        <button
          className="btn"
          onClick={onInstall}
          disabled={busy || !download}
          title={
            download
              ? "Unpack this into a floppy image"
              : "Download it first — installing works from the file on your disk"
          }
        >
          Install to ADF…
        </button>
        <button
          className="btn"
          onClick={onInstallHdf}
          disabled={busy || !download}
          title={
            download
              ? "Unpack this into a partition of a hard disk image"
              : "Download it first — installing works from the file on your disk"
          }
        >
          Install to HDF…
        </button>
        {/* §41.5.6. ART's Collection is an index of a folder, not a database
            you add rows to, so this indexes the download folder rather than
            pretending to file one package away somewhere. */}
        <button
          className="btn"
          onClick={onCollect}
          disabled={busy || !download}
          title={
            download
              ? "Index your download folder in the Collection screen"
              : "Download it first — the Collection indexes files on your disk"
          }
        >
          Show in Collection
        </button>
      </div>

      {installed && (
        <div style={{ fontSize: 12 }}>
          <div className="badge badge-ok" style={{ display: "block" }}>
            {installed.files} file{installed.files === 1 ? "" : "s"} written to{" "}
            {installed.into || "the root of the disk"}
            {installed.backup && " · the previous image was backed up"}
          </div>
          {installed.skipped.length > 0 && (
            <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
              Not written: {installed.skipped.join("; ")}
            </div>
          )}
        </div>
      )}

      {download && (
        <div style={{ fontSize: 12 }}>
          {download.placement.skipped_existing ? (
            // Not a failure: the verified file is in the cache, and a file the
            // user already had was left exactly as it was.
            <div className="badge badge-warn" style={{ display: "block" }}>
              A file of that name was already in{" "}
              {download.placement.subfolder || "the download folder"} — it was
              left alone. Choose "Keep both" or "Replace it" to change that.
            </div>
          ) : (
            <div className="badge badge-ok" style={{ display: "block" }}>
              {download.outcome.from_cache
                ? "Already downloaded before — copied from the cache"
                : `Downloaded from ${download.outcome.mirror}, size and archive checked`}
              {download.placement.renamed && " · saved under a new name to keep both"}
            </div>
          )}

          <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
            {download.placement.path}
          </div>

          {powerMode && (
            <div className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
              <div>{download.outcome.bytes.toLocaleString()} bytes</div>
              <div>SHA-256 {download.outcome.sha256}</div>
              <div>Cache: {download.outcome.path}</div>
            </div>
          )}
        </div>
      )}

      <h2 style={{ fontSize: 14, marginTop: 16 }}>Readme</h2>
      {readmeLoading ? (
        <p className="muted" style={{ fontSize: 12 }}>
          Fetching…
        </p>
      ) : readme === null ? (
        <p className="muted" style={{ fontSize: 12 }}>
          No readme was available for this package.
        </p>
      ) : (
        <pre
          style={{
            fontSize: 11,
            maxHeight: 320,
            overflow: "auto",
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            margin: 0,
          }}
        >
          {readme.text}
        </pre>
      )}
    </section>
  );
}

/**
 * What you have downloaded, against what Aminet says now (§41.5.6).
 *
 * Suggests; never acts. Aminet has no version column, so "newer" is inferred
 * from the catalogue entry having changed since the download — the panel says
 * which signal fired rather than showing a bare badge, because the signals are
 * not equally strong. Everything here is local: the answer is as of the last
 * sync, and the panel says so instead of implying it just checked.
 */
function UpdateView({
  updates,
  onRefresh,
  onOpen,
  onForget,
}: {
  updates: PackageUpdate[] | null;
  onRefresh: () => void;
  onOpen: (row: PackageUpdate) => void;
  onForget: (row: PackageUpdate) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  // Nothing downloaded yet: an empty panel would be one more thing to read
  // past on the way to the search box.
  if (updates === null || updates.length === 0) return null;

  const actionable = updates.filter((row) => row.state.state === "newer");
  const attention = updates.filter(
    (row) => row.state.state === "withdrawn" || row.state.state === "file_missing"
  );
  const shown = expanded ? updates : [...actionable, ...attention];

  return (
    <section className="card" style={{ marginBottom: 12 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <strong style={{ fontSize: 14 }}>
          {actionable.length === 0
            ? `Your ${updates.length} download${updates.length === 1 ? "" : "s"} — all current`
            : `${actionable.length} of your ${updates.length} download${
                updates.length === 1 ? "" : "s"
              } ${actionable.length === 1 ? "has" : "have"} changed on Aminet`}
        </strong>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            className="btn"
            style={{ fontSize: 12 }}
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? "Only what needs attention" : "Show all"}
          </button>
          <button className="btn" style={{ fontSize: 12 }} onClick={onRefresh}>
            Re-check
          </button>
        </div>
      </div>

      <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
        Compared against your last catalogue sync — nothing was downloaded to
        work this out. ART never replaces a download on its own.
      </div>

      {shown.length === 0 ? (
        <div className="badge badge-ok" style={{ display: "block", fontSize: 12, marginTop: 8 }}>
          Everything you have downloaded matches the catalogue.
        </div>
      ) : (
        <div style={{ marginTop: 8 }}>
          {shown.map((row) => (
            <div
              key={`${row.reference.provider}:${row.reference.path}`}
              style={{
                display: "flex",
                gap: 8,
                alignItems: "baseline",
                padding: "4px 0",
                borderTop: "1px solid var(--border)",
                flexWrap: "wrap",
              }}
            >
              <span style={{ minWidth: 160, fontSize: 12, wordBreak: "break-all" }}>
                {row.name}
              </span>
              <span
                className={
                  row.state.state === "newer"
                    ? "badge badge-warn"
                    : row.state.state === "current"
                    ? "badge badge-ok"
                    : "badge"
                }
                style={{ fontSize: 11 }}
              >
                {describeUpdate(row)}
              </span>
              <span style={{ flex: 1 }} />
              {row.current && (
                <button
                  className="btn"
                  style={{ fontSize: 11 }}
                  onClick={() => onOpen(row)}
                >
                  Open
                </button>
              )}
              <button
                className="btn"
                style={{ fontSize: 11 }}
                onClick={() => onForget(row)}
                title="Stop tracking this download. The file is not touched."
              >
                Stop tracking
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/**
 * The mirror order, editable (§41.5.6, Power User Mode only).
 *
 * Order is the point, not decoration: `fetch_with_failover` walks the list top
 * to bottom, so someone in Australia moving their nearest mirror to the top is
 * the difference between a fast sync and a slow one. Edits are staged locally
 * and only sent when "Apply" is pressed, so a half-typed URL is never the live
 * configuration.
 */
function MirrorEditor({
  provider,
  onApply,
}: {
  provider: ProviderInfo;
  onApply: (mirrors: MirrorInfo[]) => Promise<void>;
}) {
  const [expanded, setExpanded] = useState(false);
  const [draft, setDraft] = useState<MirrorInfo[]>(provider.mirrors);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Reopening starts from what is actually configured, not from an abandoned
  // edit made earlier in the session.
  function toggle() {
    if (!expanded) {
      setDraft(provider.mirrors);
      setSaved(false);
    }
    setExpanded(!expanded);
  }

  function edit(index: number, patch: Partial<MirrorInfo>) {
    setDraft((rows) => rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
    setSaved(false);
  }

  function move(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= draft.length) return;
    setDraft((rows) => {
      const next = [...rows];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
    setSaved(false);
  }

  async function apply(mirrors: MirrorInfo[]) {
    setSaving(true);
    try {
      await onApply(mirrors);
      setSaved(true);
    } catch {
      // The message is already on screen; the draft stays so it can be fixed.
    } finally {
      setSaving(false);
    }
  }

  const dirty =
    draft.length !== provider.mirrors.length ||
    draft.some(
      (row, i) =>
        row.name !== provider.mirrors[i].name ||
        row.base_url !== provider.mirrors[i].base_url
    );

  return (
    <div style={{ marginTop: 8 }}>
      <div className="faint" style={{ fontSize: 11 }}>
        <div>Index: {provider.index_path}</div>
        <div style={{ wordBreak: "break-all" }}>Cache: {provider.cache_path}</div>
      </div>

      <button
        className="btn"
        style={{ fontSize: 12, marginTop: 6 }}
        onClick={toggle}
        aria-expanded={expanded}
      >
        {expanded ? "Hide mirrors" : `Mirrors (${provider.mirrors.length}) — edit`}
      </button>

      {!expanded && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          In order: {provider.mirrors.map((m) => m.name).join(" → ")}
        </div>
      )}

      {expanded && (
        <div style={{ marginTop: 8 }}>
          <div className="muted" style={{ fontSize: 11, marginBottom: 6 }}>
            Tried top to bottom. Put the one nearest you first.
          </div>

          {draft.map((mirror, index) => (
            <div
              key={index}
              style={{
                display: "flex",
                gap: 6,
                alignItems: "center",
                marginBottom: 4,
                flexWrap: "wrap",
              }}
            >
              <span className="faint" style={{ fontSize: 11, width: 16 }}>
                {index + 1}.
              </span>
              <input
                value={mirror.name}
                onChange={(event) => edit(index, { name: event.target.value })}
                placeholder="Name"
                aria-label={`Mirror ${index + 1} name`}
                style={{ width: 140, fontSize: 12 }}
              />
              <input
                value={mirror.base_url}
                onChange={(event) => edit(index, { base_url: event.target.value })}
                placeholder="https://…"
                aria-label={`Mirror ${index + 1} address`}
                style={{ flex: 1, minWidth: 220, fontSize: 12 }}
              />
              <button
                className="btn"
                style={{ fontSize: 11 }}
                onClick={() => move(index, -1)}
                disabled={index === 0}
                title="Move up"
                aria-label={`Move ${mirror.name || "mirror"} up`}
              >
                ↑
              </button>
              <button
                className="btn"
                style={{ fontSize: 11 }}
                onClick={() => move(index, 1)}
                disabled={index === draft.length - 1}
                title="Move down"
                aria-label={`Move ${mirror.name || "mirror"} down`}
              >
                ↓
              </button>
              <button
                className="btn"
                style={{ fontSize: 11 }}
                onClick={() => {
                  setDraft((rows) => rows.filter((_, i) => i !== index));
                  setSaved(false);
                }}
                title="Remove this mirror"
                aria-label={`Remove ${mirror.name || "mirror"}`}
              >
                ✕
              </button>
            </div>
          ))}

          {draft.length === 0 && (
            <div className="badge badge-warn" style={{ display: "block", fontSize: 11 }}>
              At least one mirror is needed — add one, or use "Back to defaults".
            </div>
          )}

          <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
            <button
              className="btn"
              style={{ fontSize: 12 }}
              onClick={() => {
                setDraft((rows) => [...rows, { name: "", base_url: "" }]);
                setSaved(false);
              }}
            >
              Add a mirror
            </button>
            <button
              className="btn btn-primary"
              style={{ fontSize: 12 }}
              onClick={() => void apply(draft)}
              disabled={saving || draft.length === 0 || !dirty}
            >
              Apply
            </button>
            <button
              className="btn"
              style={{ fontSize: 12 }}
              onClick={() => void apply([])}
              disabled={saving}
              title="Restore the mirrors ART ships with"
            >
              Back to defaults
            </button>
            {saved && !dirty && (
              <span className="badge badge-ok" style={{ fontSize: 11 }}>
                Saved — used from the next sync or download on
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * A claim with its provenance. §14 and §34: free text from a readme is never
 * presented at the same strength as a machine-generated index column.
 */
function ClaimLine({ label, claim }: { label: string; claim: Claim<string> | null }) {
  if (!claim) return null;
  return (
    <div style={{ fontSize: 12 }}>
      <span className="muted">{label}:</span> {claim.value}{" "}
      <span className="faint" style={{ fontSize: 11 }}>
        ({claim.confidence.toLowerCase()} confidence, from the{" "}
        {describeSource(claim.source)})
      </span>
    </div>
  );
}
