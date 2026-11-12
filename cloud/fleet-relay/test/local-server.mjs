// A local stand-in for the Worker, for testing stations against the real relay logic:
//   node cloud/fleet-relay/test/local-server.mjs 8787
import { createServer } from "node:http";
import { MemoryStore, Room, roomFromPath, json } from "../src/relay.js";

const rooms = new Map();
const port = Number(process.argv[2] || 8787);
createServer(async (req, res) => {
  const chunks = [];
  for await (const c of req) chunks.push(c);
  const url = new URL(req.url, `http://127.0.0.1:${port}`);
  const request = new Request(url, { method: req.method, headers: req.headers, body: chunks.length ? Buffer.concat(chunks) : undefined });
  const room = roomFromPath(url.pathname);
  let response;
  if (!room) response = json(404, { error: "not found" });
  else {
    if (!rooms.has(room)) rooms.set(room, new Room(new MemoryStore()));
    response = await rooms.get(room).handle(request);
  }
  res.writeHead(response.status, Object.fromEntries(response.headers));
  res.end(await response.text());
}).listen(port, "127.0.0.1", () => console.log(`fleet relay on http://127.0.0.1:${port}`));
