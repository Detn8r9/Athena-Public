/** Binary (IEC) byte formatting: KiB / MiB / GiB / TiB for technical previews. */
export function formatBytesBinary(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const tb = bytes / 1024 ** 4;
  if (tb >= 1) return `${tb.toFixed(2)} TiB`;
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GiB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${mb.toFixed(1)} MiB`;
  const kb = bytes / 1024;
  if (kb >= 1) return `${kb.toFixed(0)} KiB`;
  return `${Math.round(bytes)} B`;
}
