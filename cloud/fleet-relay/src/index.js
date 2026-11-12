// Cloudflare Worker entry: routes each fleet's room to its own Durable Object.
import { json, Room, roomFromPath } from "./relay.js";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/" || url.pathname === "/health") {
      return json(200, { ok: true, service: "athena-fleet-relay" });
    }
    const room = roomFromPath(url.pathname);
    if (!room) return json(404, { error: "not found" });
    const stub = env.ROOMS.get(env.ROOMS.idFromName(room));
    return stub.fetch(request);
  },
};

export class FleetRoom {
  constructor(state) {
    this.room = new Room(state.storage);
  }

  async fetch(request) {
    return this.room.handle(request);
  }
}
