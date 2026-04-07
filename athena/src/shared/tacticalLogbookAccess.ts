/** Mirrors Rust `tactical_occupation_matches` in `lib.rs` for UI gating only. */
const TACTICAL_OCCUPATION_KEYS = [
  "soldier",
  "marine",
  "infantry",
  "sergeant",
  "officer",
  "tactical",
  "operator",
  "security",
  "military",
  "army",
  "navy",
  "airman",
  "air force",
  "national guard",
  "guard",
  "mp",
  "mps",
  "corpsman",
  "sailor",
  "coast guard",
  "ranger",
  "artillery",
  "cavalry",
  "commando",
  "recon",
  "scout",
] as const;

export function tacticalOccupationMatches(occupation: string): boolean {
  const o = occupation.trim().toLowerCase();
  return TACTICAL_OCCUPATION_KEYS.some((k) => o.includes(k));
}

/** Rank 4+ (Centurion) or military-style occupation in logbook profile — matches `can_access_tactical_logbook`. */
export function canAccessTacticalLogbook(rankLevel: number, occupation: string): boolean {
  if (rankLevel >= 4) return true;
  return tacticalOccupationMatches(occupation);
}
