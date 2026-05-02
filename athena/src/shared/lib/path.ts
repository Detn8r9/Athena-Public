export function normalizePathSlashes(p: string): string {
  return String(p || "").replace(/\\/g, "/");
}

export function joinPath(a: string, b: string): string {
  const left = String(a || "").replace(/[\\/]+$/, "");
  const right = String(b || "").replace(/^[\\/]+/, "");
  if (!left) return right;
  if (!right) return left;
  return `${left}/${right}`;
}

