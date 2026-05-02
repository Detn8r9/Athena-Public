/** Heuristic aligned with `portable::is_probably_satellite_map_archive` (file name / path). */
export function isProbablySatelliteArchiveFileName(fileName: string): boolean {
  const lower = fileName.trim().toLowerCase();
  return (
    lower.includes("satellite") ||
    lower.startsWith("satellite_") ||
    lower.startsWith("satellite_region_") ||
    lower.includes("satellite-v2")
  );
}
