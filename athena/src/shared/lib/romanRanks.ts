/** Station rank titles (Level 5 Caesar is exclusive — not assignable via Add personnel). */
export const RANK_TITLES: Record<number, string> = {
  1: "Auxiliary",
  2: "Legionary",
  3: "Centurion",
  4: "General",
  5: "Caesar",
};

/** Full role descriptions for tooltips and reference. */
export const RANK_DESCRIPTIONS: Record<number, string> = {
  1: "Basic base station personnel. Restricted strictly to the local AI chat.",
  2: "Standard operators. Passive communications (e.g. mesh inbox read) and read-only Logistics Hub inventory.",
  3: "Communications commanders. Active radio rights, Station AI queue, and inventory edits within their division.",
  4: "Tactical commanders. Tactical map, Personnel registry (add users), and inventory edits across all divisions.",
  5: "System architect master account. Full base station control, logistics log fingerprint notes, and hardware export.",
};

/** @deprecated Use RANK_TITLES */
export const ROMAN_RANK_TITLES = RANK_TITLES;

export function formatRomanRank(level: number): string {
  const title = RANK_TITLES[level];
  return title ? `Level ${level} — ${title}` : `Level ${level}`;
}

export function clampRankLevel(level: number): 1 | 2 | 3 | 4 | 5 {
  if (!Number.isFinite(level) || level < 1) return 1;
  if (level > 5) return 5;
  return Math.round(level) as 1 | 2 | 3 | 4 | 5;
}

/** Ranks that can be assigned when creating a new account (Caesar is exclusive). */
export const ASSIGNABLE_RANKS = [1, 2, 3, 4] as const;
