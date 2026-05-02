/** Directorate keys — must match Rust `DIRECTORATE_KEYS` in logistics_apply / db heuristics. */
export const DIRECTORATES = [
  "Infrastructure",
  "Vanguard",
  "Sustenance",
  "InformationMedical",
] as const;

export type Directorate = (typeof DIRECTORATES)[number];

export type JobTier = "sector_lead" | "personnel";

export const DIRECTORATE_LABELS: Record<Directorate, string> = {
  Infrastructure: "Infrastructure Directorate (Engineering & Maintenance)",
  Vanguard: "Vanguard Operations (Security & Tactical)",
  Sustenance: "Sustenance Logistics (Agriculture & Resources)",
  InformationMedical: "Information & Medical Command (Intel & Academy)",
};

export const SECTOR_LEADS: Record<Directorate, readonly string[]> = {
  Infrastructure: ["Fabrication Management", "Energy Systems Oversight", "Facilities Maintenance"],
  Vanguard: ["Perimeter Command", "Reconnaissance", "Armory & Procurement"],
  Sustenance: ["Agronomy & Cultivation", "Forestry Management", "Resource Allocation"],
  InformationMedical: ["Academy Hub Administration", "Medical Services", "Data Archives"],
};

export const PERSONNEL_ROLES: Record<Directorate, readonly string[]> = {
  Infrastructure: ["CNC Operator", "Solar/HVAC Technician", "Heavy Equipment Mechanic", "Network Electrician"],
  Vanguard: ["Sentry Guard", "Forward Scout", "Armorer", "Tactical Drill Instructor"],
  Sustenance: ["Crop Specialist", "Timber Surveyor", "Water Systems Technician", "Quartermaster"],
  InformationMedical: ["Curriculum Instructor", "Field Medic", "Database Archivist", "Diagnostics Technician"],
};

function titlesFor(directorate: Directorate, tier: JobTier): readonly string[] {
  return tier === "sector_lead" ? SECTOR_LEADS[directorate] : PERSONNEL_ROLES[directorate];
}

/** Roster "division" string for legacy `personnel.division` and author lines: `DirectorateLabel / Title`. */
export function formatRosterDivision(directorate: Directorate, _tier: JobTier, jobTitle: string): string {
  const short = DIRECTORATE_LABELS[directorate].split(" (")[0]?.trim() ?? directorate;
  return `${short} / ${jobTitle}`.trim();
}

/** Pre-fill rank: Sector Lead → 3, Personnel → 1 (soft default). */
export function defaultRankForTier(tier: JobTier): number {
  return tier === "sector_lead" ? 3 : 1;
}

export function isValidTitleFor(directorate: Directorate, tier: JobTier, jobTitle: string): boolean {
  return titlesFor(directorate, tier).some((t) => t === jobTitle);
}

export { titlesFor };
