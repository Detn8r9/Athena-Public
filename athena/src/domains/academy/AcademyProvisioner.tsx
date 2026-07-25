import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { Clapperboard } from "lucide-react";
import { useDownloadQueueStore } from "../../stores/downloadQueueStore";
import { leafArchivesFullyOnDisk, selectionToAcademyKiwixJobs } from "./academyCurriculumQueueJobs";
import {
  BASE_LEAF_BYTES,
  GRADES,
  leafKey,
  leavesForGrade,
  leavesForTier,
  SUBJECT_HAS_SUPPLEMENTARY_VIDEO,
  SUBJECT_IDS,
  SUBJECT_LABELS,
  SUPPLEMENTARY_LEAF_BYTES,
  type SubjectId,
  type TierId,
  TIER_LABELS,
} from "./academyProvisionerModel";
import "./AcademyProvisioner.css";

type SelectionState = ReadonlySet<string>;

type SelAction =
  | { type: "toggle_tier"; tier: TierId; eligibleKeys?: readonly string[] }
  | { type: "toggle_grade"; gradeId: string; eligibleKeys?: readonly string[] }
  | { type: "toggle_subject"; key: string }
  | { type: "deselect_keys"; keys: readonly string[] };

function allKeysOn(selected: SelectionState, keys: readonly string[]): boolean {
  return keys.length > 0 && keys.every((k) => selected.has(k));
}

function toggleKeys(selected: SelectionState, keys: readonly string[]): Set<string> {
  const next = new Set(selected);
  const on = allKeysOn(selected, keys);
  if (on) for (const k of keys) next.delete(k);
  else for (const k of keys) next.add(k);
  return next;
}

function selectionReducer(selected: SelectionState, action: SelAction): Set<string> {
  switch (action.type) {
    case "toggle_tier": {
      const keys = action.eligibleKeys ?? leavesForTier(action.tier);
      return toggleKeys(selected, keys);
    }
    case "toggle_grade": {
      const keys = action.eligibleKeys ?? leavesForGrade(action.gradeId);
      return toggleKeys(selected, keys);
    }
    case "toggle_subject": {
      const next = new Set(selected);
      if (next.has(action.key)) next.delete(action.key);
      else next.add(action.key);
      return next;
    }
    case "deselect_keys": {
      const next = new Set(selected);
      for (const k of action.keys) next.delete(k);
      return next;
    }
    default:
      return new Set(selected);
  }
}

type Tri = "all" | "some" | "none";

function tierTri(selected: SelectionState, _tier: TierId, eligibleKeys: readonly string[]): Tri {
  if (eligibleKeys.length === 0) return "none";
  let n = 0;
  for (const k of eligibleKeys) if (selected.has(k)) n += 1;
  if (n === 0) return "none";
  if (n === eligibleKeys.length) return "all";
  return "some";
}

function gradeTri(selected: SelectionState, _gradeId: string, eligibleKeys: readonly string[]): Tri {
  if (eligibleKeys.length === 0) return "none";
  let n = 0;
  for (const k of eligibleKeys) if (selected.has(k)) n += 1;
  if (n === 0) return "none";
  if (n === eligibleKeys.length) return "all";
  return "some";
}

function TriStateCheckbox({
  tri,
  onChange,
  ariaLabel,
  disabled,
}: {
  tri: Tri;
  onChange: () => void;
  ariaLabel: string;
  disabled?: boolean;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => {
    if (ref.current) ref.current.indeterminate = tri === "some";
  }, [tri]);

  return (
    <input
      ref={ref}
      type="checkbox"
      checked={tri === "all"}
      disabled={disabled}
      onChange={onChange}
      aria-label={ariaLabel}
    />
  );
}

interface RagDoc {
  id: string;
  filename: string;
  file_type: string;
  total_chunks: number;
  ingested_at: string;
}

type DiscoveredZim = {
  path: string;
  name: string;
  size_bytes: number;
  size_label?: string;
};

const AUDIENCE_LEVELS = ["Primary", "Secondary", "Advanced", "Professional"] as const;

function estimateBytes(selected: SelectionState, includeSupplementary: boolean): number {
  let bytes = 0;
  for (const key of selected) {
    bytes += BASE_LEAF_BYTES;
    if (includeSupplementary) {
      const parts = key.split("::");
      const gradeId = parts[0];
      const subject = parts[1] as SubjectId | undefined;
      if (gradeId && subject && SUBJECT_HAS_SUPPLEMENTARY_VIDEO[subject]) {
        bytes += SUPPLEMENTARY_LEAF_BYTES;
      }
    }
  }
  return bytes;
}

export interface AcademyProvisionerProps {
  onAcademyEstimateBytesChange: (bytes: number) => void;
  /** Hub tab id: avoids clearing curriculum jobs from the cart while this tab is mounted but hidden with an empty matrix. */
  hubActiveTab: "queue" | "packages" | "academy" | "maps" | "models" | "documents";
}

export function AcademyProvisioner({ onAcademyEstimateBytesChange, hubActiveTab }: AcademyProvisionerProps) {
  const [selected, dispatch] = useReducer(selectionReducer, undefined, () => new Set<string>());
  const [includeSupplementary, setIncludeSupplementary] = useState(false);
  const [ragDocs, setRagDocs] = useState<RagDoc[]>([]);
  const [sourceDocId, setSourceDocId] = useState<string>("");
  const [audienceIdx, setAudienceIdx] = useState(1);
  const [localZims, setLocalZims] = useState<DiscoveredZim[]>([]);

  useEffect(() => {
    if (hubActiveTab !== "academy") return;
    let cancelled = false;
    void invoke<DiscoveredZim[]>("scan_kiwix_zim_files")
      .then((rows) => {
        if (!cancelled) setLocalZims(Array.isArray(rows) ? rows : []);
      })
      .catch(() => {
        if (!cancelled) setLocalZims([]);
      });
    return () => {
      cancelled = true;
    };
  }, [hubActiveTab]);

  const installedZimBasenamesLower = useMemo(() => {
    const s = new Set<string>();
    for (const z of localZims) {
      const n = String(z.name ?? "").trim().toLowerCase();
      if (n) s.add(n);
    }
    return s;
  }, [localZims]);

  const isCurriculumLeafOnDisk = useCallback(
    (leafKey: string) => leafArchivesFullyOnDisk(leafKey, includeSupplementary, installedZimBasenamesLower),
    [includeSupplementary, installedZimBasenamesLower],
  );

  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  useEffect(() => {
    const s = selectedRef.current;
    const toRemove: string[] = [];
    for (const k of s) {
      if (isCurriculumLeafOnDisk(k)) toRemove.push(k);
    }
    if (toRemove.length > 0) dispatch({ type: "deselect_keys", keys: toRemove });
  }, [isCurriculumLeafOnDisk, includeSupplementary, installedZimBasenamesLower]);

  useEffect(() => {
    void invoke<RagDoc[]>("list_rag_documents")
      .then((rows) => {
        setRagDocs(Array.isArray(rows) ? rows : []);
      })
      .catch(() => setRagDocs([]));
  }, []);

  const bytes = useMemo(
    () => estimateBytes(selected, includeSupplementary),
    [selected, includeSupplementary],
  );

  useEffect(() => {
    onAcademyEstimateBytesChange(bytes);
  }, [bytes, onAcademyEstimateBytesChange]);

  useEffect(() => {
    if (hubActiveTab !== "academy" && selected.size === 0) return;
    useDownloadQueueStore
      .getState()
      .syncAcademyCurriculumPlan(selectionToAcademyKiwixJobs(selected, includeSupplementary));
  }, [hubActiveTab, selected, includeSupplementary]);

  const gradesByTier = useMemo(() => {
    const m: Record<TierId, typeof GRADES> = { elementary: [], middle: [], high: [] };
    for (const g of GRADES) m[g.tier].push(g);
    return m;
  }, []);

  const onSubject = useCallback((key: string) => {
    dispatch({ type: "toggle_subject", key });
  }, []);

  const audienceLabel = AUDIENCE_LEVELS[Math.max(0, Math.min(AUDIENCE_LEVELS.length - 1, audienceIdx))]!;

  return (
    <div className="academy-provisioner">
      <div className="academy-provisioner__matrix-head">
        <div>
          <h2 className="academy-provisioner__h2">Curriculum selection matrix</h2>
          <p className="academy-provisioner__hint" style={{ marginTop: 0, marginBottom: 0 }}>
            Tier and grade boxes use a dash when partially selected. Your selection syncs to the global download cart
            automatically as Kiwix-style archive jobs. Matrix cells whose archives are already on disk are grayed out
            and cannot be queued again.
          </p>
        </div>
        <div className="academy-provisioner__matrix-actions">
          <label className="academy-provisioner__sup-inline" aria-label="Toggle supplementary media">
            <span className="academy-provisioner__sup-inlineText">Supplementary media</span>
            <span className="academy-provisioner__switch">
              <input
                className="academy-provisioner__switchInput"
                type="checkbox"
                checked={includeSupplementary}
                onChange={(e) => setIncludeSupplementary(e.target.checked)}
              />
              <span className="academy-provisioner__switchTrack" aria-hidden />
            </span>
          </label>
        </div>
      </div>

      {(Object.keys(TIER_LABELS) as TierId[]).map((tier) => {
        const tierEligible = leavesForTier(tier).filter((k) => !isCurriculumLeafOnDisk(k));
        const tierTriState = tierTri(selected, tier, tierEligible);
        return (
          <details key={tier} className="academy-provisioner__tier">
            <summary className="academy-provisioner__tier-head">
              <span className="academy-provisioner__tier-check" onClick={(e) => e.stopPropagation()}>
                <TriStateCheckbox
                  tri={tierTriState}
                  disabled={tierEligible.length === 0}
                  onChange={() => dispatch({ type: "toggle_tier", tier, eligibleKeys: tierEligible })}
                  ariaLabel={`Select all ${TIER_LABELS[tier]}`}
                />
              </span>
              <span className="academy-provisioner__tier-title">{TIER_LABELS[tier]}</span>
            </summary>
            <div className="academy-provisioner__tier-body">
              {gradesByTier[tier].map((grade) => {
                const gradeEligible = leavesForGrade(grade.id).filter((k) => !isCurriculumLeafOnDisk(k));
                return (
                  <GradeBlock
                    key={grade.id}
                    gradeId={grade.id}
                    gradeLabel={grade.label}
                    tri={gradeTri(selected, grade.id, gradeEligible)}
                    eligibleKeys={gradeEligible}
                    selected={selected}
                    includeSupplementary={includeSupplementary}
                    isCurriculumLeafOnDisk={isCurriculumLeafOnDisk}
                    onToggleGrade={() => dispatch({ type: "toggle_grade", gradeId: grade.id, eligibleKeys: gradeEligible })}
                    onToggleSubject={onSubject}
                  />
                );
              })}
            </div>
          </details>
        );
      })}

      <section className="academy-provisioner__ai">
        <h3>On-demand curriculum generation</h3>
        <p className="academy-provisioner__hint" style={{ marginTop: 0 }}>
          Mockup: the selected harvested document and audience level will be passed to the local LLM to build a structured syllabus in the Academy app (not wired yet).
        </p>
        <div className="academy-provisioner__field">
          <span className="academy-provisioner__label">Source document (from Documents / Media library)</span>
          <select
            className="academy-provisioner__select"
            value={sourceDocId}
            onChange={(e) => setSourceDocId(e.target.value)}
            aria-label="Source document"
          >
            <option value="">— Select ingested document —</option>
            {ragDocs.map((d) => (
              <option key={d.id} value={d.id}>
                {d.filename} ({d.file_type}) · {d.total_chunks} chunks
              </option>
            ))}
          </select>
        </div>
        <div className="academy-provisioner__field">
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: 8, maxWidth: 420 }}>
            <span className="academy-provisioner__label">Audience level</span>
            <span className="academy-provisioner__mono">{audienceLabel}</span>
          </div>
          <input
            type="range"
            min={0}
            max={AUDIENCE_LEVELS.length - 1}
            step={1}
            value={audienceIdx}
            onChange={(e) => setAudienceIdx(Number(e.target.value))}
            className="academy-provisioner__range"
            aria-valuetext={audienceLabel}
          />
        </div>
        <button
          type="button"
          className="academy-provisioner__btn"
          disabled
          title="Planned: pass selected document + audience to local LLM for structured syllabus in the Academy module."
        >
          Preview syllabus (local LLM — coming in Academy)
        </button>
      </section>
    </div>
  );
}

function GradeBlock({
  gradeId,
  gradeLabel,
  tri,
  eligibleKeys,
  selected,
  includeSupplementary,
  isCurriculumLeafOnDisk,
  onToggleGrade,
  onToggleSubject,
}: {
  gradeId: string;
  gradeLabel: string;
  tri: Tri;
  eligibleKeys: readonly string[];
  selected: SelectionState;
  includeSupplementary: boolean;
  isCurriculumLeafOnDisk: (leafKey: string) => boolean;
  onToggleGrade: () => void;
  onToggleSubject: (key: string) => void;
}) {
  return (
    <div className="academy-provisioner__grade">
      <div className="academy-provisioner__grade-head">
        <TriStateCheckbox
          tri={tri}
          disabled={eligibleKeys.length === 0}
          onChange={onToggleGrade}
          ariaLabel={`Select all subjects for ${gradeLabel}`}
        />
        <span className="academy-provisioner__grade-label">{gradeLabel}</span>
      </div>
      <div className="academy-provisioner__subjects">
        {SUBJECT_IDS.map((sid) => {
          const key = leafKey(gradeId, sid);
          const hasVid = SUBJECT_HAS_SUPPLEMENTARY_VIDEO[sid];
          const checked = selected.has(key);
          const onDisk = isCurriculumLeafOnDisk(key);
          return (
            <label
              key={key}
              className={"academy-provisioner__subject" + (onDisk ? " academy-provisioner__subject--installed" : "")}
            >
              <input type="checkbox" checked={checked} disabled={onDisk} onChange={() => onToggleSubject(key)} />
              {hasVid ? (
                <span className="academy-provisioner__vid" title="Video supplements available" aria-label="Video supplements available">
                  <Clapperboard size={14} aria-hidden />
                </span>
              ) : (
                <span style={{ width: 14 }} aria-hidden />
              )}
              <span>{SUBJECT_LABELS[sid]}</span>
              {onDisk ? (
                <span className="academy-provisioner__installed-pill" title="Already in your ZIM library">
                  On disk
                </span>
              ) : null}
              {includeSupplementary && hasVid && checked ? (
                <span className="academy-provisioner__mono" title="Queued mock archive IDs">
                  +vid
                </span>
              ) : null}
            </label>
          );
        })}
      </div>
    </div>
  );
}
