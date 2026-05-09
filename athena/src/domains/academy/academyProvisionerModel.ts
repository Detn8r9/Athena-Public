/** Tier → grades → shared subject list for the Academy provisioning matrix. */

export type TierId = "elementary" | "middle" | "high";

export type SubjectId = "mathematics" | "science" | "history" | "language_arts" | "technical";

export const SUBJECT_LABELS: Record<SubjectId, string> = {
  mathematics: "Mathematics",
  science: "Science",
  history: "History",
  language_arts: "Language Arts",
  technical: "Technical / Vocational",
};

/** Curated subjects that ship optional high-quality video / documentary ZIM bundles in this preview model. */
export const SUBJECT_HAS_SUPPLEMENTARY_VIDEO: Record<SubjectId, boolean> = {
  mathematics: true,
  science: true,
  history: true,
  language_arts: false,
  technical: true,
};

export interface GradeDef {
  id: string;
  label: string;
  tier: TierId;
}

export const TIER_LABELS: Record<TierId, string> = {
  elementary: "Elementary (PreK–6)",
  middle: "Middle School (7–8)",
  high: "High School (9–12)",
};

export const GRADES: GradeDef[] = [
  { id: "prek", label: "Pre-K", tier: "elementary" },
  { id: "k", label: "Kindergarten", tier: "elementary" },
  { id: "g1", label: "Grade 1", tier: "elementary" },
  { id: "g2", label: "Grade 2", tier: "elementary" },
  { id: "g3", label: "Grade 3", tier: "elementary" },
  { id: "g4", label: "Grade 4", tier: "elementary" },
  { id: "g5", label: "Grade 5", tier: "elementary" },
  { id: "g6", label: "Grade 6", tier: "elementary" },
  { id: "g7", label: "Grade 7", tier: "middle" },
  { id: "g8", label: "Grade 8", tier: "middle" },
  { id: "g9", label: "Grade 9", tier: "high" },
  { id: "g10", label: "Grade 10", tier: "high" },
  { id: "g11", label: "Grade 11", tier: "high" },
  { id: "g12", label: "Grade 12", tier: "high" },
];

export const SUBJECT_ORDER: SubjectId[] = [
  "mathematics",
  "science",
  "history",
  "language_arts",
  "technical",
];

export const SUBJECT_IDS = SUBJECT_ORDER;

export function leafKey(gradeId: string, subject: SubjectId): string {
  return `${gradeId}::${subject}`;
}

export function parseLeafKey(key: string): { gradeId: string; subject: SubjectId } | null {
  const i = key.indexOf("::");
  if (i < 0) return null;
  const gradeId = key.slice(0, i);
  const subject = key.slice(i + 2) as SubjectId;
  if (!SUBJECT_IDS.includes(subject)) return null;
  return { gradeId, subject };
}

export function leavesForGrade(gradeId: string): string[] {
  return SUBJECT_IDS.map((s) => leafKey(gradeId, s));
}

export function leavesForTier(tier: TierId): string[] {
  const keys: string[] = [];
  for (const g of GRADES) {
    if (g.tier === tier) keys.push(...leavesForGrade(g.id));
  }
  return keys;
}

/** Mock core package size per grade+subject (bytes). */
export const BASE_LEAF_BYTES = 42 * 1024 * 1024; // 42 MiB

/** Additional video / documentary archive bundle per eligible leaf when supplementary is on. */
export const SUPPLEMENTARY_LEAF_BYTES = 118 * 1024 * 1024; // 118 MiB

/** Mock “queue id” strings appended when supplementary media is enabled (for UI / future job wiring). */
export function supplementaryArchiveIdsForLeaf(gradeId: string, subject: SubjectId): string[] {
  if (!SUBJECT_HAS_SUPPLEMENTARY_VIDEO[subject]) return [];
  const slug = `${gradeId}-${subject}`;
  return [`khan-academy-videos-${slug}.zim`, `edu-channels-${slug}-curated.bundle`];
}
