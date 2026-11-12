// Run with: node --test cloud/fleet-relay/test/relay.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { LIMITS, MemoryStore, Room, roomFromPath } from "../src/relay.js";

const ROOM = "0123456789abcdef0123456789abcdef";
const AUTH = "a".repeat(64);
const OTHER = "b".repeat(64);

function req(method, { auth = AUTH, body, after } = {}) {
  const url = `https://relay.test/v1/rooms/${ROOM}/messages${after !== undefined ? `?after=${after}` : ""}`;
  return new Request(url, {
    method,
    headers: { authorization: `Bearer ${auth}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify({ body }),
  });
}

test("only well-formed room paths are routed", () => {
  assert.equal(roomFromPath(`/v1/rooms/${ROOM}/messages`), ROOM);
  assert.equal(roomFromPath("/v1/rooms/../messages"), null);
  assert.equal(roomFromPath("/v1/rooms/ABC/messages"), null);
});

test("the first password claims the room and others are refused", async () => {
  const room = new Room(new MemoryStore());
  assert.equal((await room.handle(req("POST", { body: "c2VhbGVk" }))).status, 200);
  assert.equal((await room.handle(req("GET", { auth: OTHER }))).status, 401);
  assert.equal((await room.handle(req("GET", { auth: "short" }))).status, 401);
  assert.equal((await room.handle(req("GET"))).status, 200);
});

test("messages come back in order after a sequence number", async () => {
  const room = new Room(new MemoryStore());
  for (const b of ["b25l", "dHdv", "dGhyZWU="]) await room.handle(req("POST", { body: b }));
  const all = await (await room.handle(req("GET", { after: 0 }))).json();
  assert.deepEqual(all.messages.map((m) => m.seq), [1, 2, 3]);
  const later = await (await room.handle(req("GET", { after: 2 }))).json();
  assert.deepEqual(later.messages, [{ seq: 3, body: "dGhyZWU=" }]);
});

test("anything that is not a sealed message is refused", async () => {
  const room = new Room(new MemoryStore());
  assert.equal((await room.handle(req("POST", { body: "plain text <script>" }))).status, 400);
  assert.equal((await room.handle(req("POST", { body: "" }))).status, 400);
  assert.equal((await room.handle(req("POST", { body: "A".repeat(LIMITS.maxBodyChars + 1) }))).status, 400);
});

test("posting is rate limited per minute", async () => {
  let now = 1_000_000;
  const room = new Room(new MemoryStore(), () => now);
  for (let i = 0; i < LIMITS.maxPostsPerMinute; i++) {
    assert.equal((await room.handle(req("POST", { body: "eA==" }))).status, 200);
  }
  assert.equal((await room.handle(req("POST", { body: "eA==" }))).status, 429);
  now += 60_000;
  assert.equal((await room.handle(req("POST", { body: "eA==" }))).status, 200);
});

test("old messages are dropped", async () => {
  let now = 0;
  const room = new Room(new MemoryStore(), () => now);
  await room.handle(req("POST", { body: "b2xk" }));
  now += LIMITS.maxAgeMs + 1;
  await room.handle(req("POST", { body: "bmV3" }));
  const list = await (await room.handle(req("GET"))).json();
  assert.deepEqual(list.messages.map((m) => m.body), ["bmV3"]);
});
