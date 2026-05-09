/**
 * Deletes orphaned / failed download chunks under `models/chat` and `library`
 * (files ending in `.tmp`, `.part`, or `.crdownload`, case-insensitive).
 *
 * Override data root: ATHENA_APP_DATA_DIR=/path/to/AthenaData
 */
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const APP_IDENTIFIER = "com.athena.micro";

const PARTIAL_SUFFIXES = [".tmp", ".part", ".crdownload"] as const;

function formatSize(bytes: number): string {
  const gb = 1024 ** 3;
  const mb = 1024 ** 2;
  if (bytes >= gb) {
    return `${(bytes / gb).toFixed(2)} GB`;
  }
  return `${(bytes / mb).toFixed(2)} MB`;
}

function isPartialFile(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return PARTIAL_SUFFIXES.some((s) => lower.endsWith(s));
}

function* walkFiles(rootDir: string): Generator<string> {
  if (!fs.existsSync(rootDir)) {
    return;
  }
  const baseStat = fs.statSync(rootDir);
  if (!baseStat.isDirectory()) {
    if (baseStat.isFile()) {
      yield rootDir;
    }
    return;
  }
  const stack: string[] = [rootDir];
  while (stack.length > 0) {
    const dir = stack.pop()!;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      const full = path.join(dir, e.name);
      try {
        if (e.isDirectory()) {
          stack.push(full);
        } else if (e.isFile()) {
          yield full;
        } else if (e.isSymbolicLink()) {
          const st = fs.statSync(full);
          if (st.isFile()) {
            yield full;
          } else if (st.isDirectory()) {
            stack.push(full);
          }
        }
      } catch {
        /* unreadable entry */
      }
    }
  }
}

function defaultAthenaDataRoot(): string {
  const home = os.homedir();
  const platform = process.platform;
  if (platform === "darwin") {
    return path.join(home, "Library", "Application Support", APP_IDENTIFIER);
  }
  if (platform === "win32") {
    const appData = process.env.APPDATA;
    if (appData && appData.trim()) {
      return path.join(appData, APP_IDENTIFIER);
    }
    return path.join(home, "AppData", "Roaming", APP_IDENTIFIER);
  }
  const xdg = process.env.XDG_DATA_HOME?.trim();
  const base = xdg && xdg.length > 0 ? xdg : path.join(home, ".local", "share");
  return path.join(base, APP_IDENTIFIER);
}

function resolveAthenaRoot(): string {
  const fromEnv = process.env.ATHENA_APP_DATA_DIR?.trim();
  if (fromEnv) {
    return path.resolve(fromEnv);
  }
  return path.resolve(defaultAthenaDataRoot());
}

function main(): void {
  const athenaRoot = resolveAthenaRoot();
  const roots = [
    { label: "models/chat", absPath: path.join(athenaRoot, "models", "chat") },
    { label: "library", absPath: path.join(athenaRoot, "library") },
  ];

  console.log("Athena prune partials (.tmp, .part, .crdownload)");
  console.log("Data root:", athenaRoot);
  console.log("");

  const candidates: string[] = [];
  for (const { absPath } of roots) {
    if (!fs.existsSync(absPath)) {
      continue;
    }
    for (const f of walkFiles(absPath)) {
      if (isPartialFile(f)) {
        candidates.push(f);
      }
    }
  }

  candidates.sort((a, b) => a.localeCompare(b));

  let freedBytes = 0;
  let deletedCount = 0;

  for (const filePath of candidates) {
    let size = 0;
    try {
      size = fs.statSync(filePath).size;
    } catch {
      console.error(`skip (stat failed)\t${filePath}`);
      continue;
    }
    try {
      fs.unlinkSync(filePath);
      freedBytes += size;
      deletedCount += 1;
      console.log(`deleted\t${filePath}\t${formatSize(size)}`);
    } catch (e) {
      console.error(`skip (delete failed)\t${filePath}\t${String(e)}`);
    }
  }

  console.log("");
  console.log("══════════════════════════════════════");
  console.log(`Deleted files: ${deletedCount}`);
  console.log(`Disk space freed: ${formatSize(freedBytes)} (${freedBytes.toLocaleString()} bytes)`);
  console.log("══════════════════════════════════════");
}

main();
