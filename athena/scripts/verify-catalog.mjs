import fs from "node:fs/promises";
import path from "node:path";

const ANSI = {
  reset: "\x1b[0m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  dim: "\x1b[2m",
};

const OK = (s) => `${ANSI.green}${s}${ANSI.reset}`;
const WARN = (s) => `${ANSI.yellow}${s}${ANSI.reset}`;
const BAD = (s) => `${ANSI.red}${s}${ANSI.reset}`;
const DIM = (s) => `${ANSI.dim}${s}${ANSI.reset}`;

const DEFAULT_FILES = [
  "src/domains/features/library-installer/manifest.json",
  "src/domains/features/academy/academy_manifest.json",
  "src/shared/academyZimCatalog.ts",
];

function parseArgs(argv) {
  const files = [];
  let concurrency = 5;
  let timeoutMs = 12_000;
  let retries = 2;
  let strict5xx = false;
  let probe = false;
  let maxRedirects = 16;
  let probeMaxBytes = 65_536;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--help" || a === "-h") {
      console.log(
        [
          "Usage: node scripts/verify-catalog.mjs [--file <path>]... [--concurrency N] [--timeoutMs N] [--retries N] [--strict5xx] [--probe] [--maxRedirects N] [--probeMaxBytes N]",
          "",
          "Modes:",
          "  Default: HEAD each URL with redirect=manual (fast catalog lint).",
          "  --probe: Follow redirects and validate the final URL is fetchable without downloading huge files:",
          "           - tries HEAD on the final URL",
          "           - if HEAD is not allowed / inconclusive, falls back to GET with Range: bytes=0-0 (or small slice)",
          "           - reads at most --probeMaxBytes from the response body (safety cap)",
          "",
          "Notes:",
          "  Some CDNs return 503 intermittently; use --retries.",
          "  Use --strict5xx to treat 5xx as fatal in both default HEAD mode and --probe mode.",
        ].join("\n")
      );
      process.exit(0);
    }
    if (a === "--file" || a === "-f") {
      const v = argv[i + 1];
      if (v) files.push(v);
      i++;
      continue;
    }
    if (a.startsWith("--concurrency=")) {
      concurrency = Number(a.split("=", 2)[1]) || concurrency;
      continue;
    }
    if (a === "--concurrency") {
      concurrency = Number(argv[i + 1]) || concurrency;
      i++;
      continue;
    }
    if (a.startsWith("--timeoutMs=")) {
      timeoutMs = Number(a.split("=", 2)[1]) || timeoutMs;
      continue;
    }
    if (a === "--timeoutMs") {
      timeoutMs = Number(argv[i + 1]) || timeoutMs;
      i++;
      continue;
    }
    if (a.startsWith("--retries=")) {
      retries = Number(a.split("=", 2)[1]) || retries;
      continue;
    }
    if (a === "--retries") {
      retries = Number(argv[i + 1]) || retries;
      i++;
      continue;
    }
    if (a === "--strict5xx") {
      strict5xx = true;
      continue;
    }
    if (a === "--probe") {
      probe = true;
      continue;
    }
    if (a.startsWith("--maxRedirects=")) {
      maxRedirects = Number(a.split("=", 2)[1]) || maxRedirects;
      continue;
    }
    if (a === "--maxRedirects") {
      maxRedirects = Number(argv[i + 1]) || maxRedirects;
      i++;
      continue;
    }
    if (a.startsWith("--probeMaxBytes=")) {
      probeMaxBytes = Number(a.split("=", 2)[1]) || probeMaxBytes;
      continue;
    }
    if (a === "--probeMaxBytes") {
      probeMaxBytes = Number(argv[i + 1]) || probeMaxBytes;
      i++;
      continue;
    }
  }
  return {
    files: files.length ? files : DEFAULT_FILES,
    concurrency: Math.max(1, Math.min(25, Math.floor(concurrency))),
    timeoutMs: Math.max(1500, Math.min(60_000, Math.floor(timeoutMs))),
    retries: Math.max(0, Math.min(10, Math.floor(retries))),
    strict5xx,
    probe,
    maxRedirects: Math.max(0, Math.min(32, Math.floor(maxRedirects))),
    probeMaxBytes: Math.max(1, Math.min(2_000_000, Math.floor(probeMaxBytes))),
  };
}

function uniq(arr) {
  return Array.from(new Set(arr));
}

function extractUrlsFromText(text) {
  const re = /\bhttps?:\/\/[^\s"'<>\\]+/gi;
  const matches = text.match(re) ?? [];
  // Strip trailing punctuation common in JSON/TS.
  return matches.map((u) => u.replace(/[),\].]+$/g, ""));
}

function extractUrlsFromJson(value) {
  const urls = [];
  const walk = (v) => {
    if (!v) return;
    if (Array.isArray(v)) {
      for (const x of v) walk(x);
      return;
    }
    if (typeof v === "object") {
      for (const [k, x] of Object.entries(v)) {
        if (k.toLowerCase() === "url" && typeof x === "string") urls.push(x);
        walk(x);
      }
    }
  };
  walk(value);
  return urls;
}

async function loadCatalogUrls(repoRoot, relPath) {
  const abs = path.resolve(repoRoot, relPath);
  const raw = await fs.readFile(abs, "utf8");
  if (relPath.endsWith(".json")) {
    try {
      const parsed = JSON.parse(raw);
      return extractUrlsFromJson(parsed);
    } catch {
      // Fall back to regex extraction if JSON is malformed.
      return extractUrlsFromText(raw);
    }
  }
  return extractUrlsFromText(raw);
}

async function headCheck(url, timeoutMs) {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(url, {
      method: "HEAD",
      redirect: "manual",
      signal: ctrl.signal,
      headers: {
        // Some CDNs behave better with a UA.
        "user-agent": "athena-verify-catalog/2.0",
      },
    });
    const status = res.status;
    const location = res.headers.get("location");
    return { ok: true, status, location };
  } catch (e) {
    return { ok: false, status: null, location: null, error: e instanceof Error ? e.message : String(e) };
  } finally {
    clearTimeout(t);
  }
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function headCheckWithRetries(url, timeoutMs, retries) {
  let last = null;
  for (let attempt = 0; attempt <= retries; attempt++) {
    const r = await headCheck(url, timeoutMs);
    last = r;
    if (!r.ok) {
      // network/timeout → retry
    } else if (r.status >= 500 && r.status <= 599) {
      // upstream/server error → retry
    } else {
      return r;
    }
    if (attempt < retries) {
      const backoff = 400 * 2 ** attempt;
      await sleep(backoff);
    }
  }
  return last ?? { ok: false, status: null, location: null, error: "request failed" };
}

function resolveLocation(baseUrl, location) {
  if (!location) return null;
  try {
    return new URL(location, baseUrl).toString();
  } catch {
    return null;
  }
}

async function drainBodyUpTo(res, maxBytes) {
  if (!res.body) return 0;
  const reader = res.body.getReader();
  let total = 0;
  try {
    while (total < maxBytes) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value && value.byteLength) total += value.byteLength;
    }
  } finally {
    try {
      await reader.cancel();
    } catch {
      // ignore
    }
  }
  return total;
}

function probeFail(message, { status } = {}) {
  return { ok: false, error: message, status: status ?? null };
}

async function probeUrlOnce(url, timeoutMs, maxRedirects, probeMaxBytes) {
  let current = url;
  const chain = [url];
  let redirectsLeft = maxRedirects;

  // Phase 1: follow redirects via HEAD when possible (cheap).
  while (true) {
    if (redirectsLeft < 0) return probeFail(`too many redirects (>${maxRedirects})`);

    const headCtrl = new AbortController();
    const headTimer = setTimeout(() => headCtrl.abort(), timeoutMs);
    let headRes;
    try {
      headRes = await fetch(current, {
        method: "HEAD",
        redirect: "manual",
        signal: headCtrl.signal,
        headers: {
          "user-agent": "athena-verify-catalog/2.0 (probe)",
        },
      });
    } catch {
      // Some hosts time out or reset HEAD even when GET works; fall through to ranged GET.
      headRes = null;
    } finally {
      clearTimeout(headTimer);
    }

    if (!headRes) break;

    if (headRes.status >= 300 && headRes.status <= 399) {
      const loc = headRes.headers.get("location");
      const next = resolveLocation(current, loc);
      if (!next) return probeFail(`HEAD redirect ${headRes.status} without Location`, { status: headRes.status });
      redirectsLeft--;
      if (chain[chain.length - 1] !== next) chain.push(next);
      current = next;
      continue;
    }

    if (headRes.status >= 200 && headRes.status <= 299) {
      const acceptRanges = headRes.headers.get("accept-ranges");
      const len = headRes.headers.get("content-length");
      return {
        ok: true,
        finalUrl: current,
        chain,
        mode: "head",
        status: headRes.status,
        acceptRanges,
        contentLength: len,
      };
    }

    // Many large-file mirrors don't implement HEAD (404/403/etc). Fall through to a tiny ranged GET.
    break;
  }

  // Phase 2: ranged GET against `current`, following redirects manually, with a hard read cap.
  while (true) {
    if (redirectsLeft < 0) return probeFail(`too many redirects (>${maxRedirects})`);

    const getCtrl = new AbortController();
    const getTimer = setTimeout(() => getCtrl.abort(), timeoutMs);
    let getRes;
    try {
      getRes = await fetch(current, {
        method: "GET",
        redirect: "manual",
        signal: getCtrl.signal,
        headers: {
          "user-agent": "athena-verify-catalog/2.0 (probe)",
          range: "bytes=0-0",
        },
      });
    } catch (e) {
      return probeFail(e instanceof Error ? e.message : String(e));
    } finally {
      clearTimeout(getTimer);
    }

    if (getRes.status >= 300 && getRes.status <= 399) {
      const loc = getRes.headers.get("location");
      const next = resolveLocation(current, loc);
      await drainBodyUpTo(getRes, Math.min(probeMaxBytes, 4096));
      if (!next) return probeFail(`GET redirect ${getRes.status} without Location`, { status: getRes.status });
      redirectsLeft--;
      if (chain[chain.length - 1] !== next) chain.push(next);
      current = next;
      continue;
    }

    if (getRes.status === 416) {
      return probeFail("GET probe got 416 Range Not Satisfiable (empty or non-range resource?)", { status: 416 });
    }

    if (!(getRes.status >= 200 && getRes.status <= 299)) {
      return probeFail(`GET probe failed (${getRes.status})`, { status: getRes.status });
    }

    const cr = getRes.headers.get("content-range");
    const len = getRes.headers.get("content-length");
    const read = await drainBodyUpTo(getRes, probeMaxBytes);

    return {
      ok: true,
      finalUrl: current,
      chain,
      mode: getRes.status === 206 ? "get-range" : "get",
      status: getRes.status,
      contentRange: cr,
      contentLength: len,
      bytesRead: read,
    };
  }
}

async function probeUrlWithRetries(url, timeoutMs, retries, maxRedirects, probeMaxBytes) {
  let last = probeFail("probe failed");
  for (let attempt = 0; attempt <= retries; attempt++) {
    const r = await probeUrlOnce(url, timeoutMs, maxRedirects, probeMaxBytes);
    if (r.ok) return r;

    last = r;

    // Retry on typical transient conditions
    const msg = (r.error ?? "").toLowerCase();
    const st = r.status;
    const transient =
      msg.includes("abort") ||
      msg.includes("timeout") ||
      msg.includes("fetch failed") ||
      msg.includes("network") ||
      st === 408 ||
      st === 425 ||
      st === 429 ||
      st === 500 ||
      st === 502 ||
      st === 503 ||
      st === 504 ||
      st === 529;

    if (!transient) return last;

    if (attempt < retries) {
      const backoff = 500 * 2 ** attempt;
      await sleep(backoff);
    }
  }
  return last;
}

async function runPool(items, concurrency, worker) {
  let idx = 0;
  const results = [];
  const runners = Array.from({ length: concurrency }, async () => {
    while (idx < items.length) {
      const cur = idx++;
      results[cur] = await worker(items[cur], cur);
    }
  });
  await Promise.all(runners);
  return results;
}

async function main() {
  const { files, concurrency, timeoutMs, retries, strict5xx, probe, maxRedirects, probeMaxBytes } = parseArgs(
    process.argv.slice(2)
  );
  const repoRoot = process.cwd();

  const allUrls = [];
  for (const f of files) {
    try {
      const urls = await loadCatalogUrls(repoRoot, f);
      allUrls.push(...urls);
      console.log(`${DIM("scan")} ${f} ${DIM(`(${urls.length} url${urls.length === 1 ? "" : "s"})`)}`);
    } catch (e) {
      console.log(BAD(`[DEAD LINK] cannot read ${f}: ${e instanceof Error ? e.message : String(e)}`));
      process.exitCode = 1;
      return;
    }
  }

  const urls = uniq(allUrls)
    .map((u) => u.trim())
    .filter((u) => u.length > 0)
    .filter((u) => u.startsWith("http://") || u.startsWith("https://"));

  console.log("");
  console.log(
    `${DIM("checking")} ${urls.length} link${urls.length === 1 ? "" : "s"} ${DIM(
      `(concurrency=${concurrency}, timeoutMs=${timeoutMs}, retries=${retries}${strict5xx ? ", strict5xx" : ""}${
        probe ? `, probe, maxRedirects=${maxRedirects}, probeMaxBytes=${probeMaxBytes}` : ""
      })`
    )}`
  );
  console.log("");

  let deadCount = 0;
  let tempFailCount = 0;
  await runPool(
    urls,
    concurrency,
    async (url) => {
      if (probe) {
        const pr = await probeUrlWithRetries(url, timeoutMs, retries, maxRedirects, probeMaxBytes);
        if (!pr.ok) {
          const st = pr.status;
          if (!strict5xx && st != null && st >= 500 && st <= 599) {
            tempFailCount++;
            console.log(
              WARN(`[PROBE TEMPFAIL ${st}]`) + ` ${url} ${DIM(`${pr.error ?? "upstream error"} (not counted as dead)`)}`
            );
            return;
          }
          deadCount++;
          console.log(BAD(`[PROBE FAIL]`) + ` ${url} ${DIM(pr.error ?? "failed")}`);
          return;
        }

        const finalNote = pr.finalUrl && pr.finalUrl !== url ? ` ${DIM(`→ ${pr.finalUrl}`)}` : "";
        const meta = [
          pr.mode ? `${pr.mode}` : "",
          pr.status ? `HTTP ${pr.status}` : "",
          pr.acceptRanges ? `Accept-Ranges=${pr.acceptRanges}` : "",
          pr.contentLength ? `Content-Length=${pr.contentLength}` : "",
          pr.contentRange ? `Content-Range=${pr.contentRange}` : "",
          pr.bytesRead != null ? `read≈${pr.bytesRead}B` : "",
        ]
          .filter(Boolean)
          .join(" · ");

        console.log(OK(`[PROBE OK]`) + ` ${url}${finalNote}${meta ? ` ${DIM(`(${meta})`)}` : ""}`);
        return;
      }

      const r = await headCheckWithRetries(url, timeoutMs, retries);
      if (!r.ok) {
        deadCount++;
        console.log(BAD(`[DEAD LINK]`) + ` ${url} ${DIM(r.error ?? "request failed")}`);
        return;
      }
      if (r.status >= 200 && r.status <= 299) {
        console.log(OK(`[OK]`) + ` ${url}`);
        return;
      }
      if (r.status >= 300 && r.status <= 399) {
        const dest = r.location ? ` → ${r.location}` : "";
        console.log(WARN(`[REDIRECT ${r.status}]`) + ` ${url}${dest}`);
        return;
      }
      if (r.status >= 500 && r.status <= 599) {
        tempFailCount++;
        if (strict5xx) {
          deadCount++;
          console.log(BAD(`[DEAD LINK ${r.status}]`) + ` ${url} ${DIM("server error")}`);
        } else {
          console.log(WARN(`[TEMPFAIL ${r.status}]`) + ` ${url} ${DIM("server error (not counted as dead)")}`);
        }
        return;
      }
      deadCount++;
      console.log(BAD(`[DEAD LINK ${r.status}]`) + ` ${url}`);
    }
  );

  console.log("");
  if (deadCount > 0) {
    console.log(BAD(`${probe ? "Probe failures" : "Dead links"}: ${deadCount}`));
    process.exitCode = 1;
  } else {
    console.log(
      OK(probe ? "All probes OK." : "All links OK (or redirected).") +
        (tempFailCount > 0 ? ` ${WARN(`Temporary upstream failures: ${tempFailCount}`)}` : "")
    );
  }
}

await main();

