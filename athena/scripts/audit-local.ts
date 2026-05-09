/**
 * Local asset audit: scans Athena app-data trees used for chat GGUFs, library (incl. ZIM),
 * Kiwix download roots, and offline map tiles — mirrors backend layout in `config.rs` / `kiwix_serve.rs` / `map_manager_download.rs`.
 *
 * Override data root: ATHENA_APP_DATA_DIR=/path/to/AthenaData
 */
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const APP_IDENTIFIER = "com.athena.micro";

function formatSize(bytes: number): string {
  const gb = 1024 ** 3;
  const mb = 1024 ** 2;
  if (bytes >= gb) {
    return `${(bytes / gb).toFixed(2)} GB`;
  }
  return `${(bytes / mb).toFixed(2)} MB`;
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

interface ScanRoot {
  label: string;
  absPath: string;
}

function collectRoots(athenaRoot: string): ScanRoot[] {
  return [
    { label: "models/chat (GGUF)", absPath: path.join(athenaRoot, "models", "chat") },
    { label: "library (documents, archives, ZIM nested)", absPath: path.join(athenaRoot, "library") },
    { label: "kiwix_downloads (ZIM)", absPath: path.join(athenaRoot, "kiwix_downloads") },
    { label: "ZIM_Archives", absPath: path.join(athenaRoot, "ZIM_Archives") },
    { label: "maps (offline .mbtiles / .pmtiles)", absPath: path.join(athenaRoot, "maps") },
  ];
}

function main(): void {
  const athenaRoot = resolveAthenaRoot();
  const roots = collectRoots(athenaRoot);

  console.log("Athena local asset audit");
  console.log("Data root:", athenaRoot);
  console.log("");

  let grandTotal = 0;

  for (const { label, absPath } of roots) {
    console.log(`── ${label} ──`);
    console.log(`Path: ${absPath}`);

    if (!fs.existsSync(absPath)) {
      console.log("(directory does not exist — skipped)");
      console.log("");
      continue;
    }

    let sectionBytes = 0;
    const files = [...walkFiles(absPath)].sort((a, b) => a.localeCompare(b));

    if (files.length === 0) {
      console.log("(empty)");
    } else {
      for (const filePath of files) {
        let size = 0;
        try {
          size = fs.statSync(filePath).size;
        } catch {
          console.log(`${path.basename(filePath)}\t(unreadable)`);
          continue;
        }
        sectionBytes += size;
        const rel = path.relative(absPath, filePath) || path.basename(filePath);
        console.log(`${rel}\t${formatSize(size)}`);
      }
    }

    grandTotal += sectionBytes;
    console.log(`Section subtotal: ${formatSize(sectionBytes)} (${sectionBytes.toLocaleString()} bytes)`);
    console.log("");
  }

  console.log("══════════════════════════════════════");
  console.log(`TOTAL (all scanned trees): ${formatSize(grandTotal)} (${grandTotal.toLocaleString()} bytes)`);
  console.log("══════════════════════════════════════");
}

main();
