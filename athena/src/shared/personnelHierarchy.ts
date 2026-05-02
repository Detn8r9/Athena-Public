export type PersonnelDomain =
  | "Engineering and Manufacturing"
  | "Security and Tactics"
  | "Logistics and Agriculture"
  | "Medical and Science"
  | "Intelligence and Comms";

export const PERSONNEL_DOMAINS: PersonnelDomain[] = [
  "Engineering and Manufacturing",
  "Security and Tactics",
  "Logistics and Agriculture",
  "Medical and Science",
  "Intelligence and Comms",
];

export const LEVEL5_COMMAND = ["Caesar"] as const;

export const LEVEL4_EXECUTIVE: string[] = [
  "Legate",
  "Chief of Staff",
  "Executive Officer",
  "Operations Prime",
  "Vanguard Commander",
];

export const LEVEL3_DIRECTORATES: string[] = [
  "Director of Engineering",
  "Director of Security",
  "Director of Logistics",
  "Director of Medical",
  "Director of Intelligence",
  "Director of Communications",
  "Director of Agriculture",
  "Director of Manufacturing",
];

export const LEVEL2_SECTOR_LEADERS: Record<PersonnelDomain, string[]> = {
  "Engineering and Manufacturing": [
    "Grid Supervisor",
    "Facilities Manager",
    "Heavy Machinery Lead",
    "Fabrication Chief",
    "Vehicle Fleet Manager",
    "Structural Engineer",
  ],
  "Security and Tactics": [
    "Watch Commander",
    "Recon Leader",
    "Armory Master",
    "Perimeter Overseer",
    "Internal Security Chief",
    "Training Director",
  ],
  "Logistics and Agriculture": [
    "Crop Manager",
    "Livestock Supervisor",
    "Water Systems Overseer",
    "Quartermaster Chief",
    "Supply Chain Lead",
    "Scavenge Coordinator",
  ],
  "Medical and Science": ["Clinic Manager", "Surgical Lead", "Research Director", "Sanitation Chief", "Pharmacology Head"],
  "Intelligence and Comms": ["Academy Head", "Archives Supervisor", "Radio Command Lead", "Network Administrator", "Signal Corps Leader"],
};

export const LEVEL1_ROLES: Record<PersonnelDomain, string[]> = {
  "Engineering and Manufacturing": [
    "Machinist",
    "Electrician",
    "HVAC Technician",
    "Mechanic",
    "Welder",
    "Carpenter",
    "Plumber",
    "Solar Technician",
    "CNC Operator",
    "Mason",
    "Draftsman",
    "Drone Operator",
  ],
  "Security and Tactics": [
    "Sentry",
    "Scout",
    "Armorer",
    "Guard",
    "Sniper",
    "Patrol Officer",
    "Drill Instructor",
    "Interrogator",
    "K9 Handler",
    "Gatekeeper",
  ],
  "Logistics and Agriculture": [
    "Farmer",
    "Forager",
    "Water Technician",
    "Quartermaster",
    "Botanist",
    "Butcher",
    "Hunter",
    "Courier",
    "Inventory Clerk",
    "Cook",
    "Tailor",
  ],
  "Medical and Science": ["Medic", "Nurse", "Surgeon", "Pharmacist", "Lab Technician", "Chemist", "Therapist", "Paramedic", "Dietitian"],
  "Intelligence and Comms": [
    "Teacher",
    "Archivist",
    "IT Technician",
    "Radio Operator",
    "Translator",
    "Cryptographer",
    "Data Analyst",
    "Network Engineer",
    "Librarian",
  ],
};

/**
 * Auto-mapping: given a Level 1 role (and domain), prefill higher tiers.
 * Users can always override these strings (free-text inputs).
 */
export function suggestHierarchyForLevel1(domain: PersonnelDomain): {
  domain: PersonnelDomain;
  role_l2: string;
  role_l3: string;
  role_l4: string;
} {
  const l2 = LEVEL2_SECTOR_LEADERS[domain]?.[0] ?? "Sector Leader";
  const l4 = "Executive Officer";
  const l3 =
    domain === "Engineering and Manufacturing"
      ? "Director of Engineering"
      : domain === "Security and Tactics"
        ? "Director of Security"
        : domain === "Logistics and Agriculture"
          ? "Director of Logistics"
          : domain === "Medical and Science"
            ? "Director of Medical"
            : "Director of Communications";
  return { domain, role_l2: l2, role_l3: l3, role_l4: l4 };
}

