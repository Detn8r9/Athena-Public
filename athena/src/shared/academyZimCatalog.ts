/**
 * Canonical Academy Kiwix ZIM catalog. Keep URLs/file names in sync with
 * `athena/src-tauri/resources/library_manifest.json` (category "Academy" rows).
 */
export interface AcademyZimCatalogEntry {
  /** Stable id (matches Academy UI download_id / manifest row key). */
  id: string;
  name: string;
  description: string;
  url: string;
  /** Final file name on disk (mirror path basename). */
  fileName: string;
  approxSizeBytes: number;
  academyAgeGroupId: string;
  academySubjectId: string;
}

export const ACADEMY_ZIM_CATALOG: readonly AcademyZimCatalogEntry[] = [
  {
    id: "phet-en",
    name: "PhET interactive simulations (English)",
    description: "Offline PhET science and math simulations from the University of Colorado.",
    url: "https://download.kiwix.org/zim/phet/phet_en_all_2026-02.zim",
    fileName: "phet_en_all_2026-02.zim",
    approxSizeBytes: 107_000_000,
    academyAgeGroupId: "ms",
    academySubjectId: "science",
  },
  {
    id: "wikibooks-en-nopic",
    name: "WikiBooks English (text-focused)",
    description: "Open-content textbooks; nopic build is smaller than the full image edition.",
    url: "https://download.kiwix.org/zim/wikibooks/wikibooks_en_all_nopic_2026-01.zim",
    fileName: "wikibooks_en_all_nopic_2026-01.zim",
    approxSizeBytes: 2_900_000_000,
    academyAgeGroupId: "ms",
    academySubjectId: "reading",
  },
  {
    id: "khan-en",
    name: "Khan Academy (English, full library)",
    description: "Very large dump suitable only for high-capacity installs; confirm free disk space first.",
    url: "https://download.kiwix.org/zim/other/khanacademy_en_all_2023-03.zim",
    fileName: "khanacademy_en_all_2023-03.zim",
    approxSizeBytes: 168_000_000_000,
    academyAgeGroupId: "hs",
    academySubjectId: "reference",
  },
] as const;

export type AcademyZimOfferForUi = {
  id: string;
  label: string;
  description: string;
  approxSize: string;
  url: string;
  fileName: string;
};

function formatApproxSize(bytes: number): string {
  const gb = bytes / 1_000_000_000;
  if (gb >= 1) return `~${gb >= 10 ? Math.round(gb) : gb.toFixed(1)} GB`;
  const mb = bytes / 1_000_000;
  return `~${Math.round(mb)} MB`;
}

export function academyZimOffersForUi(): AcademyZimOfferForUi[] {
  return ACADEMY_ZIM_CATALOG.map((e) => ({
    id: e.id,
    label: e.name,
    description: e.description,
    approxSize: formatApproxSize(e.approxSizeBytes),
    url: e.url,
    fileName: e.fileName,
  }));
}
