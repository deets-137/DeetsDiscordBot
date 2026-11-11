// The room: one Durable Object that holds the bot's socket and every page's socket, and
// passes messages between them (docs/features/ACTIVITY.md §5).
//
// It sleeps between messages (the WebSocket Hibernation API), so an idle room costs nothing.
// Because it sleeps, it keeps nothing in memory: what it knows about a socket is in that
// socket's attachment, and it finds sockets again by tag.
//
// A page's socket goes through these states:
//   new       connected; no ticket yet
//   waiting   signed in, but no bot is connected to check the call. The person sees "The host
//             is not playing here". When a bot connects, the check runs.
//   checking  the bot was asked whether this person is in this Activity instance
//   in        the bot said yes, and named the instance's voice channel
//
// The relay decides nothing about music. It decides two things: who a person is (the ticket),
// and whether the bot is in that person's call.

import { DurableObject, env } from "cloudflare:workers";
import { readTicket } from "./ticket.ts";
import { CLOSE, WIRE, type FromGuest, type FromHost, type ToGuest, type ToHost } from "./wire.ts";

const MAX_GUESTS = 200;
const MAX_GUEST_FRAME = 4096;
const MAX_HOST_FRAME = 1 << 20;

interface HostInfo {
  kind: "host";
  channels: string[];
}
interface GuestInfo {
  kind: "guest";
  /** This socket's id: its tag, and the `rid` of its check. */
  id: string;
  state: "new" | "waiting" | "checking" | "in";
  user?: { id: string; name: string };
  instance?: string;
  /** The instance's voice channel, once the bot has checked it. */
  channel?: string;
  /** What the page was last told about the bot being in its call. */
  here?: boolean;
}
type Info = HostInfo | GuestInfo;

const info = (ws: WebSocket): Info | null => ws.deserializeAttachment() as Info | null;
const send = (ws: WebSocket, msg: ToGuest | ToHost): void => {
  try {
    ws.send(JSON.stringify(msg));
  } catch {
    /* the socket closed under us: its close handler tidies up */
  }
};

export class Room extends DurableObject {
  constructor(ctx: DurableObjectState, e: Cloudflare.Env) {
    super(ctx, e);
    // A keep-alive from either side is answered without waking the room.
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  /** The bot's open socket. A replaced one stays in the list, closing, for a moment: not that one. */
  private host(): WebSocket | null {
    return this.ctx.getWebSockets("host").find((h) => h.readyState === WebSocket.OPEN) ?? null;
  }
  private guests(): WebSocket[] {
    return this.ctx.getWebSockets("guest");
  }

  override async fetch(req: Request): Promise<Response> {
    const path = new URL(req.url).pathname;
    const pair = new WebSocketPair();
    const [client, server] = [pair[0], pair[1]];

    if (path === "/host") {
      // One bot. A new connection replaces the old one (a restart, a network drop).
      for (const old of this.ctx.getWebSockets("host")) old.close(CLOSE.replaced, "Another host connected.");
      this.ctx.acceptWebSocket(server, ["host"]);
      server.serializeAttachment({ kind: "host", channels: [] } satisfies HostInfo);
      console.log("[room] the host connected");
      // People who signed in while no bot was here: check them now.
      for (const g of this.guests()) {
        const gi = info(g);
        if (gi?.kind === "guest" && gi.user && gi.instance) this.askHost(g, gi);
      }
      return new Response(null, { status: 101, webSocket: client });
    }

    if (path === "/guest") {
      if (this.guests().length >= MAX_GUESTS) return new Response("The relay is full.", { status: 503 });
      const id = crypto.randomUUID();
      this.ctx.acceptWebSocket(server, ["guest", `g:${id}`]);
      server.serializeAttachment({ kind: "guest", id, state: "new" } satisfies GuestInfo);
      return new Response(null, { status: 101, webSocket: client });
    }

    return new Response("No such route.", { status: 404 });
  }

  override async webSocketMessage(ws: WebSocket, raw: string | ArrayBuffer): Promise<void> {
    const who = info(ws);
    if (!who) return ws.close(CLOSE.badFrame, "Unknown socket.");
    const limit = who.kind === "host" ? MAX_HOST_FRAME : MAX_GUEST_FRAME;
    if (typeof raw !== "string") return ws.close(CLOSE.badFrame, "Text frames only.");
    if (raw.length > limit) return ws.close(CLOSE.tooLarge, "The frame is too large.");
    let msg: unknown;
    try {
      msg = JSON.parse(raw);
    } catch {
      return ws.close(CLOSE.badFrame, "The frame is not JSON.");
    }
    if (who.kind === "host") this.fromHost(ws, who, msg as FromHost);
    else await this.fromGuest(ws, who, msg as FromGuest);
  }

  // ── a page ───────────────────────────────────────────────────────────────────

  private async fromGuest(ws: WebSocket, gi: GuestInfo, msg: FromGuest): Promise<void> {
    if (msg?.t !== "hello" || gi.state !== "new") return ws.close(CLOSE.badFrame, "Unexpected frame.");
    if (msg.v !== WIRE) return this.refuse(ws, "This page is out of date. Close the Activity and open it again.");
    if (typeof msg.ticket !== "string" || typeof msg.instance !== "string" || !/^[\w-]{1,128}$/.test(msg.instance)) {
      return ws.close(CLOSE.badFrame, "Unexpected frame.");
    }
    const user = await readTicket(env.HOST_SECRET, msg.ticket);
    if (!user) return this.refuse(ws, "The sign-in ran out. Close the Activity and open it again.");
    gi.user = user;
    gi.instance = msg.instance;
    if (this.host()) return this.askHost(ws, gi);
    // No bot: one line, and nothing else (ACTIVITY.md §6).
    gi.state = "waiting";
    gi.here = false;
    ws.serializeAttachment(gi);
    send(ws, { t: "welcome", user, host: false });
  }

  /** Ask the bot whether this person is in this Activity instance. */
  private askHost(ws: WebSocket, gi: GuestInfo): void {
    const host = this.host();
    if (!host || !gi.user || !gi.instance) return;
    gi.state = "checking";
    ws.serializeAttachment(gi);
    send(host, { t: "check", rid: gi.id, user: gi.user.id, instance: gi.instance });
  }

  private refuse(ws: WebSocket, reason: string): void {
    send(ws, { t: "refused", reason });
    ws.close(CLOSE.refused, "Refused.");
  }

  // ── the bot ──────────────────────────────────────────────────────────────────

  private fromHost(ws: WebSocket, hi: HostInfo, msg: FromHost): void {
    if (msg?.t === "channels") {
      if (!Array.isArray(msg.channels) || msg.channels.some((c) => typeof c !== "string")) return;
      hi.channels = msg.channels.slice(0, 64);
      ws.serializeAttachment(hi);
      this.tellGuests(hi.channels);
      return;
    }
    if (msg?.t === "checked") {
      const guest = typeof msg.rid === "string" ? this.ctx.getWebSockets(`g:${msg.rid}`)[0] : undefined;
      const gi = guest ? info(guest) : null;
      if (!guest || gi?.kind !== "guest" || gi.state !== "checking" || !gi.user) return; // gone, or not asked
      if (!msg.ok || typeof msg.channel !== "string") {
        return this.refuse(guest, typeof msg.reason === "string" ? msg.reason.slice(0, 200) : "Discord did not confirm that you are in this Activity.");
      }
      const first = gi.here === undefined;
      gi.state = "in";
      gi.channel = msg.channel;
      const here = hi.channels.includes(msg.channel);
      const changed = gi.here !== here;
      gi.here = here;
      guest.serializeAttachment(gi);
      if (first) send(guest, { t: "welcome", user: gi.user, host: here });
      else if (changed) send(guest, { t: "host", here });
    }
  }

  /** The bot's channels changed (or the bot went away): tell each page whose answer changed. */
  private tellGuests(channels: string[]): void {
    for (const g of this.guests()) {
      const gi = info(g);
      if (gi?.kind !== "guest" || gi.state !== "in" || !gi.channel) continue;
      const here = channels.includes(gi.channel);
      if (gi.here === here) continue;
      gi.here = here;
      g.serializeAttachment(gi);
      send(g, { t: "host", here });
    }
  }

  override async webSocketClose(ws: WebSocket): Promise<void> {
    const who = info(ws);
    if (who?.kind !== "host") return;
    // A replaced host's close arrives after the new host connected: then nothing changed.
    if (this.ctx.getWebSockets("host").some((h) => h !== ws && h.readyState === WebSocket.OPEN)) return;
    console.log("[room] the host disconnected");
    for (const g of this.guests()) {
      const gi = info(g);
      if (gi?.kind !== "guest" || gi.state === "new") continue;
      // The next bot checks each person again.
      const was = gi.here;
      gi.state = "waiting";
      gi.here = false;
      g.serializeAttachment(gi);
      if (was) send(g, { t: "host", here: false });
    }
  }

  override async webSocketError(ws: WebSocket): Promise<void> {
    await this.webSocketClose(ws);
  }
}
