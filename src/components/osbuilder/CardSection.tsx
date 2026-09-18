// The card's partitions, what fills them, what they measure, and the one
// line that says whether it all fits (round 4, task 8; design § 3, § 4).
//
// **Three summary registers, the mock-up's own** (U § 4.3): a caption beside
// the heading stating what the operation costs, a "free of total" line under
// it, and a `.pane-status` footer reading used-over-total. The footer is the
// line an overflow turns red, and an overflow always names the partition and
// the bytes — "it does not fit" alone leaves the user guessing which of their
// own partitions to cut.
//
// **Nothing here derives a size.** Every figure comes from `card_os_measure`
// (Q5), which measures without staging anything: unpacking 600 MB to draw a
// number is the failure the design already rejected. Every source is
// classified by `card_os_classify` (Q6) and every typed volume name by
// `card_os_check_volume_name` (Q7) — the core is the authority on all three,
// and a screen that re-derived any of them would be a second answer.
//
// **A measure in flight shows no number.** The request's own fingerprint
// (`CardBuilder.tsx`'s pattern) is what asks again; when it changes, the
// previous measurement goes at once. A figure that is no longer true is worse
// than no figure, because it looks like an answer.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useOutletContext } from "react-router-dom";
import { open, save } from "@tauri-apps/plugin-dialog";

import { CardPartitionRow } from "@/components/osbuilder/CardPartitionRow";
import { Field } from "@/components/osbuilder/Field";
import { CARD_BUILDER_ARCHIVE_KEY } from "@/lib/cardBuild";
import {
  cardOsCheckVolumeName,
  cardOsClassify,
  cardOsMeasure,
  onCardOsMeasureResult,
  type CardOsMeasureRequest,
  type CardRefusal,
  type ClassifiedSource,
  type MeasuredCard,
} from "@/lib/cardOs";
import {
  driverMissingPhrase,
  driverPhrase,
  isFixedPartition,
  isReservedPartitionName,
  measurableInputs,
  overflowPhrase,
  sizePhrase,
  totalPhrase,
  volumeNameProblemPhrase,
} from "@/lib/cardOsMeasure";
import type { CardPartitionTarget } from "@/lib/cardTarget";
import type { DropContext } from "@/lib/dropContext";
import { cardRowAt } from "@/lib/dropTarget";
import { errorPhrase } from "@/lib/errorText";
import { subscribeSafely } from "@/lib/jobs";
import { isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useCardRunTree } from "@/lib/cardRunTree";
import { usePowerMode } from "@/lib/uxmode";
import { useSettingsStore } from "@/stores/settingsStore";

/** The names the design offers before anyone types one (design § 3). */
const OFFERED_NAMES = ["Games", "Stuff", "Demos"] as const;

export function CardSection() {
  const { t } = useTranslation();
  const { session, setCardTarget } = useBuildSession();
  const powerMode = usePowerMode();

  const target = session.cardTarget;
  /**
   * **Which tree the sizes are measured against.**
   *
   * The card's own tree exists only inside a run, and it lives in
   * `@/lib/cardRunTree` — never in `session.tree`, which is the folder lane's
   * persisted, user-facing root (final review, C3). A run's tree wins while
   * one is open; otherwise the user's own remembered tree is what there is to
   * measure, which is what a user who built a folder distribution first
   * expects.
   */
  const runTree = useCardRunTree();
  const tree = runTree ?? session.tree.root;
  /**
   * **The hst-imager the user configured, forwarded** (final review, I2).
   *
   * `#[serde(default)]` on the request's `hstImagerPath` turned its absence
   * into `HstImagerState::NotConfigured`, so the measurement told a user to go
   * to Settings and point ART at an hst.imager.exe that was already sitting in
   * Settings — a refusal naming a step already done, which is the actionable
   * rule inverted. `VolumePreload` has read this key since round 3.
   */
  const hstImagerPath = useSettingsStore((state) => state.settings.hstImagerPath);
  const material = useMemo(
    () => session.material.folders.map((folder) => folder.path),
    [session.material.folders]
  );

  /**
   * **One key, two screens.** `CardBuilder.tsx` has remembered the user's
   * Emu68 archive since SD-1; this section asks the same question, so it
   * *reads* that key as its default rather than asking twice. It never
   * writes it: what this screen stores is the card target's own
   * `emu68Archive`, per release (Q2), and a late read that wrote back would
   * be exactly the overwrite ART-089 forbids.
   */
  const [builderArchive] = useRemembered<string | null>(
    CARD_BUILDER_ARCHIVE_KEY,
    isTextOrNothing,
    null
  );
  const emu68Archive = target.emu68Archive ?? builderArchive;

  const [measured, setMeasured] = useState<MeasuredCard | null>(null);
  const [refusal, setRefusal] = useState<CardRefusal | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [classified, setClassified] = useState<Record<string, ClassifiedSource>>({});
  const [adding, setAdding] = useState(false);
  const [typedName, setTypedName] = useState("");
  const [nameProblem, setNameProblem] = useState<string | null>(null);

  // The target, read by the handlers rather than closed over: a drop arrives
  // through an effect keyed on the drop, and the partition list it has to
  // edit is whatever the list is *now*.
  const targetRef = useRef(target);
  targetRef.current = target;

  const updatePartitions = useCallback(
    (change: (partitions: CardPartitionTarget[]) => CardPartitionTarget[]) => {
      const current = targetRef.current;
      setCardTarget({ ...current, partitions: change(current.partitions) });
    },
    [setCardTarget]
  );

  const addSources = useCallback(
    (row: number, paths: string[]) => {
      if (paths.length === 0) return;
      updatePartitions((partitions) =>
        partitions.map((partition, index) =>
          index === row && !isFixedPartition(partition.name)
            ? {
                ...partition,
                sources: [
                  ...partition.sources,
                  ...paths.filter((path) => !partition.sources.includes(path)),
                ],
              }
            : partition
        )
      );
    },
    [updatePartitions]
  );

  // -------------------------------------------------------------------
  // Drops (Q3): the one global listener carried the drop out; this is the
  // hit test against the one attribute a row owns.
  //
  // **Keyed on the drop, never on the pointer** (final review, I1). This used
  // to join `analyses` and `dropPosition` here, which are set at different
  // moments: merely dragging across the section re-ran it with the previous
  // drop's paths and appended them, row by row, before anything had been
  // dropped. `lastDrop` is one value with its own `seq`, so the effect runs
  // once per drop and never on a move.
  // -------------------------------------------------------------------
  const dropContext = useOutletContext<DropContext | undefined>();
  const lastDrop = dropContext?.lastDrop ?? null;
  const seq = lastDrop?.seq ?? null;
  const lastDropRef = useRef(lastDrop);
  lastDropRef.current = lastDrop;
  /** The fixed row a drop landed on, by name, so the act is answered rather
   *  than ignored (final review, minor 1). */
  const [dropRefused, setDropRefused] = useState<string | null>(null);
  useEffect(() => {
    if (seq === null) return;
    const drop = lastDropRef.current;
    if (!drop || drop.seq !== seq || drop.paths.length === 0 || !drop.at) return;
    const row = cardRowAt(drop.at);
    if (row === null) return;
    const partition = targetRef.current.partitions[row];
    if (!partition) return;
    // **System and Work look like the other rows and take nothing**, so a
    // drop on one used to vanish without a word. Say which of the two it is
    // and where its contents come from instead.
    if (isFixedPartition(partition.name)) {
      setDropRefused(partition.name);
      return;
    }
    setDropRefused(null);
    addSources(row, drop.paths);
  }, [seq, addSources]);

  // -------------------------------------------------------------------
  // What each source is (Q6)
  // -------------------------------------------------------------------
  const sourceList = JSON.stringify(target.partitions.flatMap((partition) => partition.sources));
  useEffect(() => {
    const paths: string[] = JSON.parse(sourceList);
    if (paths.length === 0) return;
    let current = true;
    void cardOsClassify(paths)
      .then((items) => {
        if (!current) return;
        setClassified((before) => {
          const next = { ...before };
          for (const item of items) next[item.path] = item;
          return next;
        });
      })
      // A classification that cannot be asked leaves the rows saying they do
      // not know yet, which is true.
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [sourceList]);

  // -------------------------------------------------------------------
  // The measurement (Q5), re-asked when its own inputs change
  // -------------------------------------------------------------------
  const request: CardOsMeasureRequest = useMemo(
    () => ({
      cardGb: target.sizeGb,
      tree: tree ?? "",
      partitions: measurableInputs(target.partitions),
      material,
      ...(target.pfs3Driver ? { pfs3Driver: target.pfs3Driver } : {}),
      ...(hstImagerPath ? { hstImagerPath } : {}),
    }),
    [target.sizeGb, target.partitions, target.pfs3Driver, tree, material, hstImagerPath]
  );
  const fingerprint = JSON.stringify(request);

  const jobRef = useRef<number | null>(null);
  const measuringRef = useRef(false);

  useEffect(() => {
    return subscribeSafely(() =>
      onCardOsMeasureResult((result) => {
        // Not ours, or not the measurement now in flight: a late answer to a
        // question about different inputs is a stale number, and a stale
        // number on this screen looks exactly like a true one.
        if (!measuringRef.current) return;
        if (jobRef.current !== null && result.jobId !== jobRef.current) return;
        measuringRef.current = false;
        jobRef.current = null;
        setMeasuring(false);
        setMeasured(result.measured);
        setRefusal(result.refusal);
      })
    );
  }, []);

  useEffect(() => {
    const asked: CardOsMeasureRequest = JSON.parse(fingerprint);
    // Nothing to measure System from yet. The sizes wait; they are not
    // guessed, and no job is started for a question the core cannot answer.
    if (!asked.tree) {
      measuringRef.current = false;
      jobRef.current = null;
      setMeasuring(false);
      setMeasured(null);
      setRefusal(null);
      return;
    }
    let current = true;
    measuringRef.current = true;
    jobRef.current = null;
    setMeasuring(true);
    setMeasured(null);
    setRefusal(null);
    void cardOsMeasure(asked)
      .then((jobId) => {
        if (current) jobRef.current = jobId;
      })
      .catch(() => {
        if (!current) return;
        measuringRef.current = false;
        setMeasuring(false);
      });
    return () => {
      current = false;
    };
  }, [fingerprint]);

  // -------------------------------------------------------------------
  // Adding a partition (Q7: the core owns the name rule)
  // -------------------------------------------------------------------
  async function addPartition(name: string) {
    const trimmed = name.trim();
    if (targetRef.current.partitions.some((partition) => partition.name === trimmed)) {
      setNameProblem(t("cardSection.nameProblem.duplicate", { name: trimmed }));
      return;
    }
    // **A name the core makes for itself is refused here** (final review,
    // minor). `check_name` passes `Work_2` — it is a legal AmigaDOS name —
    // but `plan_card_image` splits the leftover into `Work`, `Work_1`, … so a
    // user's own `Work_2` would be a second volume with that name, and the
    // row it drew could not be removed, filled or measured.
    if (isReservedPartitionName(trimmed)) {
      setNameProblem(t("cardSection.nameProblem.reserved", { name: trimmed }));
      return;
    }
    const verdict = await cardOsCheckVolumeName(trimmed);
    const problem = volumeNameProblemPhrase(verdict);
    if (problem) {
      setNameProblem(t(problem.key, problem.params));
      return;
    }
    setNameProblem(null);
    setTypedName("");
    setAdding(false);
    updatePartitions((partitions) => {
      // Before Work, which is always the last row: it is what is left over.
      const at = partitions.findIndex((partition) => isFixedPartition(partition.name) && partition.name !== "System");
      const next = [...partitions];
      next.splice(at < 0 ? next.length : at, 0, { name: trimmed, sources: [] });
      return next;
    });
  }

  async function chooseFolder(row: number) {
    const picked = await open({
      directory: true,
      multiple: true,
      title: t("cardSection.chooseFolderTitle"),
    });
    const paths = typeof picked === "string" ? [picked] : Array.isArray(picked) ? picked : [];
    addSources(row, paths);
  }

  async function chooseFiles(row: number) {
    const picked = await open({
      multiple: true,
      title: t("cardSection.chooseFilesTitle"),
    });
    const paths = typeof picked === "string" ? [picked] : Array.isArray(picked) ? picked : [];
    addSources(row, paths);
  }

  // -------------------------------------------------------------------
  // The three rows above the partitions (design § 3's sketch)
  // -------------------------------------------------------------------

  /** Where the card image is written. `card_os_prepare`'s `image`. */
  async function chooseImage() {
    const picked = await save({
      title: t("cardSection.image.chooseTitle"),
      defaultPath: "amiga.img",
      filters: [{ name: "Card image", extensions: ["img"] }],
    });
    if (typeof picked === "string") setCardTarget({ ...targetRef.current, image: picked });
  }

  async function chooseEmu68() {
    const picked = await open({
      multiple: false,
      title: t("cardSection.emu68.chooseTitle"),
      filters: [{ name: "Emu68 release", extensions: ["zip"] }],
    });
    if (typeof picked === "string")
      setCardTarget({ ...targetRef.current, emu68Archive: picked });
  }

  async function choosePfs3() {
    const picked = await open({
      multiple: false,
      title: t("cardSection.pfs3.chooseTitle"),
    });
    if (typeof picked === "string") setCardTarget({ ...targetRef.current, pfs3Driver: picked });
  }

  // -------------------------------------------------------------------
  // What the three summary registers say
  // -------------------------------------------------------------------
  // **The one gate every figure on this screen passes through.** A
  // measurement that is in flight describes inputs that have already changed,
  // so *nothing* derived from the last one is drawn while it runs — not the
  // rows, not the free line, not the heading's cost. `setMeasured(null)` in
  // the effect above says the same thing twice on purpose; this is the guard
  // that holds even if a later answer arrives between the two.
  const shown = measuring ? null : measured;
  const plan = shown?.plan ?? null;
  const overflow = refusal ? overflowPhrase(refusal) : null;
  // The missing driver has its own sentence — what to add and where ART
  // looked — so it never falls through to the generic strip.
  const driverMissing = refusal && !measuring ? driverMissingPhrase(refusal) : null;
  const otherRefusal =
    refusal && !overflow && !driverMissing ? errorPhrase(refusal) : null;
  const total = plan ? totalPhrase(plan) : null;
  const free =
    plan === null
      ? null
      : Math.max(
          0,
          plan.image_bytes - plan.partitions.reduce((sum, partition) => sum + partition.bytes, 0)
        );

  const statusLine = !tree
    ? t("cardSection.needTree")
    : measuring
      ? t("cardSection.measuring")
      : total
        ? t(total.key, total.params)
        : overflow
          ? t("cardSection.overflow.status")
          : t("cardSection.notMeasured");

  return (
    <section data-testid="card-section" style={{ marginTop: 12 }}>
      {/* **Where the card image goes.** The one value `card_os_prepare` is
          given as its `image`; per release, like everything else on this
          card. A `save` dialog rather than an `open` one, because the file
          does not exist yet — and the build refuses to start without it,
          which the sentence below says here rather than letting it be found
          when the button does nothing. */}
      <Field
        label={t("cardSection.image.label")}
        value={target.image}
        empty={t("cardSection.image.none")}
        onChoose={() => void chooseImage()}
        choose={t("common.browse")}
        hint={t("cardSection.image.hint")}
        testId="card-image-field"
        describedBy={target.image ? undefined : "card-image-blocker"}
      />
      {!target.image && (
        <p
          id="card-image-blocker"
          className="badge badge-warn"
          style={{ fontSize: 11, margin: "-6px 0 12px", display: "inline-block" }}
          data-testid="card-image-blocker"
        >
          {t("cardSection.image.blocker")}
        </p>
      )}

      {/* **The user's own Emu68 release.** ART never downloads one. The card
          builder has asked for this file since SD-1, so its remembered answer
          is this row's default — read, never written (ART-089). */}
      <Field
        label={t("cardSection.emu68.label")}
        value={emu68Archive}
        empty={t("cardSection.emu68.none")}
        onChoose={() => void chooseEmu68()}
        choose={t("common.browse")}
        hint={t("cardSection.emu68.hint")}
        testId="card-emu68-field"
      />

      {/* § 47: the explicit driver is a power-user row on the same screen.
          In beginner mode the line under the partitions says which driver
          ART found by itself, which is the answer a beginner needs. */}
      {powerMode && (
        <Field
          label={t("cardSection.pfs3.label")}
          value={target.pfs3Driver}
          empty={t("cardSection.pfs3.none")}
          onChoose={() => void choosePfs3()}
          choose={t("common.browse")}
          hint={t("cardSection.pfs3.hint")}
          testId="card-pfs3-field"
        />
      )}

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          gap: 12,
        }}
      >
        <h3 style={{ fontSize: 14, margin: "0 0 4px" }}>{t("cardSection.heading")}</h3>
        {/* Register 1: what the operation costs, beside its own heading —
            the image's real size once Rust has answered it, and **no number
            at all** before that. A card's bytes are `cardImageBytes`'s to
            state (ART-308), never a multiplication done here. */}
        <span className="faint" style={{ fontSize: 11 }} data-testid="card-heading-cost">
          {plan
            ? t("cardSection.headingCost", {
                size: t(sizePhrase(plan.image_bytes).key, sizePhrase(plan.image_bytes).params),
              })
            : t("cardSection.headingCostPending")}
        </span>
      </div>

      {/* Register 2: free of total, in that order (the mock-up's capacity
          line). Absent until something has actually been measured. */}
      {free !== null && plan !== null && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 8px" }} data-testid="card-free">
          {t("cardSection.free", {
            free: t(sizePhrase(free).key, sizePhrase(free).params),
            total: t(sizePhrase(plan.image_bytes).key, sizePhrase(plan.image_bytes).params),
          })}
        </p>
      )}

      <div className="card-prow card-pane-head">
        <span>{t("cardSection.column.name")}</span>
        <span>{t("cardSection.column.holds")}</span>
        <span className="card-prow-size">{t("cardSection.column.size")}</span>
        <span />
        <span />
      </div>

      {target.partitions.map((partition, index) => (
        <CardPartitionRow
          key={partition.name}
          index={index}
          partition={partition}
          fixed={isFixedPartition(partition.name)}
          plan={plan}
          measured={
            partition.name === "System"
              ? shown?.system
              : shown?.partitions.find((one) => one.volumeName === partition.name)
          }
          measuring={measuring}
          classified={classified}
          overflowing={overflow?.params?.partition === partition.name}
          powerMode={powerMode}
          onAddFolder={() => void chooseFolder(index)}
          onAddFiles={() => void chooseFiles(index)}
          onRemoveSource={(at) =>
            updatePartitions((partitions) =>
              partitions.map((one, where) =>
                where === index
                  ? { ...one, sources: one.sources.filter((_, position) => position !== at) }
                  : one
              )
            )
          }
          onRemove={() =>
            updatePartitions((partitions) => partitions.filter((_, where) => where !== index))
          }
          onFloor={(bytes) =>
            updatePartitions((partitions) =>
              partitions.map((one, where) => {
                if (where !== index) return one;
                // An emptied floor leaves the key absent rather than stored
                // as 0: a stored zero is itself a decision nobody made.
                return bytes === undefined
                  ? { name: one.name, sources: one.sources }
                  : { ...one, floorBytes: bytes };
              })
            )
          }
        />
      ))}

      <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 8, flexWrap: "wrap" }}>
        <button
          type="button"
          className="btn btn-sm"
          data-testid="card-add-partition"
          onClick={() => {
            setAdding((open) => !open);
            setNameProblem(null);
          }}
        >
          {t("cardSection.addPartition")}
        </button>
        {adding && (
          <>
            {OFFERED_NAMES.map((name) => (
              <button
                key={name}
                type="button"
                className="btn btn-sm"
                data-testid={`card-add-preset-${name}`}
                onClick={() => void addPartition(name)}
              >
                {name}
              </button>
            ))}
            <input
              className="input"
              style={{ maxWidth: "12em" }}
              aria-label={t("cardSection.newName.label")}
              placeholder={t("cardSection.newName.placeholder")}
              data-testid="card-new-name"
              value={typedName}
              onChange={(event) => setTypedName(event.target.value)}
            />
            <button
              type="button"
              className="btn btn-sm btn-primary"
              data-testid="card-new-name-add"
              onClick={() => void addPartition(typedName)}
            >
              {t("cardSection.newName.add")}
            </button>
          </>
        )}
      </div>
      {nameProblem && (
        <p
          className="badge badge-err"
          style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}
          data-testid="card-new-name-problem"
        >
          {nameProblem}
        </p>
      )}

      {/* The PFS3 driver the measurement found, named with where it came
          from — the design's own line. It is not a question: ART finds the
          driver in the material folders, and the only thing to answer is the
          power-user override above. */}
      {shown && (
        <p className="faint" style={{ fontSize: 11, margin: "8px 0 0" }} data-testid="card-driver">
          {t(driverPhrase(shown.driver).key, driverPhrase(shown.driver).params)}
        </p>
      )}
      {/* A drop the section could not act on — said, because the row it
          landed on looks exactly like the ones that do take sources. */}
      {dropRefused && (
        <p className="infobar" style={{ marginTop: 8 }} data-testid="card-drop-fixed">
          {/* Two literal keys rather than one call over a ternary, so
              `literal-keys.test.ts` can check both statically — the rule
              `CardPartitionRow` already follows for its own two. */}
          {dropRefused === "System"
            ? t("cardSection.dropFixed.system")
            : t("cardSection.dropFixed.work")}
        </p>
      )}

      {driverMissing && (
        <p className="infobar warn" style={{ marginTop: 8 }} data-testid="card-driver-missing">
          {t(driverMissing.key, driverMissing.params)}
        </p>
      )}

      {/* The one `.infobar`, in the variant the ending calls for (Q10). */}
      {overflow && (
        <p className="infobar err" style={{ marginTop: 8 }} data-testid="card-overflow">
          {t(overflow.key, overflow.params)}
        </p>
      )}
      {otherRefusal && (
        <p className="infobar warn" style={{ marginTop: 8 }} data-testid="card-refusal">
          {t(otherRefusal.key, otherRefusal.params)}
        </p>
      )}

      {/* Register 3: the `.pane-status` footer — used over total, one line,
          no bar. Red when the card does not fit. */}
      <p
        className={`card-pane-status${overflow ? " card-total-err" : ""}`}
        data-testid="card-total"
      >
        {statusLine}
      </p>
    </section>
  );
}
