// Fleet relay logic, kept free of Cloudflare APIs so it can be tested with plain Node.
//
// The relay is a sealed mailbox per fleet ("room"). Stations post messages they have already
// encrypted with their group key, and read back messages after a sequence number. The relay never
// sees readable content and holds no keys. Its only secret is a hash of each room's password.

export const LIMITS = {
  maxBodyChars: 70_000, // a 48 KB sealed message is about 64 K base64 characters
  maxMessagesPerRoom: 2_000,
  maxAgeMs: 14 * 24 * 3600 * 1000,
  maxPostsPerMinute: 60,
  maxReturned: 200,
};

const ROOM_RE = /^[0-9a-f]{32}$/;
const AUTH_RE = /^[0-9a-f]{64}$/;

export function json(status, body) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

export async function sha256Hex(text) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function sameString(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/** `/v1/rooms/<room>/messages` -> room id, or null. */
export function roomFromPath(pathname) {
  const m = /^\/v1\/rooms\/([^/]+)\/messages$/.exec(pathname);
  return m && ROOM_RE.test(m[1]) ? m[1] : null;
}

/**
 * One room. `store` is a small key-value interface:
 *   get(key) -> value | undefined, put(key, value), list({ prefix }) -> Map, delete(keys[])
 * which both the Durable Object storage and the in-memory test store provide.
 */
export class Room {
  constructor(store, now = () => Date.now()) {
    this.store = store;
    this.now = now;
  }

  /** The first station to use a room sets its password; after that it must match. */
  async authorize(request) {
    const header = request.headers.get("authorization") || "";
    const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";
    if (!AUTH_RE.test(token)) return false;
    const hash = await sha256Hex(token);
    const stored = await this.store.get("auth");
    if (stored === undefined) {
      await this.store.put("auth", hash);
      return true;
    }
    return sameString(stored, hash);
  }

  async handle(request) {
    if (!(await this.authorize(request))) return json(401, { error: "unauthorized" });
    if (request.method === "POST") return this.post(request);
    if (request.method === "GET") return this.list(new URL(request.url));
    return json(405, { error: "method not allowed" });
  }

  async post(request) {
    let body;
    try {
      body = (await request.json()).body;
    } catch {
      return json(400, { error: "expected JSON with a body field" });
    }
    if (typeof body !== "string" || body.length === 0 || body.length > LIMITS.maxBodyChars || !/^[A-Za-z0-9+/=]+$/.test(body)) {
      return json(400, { error: "body must be a sealed message" });
    }
    const now = this.now();
    const minute = Math.floor(now / 60_000);
    const rate = (await this.store.get("rate")) || { minute, count: 0 };
    const current = rate.minute === minute ? rate : { minute, count: 0 };
    if (current.count >= LIMITS.maxPostsPerMinute) return json(429, { error: "too many messages, slow down" });
    current.count += 1;
    await this.store.put("rate", current);

    const seq = ((await this.store.get("seq")) || 0) + 1;
    await this.store.put("seq", seq);
    await this.store.put(`m:${String(seq).padStart(12, "0")}`, { seq, at: now, body });
    await this.prune(now);
    return json(200, { seq });
  }

  async list(url) {
    const after = Math.max(0, Number.parseInt(url.searchParams.get("after") || "0", 10) || 0);
    const all = await this.store.list({ prefix: "m:" });
    const messages = [];
    for (const value of all.values()) {
      if (value.seq > after) messages.push({ seq: value.seq, body: value.body });
      if (messages.length >= LIMITS.maxReturned) break;
    }
    return json(200, { messages });
  }

  /** Drop messages older than two weeks and keep at most the newest 2,000. */
  async prune(now) {
    const all = await this.store.list({ prefix: "m:" });
    const keys = [...all.keys()];
    const drop = [];
    const excess = keys.length - LIMITS.maxMessagesPerRoom;
    keys.forEach((key, i) => {
      if (i < excess || now - all.get(key).at > LIMITS.maxAgeMs) drop.push(key);
    });
    if (drop.length > 0) await this.store.delete(drop);
  }
}

/** In-memory store with the same interface, for tests. Keys list in sorted order like Cloudflare's. */
export class MemoryStore {
  constructor() {
    this.map = new Map();
  }
  async get(key) {
    return this.map.get(key);
  }
  async put(key, value) {
    this.map.set(key, structuredClone(value));
  }
  async list({ prefix }) {
    const keys = [...this.map.keys()].filter((k) => k.startsWith(prefix)).sort();
    return new Map(keys.map((k) => [k, this.map.get(k)]));
  }
  async delete(keys) {
    for (const k of keys) this.map.delete(k);
  }
}
