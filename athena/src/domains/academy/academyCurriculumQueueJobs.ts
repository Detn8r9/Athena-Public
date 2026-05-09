import type { StagedDownloadJob } from "../../shared/dataAcquisition/stagedDownloadJobs";
import {
  BASE_LEAF_BYTES,
  SUPPLEMENTARY_LEAF_BYTES,
  parseLeafKey,
  SUBJECT_HAS_SUPPLEMENTARY_VIDEO,
  type SubjectId,
} from "./academyProvisionerModel";

/** Jobs emitted by the curriculum matrix (replaceable slice in the global download cart). */
const ATHENA_CURRICULUM_FN = /athena-(curriculum|supplementary)__/;

export function isAthenaCurriculumStagedJob(job: StagedDownloadJob): boolean {
  return ATHENA_CURRICULUM_FN.test(job.fileName);
}

/** ZIM basenames (as on disk under library / kiwix roots) for one curriculum matrix leaf. */
export function zimBasenamesForCurriculumLeaf(leafKey: string, includeSupplementary: boolean): string[] {
  const parsed = parseLeafKey(leafKey);
  if (!parsed) return [];
  const { gradeId, subject } = parsed;
  const out: string[] = [`athena-curriculum__${gradeId}__${subject}.zim`];
  if (includeSupplementary && SUBJECT_HAS_SUPPLEMENTARY_VIDEO[subject as SubjectId]) {
    out.push(`athena-supplementary__${gradeId}__${subject}.zim`);
  }
  return out;
}

/** True when every archive this leaf would queue already exists locally (ZIM basename match, case-insensitive). */
export function leafArchivesFullyOnDisk(
  leafKey: string,
  includeSupplementary: boolean,
  zimBasenamesLower: ReadonlySet<string>,
): boolean {
  const names = zimBasenamesForCurriculumLeaf(leafKey, includeSupplementary);
  if (names.length === 0) return false;
  for (const n of names) {
    if (!zimBasenamesLower.has(n.toLowerCase())) return false;
  }
  return true;
}

/** Placeholder Kiwix mirror paths; deployment may resolve alternates like other ZIM jobs. */
const CURRICULUM_ZIM_URL_BASE = "https://download.kiwix.org/zim/custom/athena";

/**
 * Maps the curriculum matrix selection to concrete staged HTTP jobs (Kiwix-style URLs and
 * `kiwix_downloads/` destinations). Sizes match the matrix estimator (`BASE_LEAF_BYTES` /
 * `SUPPLEMENTARY_LEAF_BYTES`).
 */
export function selectionToAcademyKiwixJobs(
  selected: ReadonlySet<string>,
  includeSupplementary: boolean,
): StagedDownloadJob[] {
  const jobs: StagedDownloadJob[] = [];
  for (const key of selected) {
    const parsed = parseLeafKey(key);
    if (!parsed) continue;
    const { gradeId, subject } = parsed;
    const encKey = encodeURIComponent(key);
    jobs.push({
      url: `${CURRICULUM_ZIM_URL_BASE}/curriculum-leaf/${encKey}/core.zim`,
      fileName: `kiwix_downloads/athena-curriculum__${gradeId}__${subject}.zim`,
      expectedBytes: BASE_LEAF_BYTES,
    });
    if (includeSupplementary && SUBJECT_HAS_SUPPLEMENTARY_VIDEO[subject as SubjectId]) {
      jobs.push({
        url: `${CURRICULUM_ZIM_URL_BASE}/supplementary-leaf/${encKey}/media.zim`,
        fileName: `kiwix_downloads/athena-supplementary__${gradeId}__${subject}.zim`,
        expectedBytes: SUPPLEMENTARY_LEAF_BYTES,
      });
    }
  }
  return jobs;
}
