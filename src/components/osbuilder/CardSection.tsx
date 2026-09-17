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
import { open } from "@tauri-apps/plugin-dialog";

import { CardPartitionRow } from "@/components/osbuilder/CardPartitionRow";
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
  isFixedPartition,
  measurableInputs,
  overflowPhrase,
  sizePhrase,
  totalPhrase,
  volumeNameProblemPhrase,
} from "@/lib/cardOsMeasure";
import type { CardPartitionTarget } from "@/lib/cardTarget";
import { cardRowAt } from "@/lib/dropTarget";
import { errorPhrase } from "@/lib/errorText";
import { subscribeSafely } from "@/lib/jobs";
import { useBuildSession } from "@/lib/useBuildSession";
import { usePowerMode } from "@/lib/uxmode";

/** The names the design offers before anyone types one (design § 3). */
const OFFERED_NAMES = ["Games", "Stuff", "Demos"] as const;

interface DropContext {
  analyses?: { path: string }[];
  /** Already in CSS pixels: `dnd.ts` converts once, with `cssPointOf`, and a
   *  second conversion here would divide by the device ratio twice. */
  dropPosition?: { x: number; y: number } | null;
}

export function CardSection() {
  const { t } = useTranslation();
  const { session, setCardTarget } = useBuildSession();
  const powerMode = usePowerMode();

  const target = session.cardTarget;
  const tree = session.tree.root;
  const material = useMemo(
    () => session.material.folders.map((folder) => folder.path),
    [session.material.folders]
  );

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
  // Drops (Q3): the one global listener carried the position out; this is
  // the hit test against the one attribute a row owns.
  // -------------------------------------------------------------------
  const dropContext = useOutletContext<DropContext>() ?? {};
  const drop = JSON.stringify({
    paths: (dropContext?.analyses ?? []).map((entry) => entry.path),
    at: dropContext?.dropPosition ?? null,
  });
  useEffect(() => {
    const { paths, at } = JSON.parse(drop) as {
      paths: string[];
      at: { x: number; y: number } | null;
    };
    if (paths.length === 0 || !at) return;
    const row = cardRowAt(at);
    if (row === null) return;
    addSources(row, paths);
  }, [drop, addSources]);

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
    }),
    [target.sizeGb, target.partitions, target.pfs3Driver, tree, material]
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
  const otherRefusal = refusal && !overflow ? errorPhrase(refusal) : null;
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
          from — the design's own line. */}
      {shown && (
        <p className="faint" style={{ fontSize: 11, margin: "8px 0 0" }} data-testid="card-driver">
          {t("cardSection.driver.found", {
            version: `${shown.driver.version}.${shown.driver.revision}`,
            from: shown.driver.fromArchive ?? shown.driver.path,
          })}
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
