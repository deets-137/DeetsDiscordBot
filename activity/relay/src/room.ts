// The room: one Durable Object that holds the bot's socket and every page's socket, and
// passes messages between them (docs/features/ACTIVITY.md §5).
//
// It sleeps between messages (the WebSocket Hibernation API), so an idle room costs nothing.
// Because it sleeps, it keeps nothing in memory: what it knows about a socket is in that
// socket's attachment, it finds sockets again by tag, and the last state the bot sent is in
// the room's storage.
//
// A page's socket goes through these states:
//   new       connected; no ticket yet
//   waiting   signed in, but no bot is connected to check the call. The person sees "The host
//             is not playing here". When a bot connects, the check runs.
//   checking  the bot was asked whether this person is in this Activity instance
//   in        the bot said yes, and named the instance's voice channel
//
// The relay decides nothing about music. It decides two things: who a person is (the ticket),
// and whether the bot is in that person's call. Only then does a page's `data` reach the bot,
// and only then does the bot's state reach the page.

import { DurableObject, env } from "cloudflare:workers";
import { readTicket } from "./ticket.ts";
import { CLOSE, WIRE, type FromGuest, type FromHost, type ToGuest, type ToHost } from "./wire.ts";

const MAX_GUESTS = 200;
const MAX_GUEST_FRAME = 4096;
const MAX_HOST_FRAME = 1 << 20;
/** A page may send this many `data` frames in each window; more are dropped (a stuck button,
 *  a script). A person pressing buttons stays far below it. */
const DATA_PER_WINDOW = 30;
const WINDOW_MS = 10_000;
/** The storage key of the last state the bot sent (raw JSON). */
const STATE = "state";

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
  /** The rate window: its start (epoch ms) and the `data` frames in it. */
  win?: number;
  n?: number;
}
type Info = HostInfo | GuestInfo;

const info = (ws: WebSocket): Info | null => ws.deserializeAttachment() as Info | null;
const sendRaw = (ws: WebSocket, raw: string): void => {
  try {
    ws.send(raw);
  } catch {
    /* the socket closed under us: its close handler tidies up */
  }
};
const send = (ws: WebSocket, msg: ToGuest | ToHost): void => sendRaw(ws, JSON.stringify(msg));

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

  /** The last state, as the frame a page gets (`{"t":"data","data":…}`), or null. */
  private async lastState(): Promise<string | null> {
    const raw = await this.ctx.storage.get<string>(STATE);
    return raw ? `{"t":"data","data":${raw}}` : null;
  }

  /** A person's page now hears that the bot is in its call: give it the last state too. */
  private async arrived(ws: WebSocket): Promise<void> {
    const frame = await this.lastState();
    if (frame) sendRaw(ws, frame);
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
      // A state from an earlier connection may be old: the new one sends its own.
      await this.ctx.storage.delete(STATE);
      // People who signed in while no bot was here, and people already in: check them now.
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
    if (who.kind === "host") await this.fromHost(ws, who, msg as FromHost);
    else await this.fromGuest(ws, who, msg as FromGuest);
  }

  // ── a page ───────────────────────────────────────────────────────────────────

  private async fromGuest(ws: WebSocket, gi: GuestInfo, msg: FromGuest): Promise<void> {
    if (msg?.t === "data" && gi.state !== "new") return this.guestData(ws, gi, msg.data);
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

  /** A page's request reaches the bot only while the bot is in that person's call. */
  private guestData(ws: WebSocket, gi: GuestInfo, data: unknown): void {
    const host = this.host();
    if (gi.state !== "in" || !gi.here || !host || data === undefined) return; // the page knows: it was told "not here"
    const now = Date.now();
    if (!gi.win || now - gi.win > WINDOW_MS) {
      gi.win = now;
      gi.n = 0;
    }
    gi.n = (gi.n ?? 0) + 1;
    ws.serializeAttachment(gi);
    if (gi.n > DATA_PER_WINDOW) {
      if (gi.n === DATA_PER_WINDOW + 1) console.warn(`[room] a page sent more than ${DATA_PER_WINDOW} requests in ${WINDOW_MS / 1000} s; dropping the rest of the window`);
      return;
    }
    send(host, { t: "data", rid: gi.id, data });
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

  private async fromHost(ws: WebSocket, hi: HostInfo, msg: FromHost): Promise<void> {
    if (msg?.t === "channels") {
      if (!Array.isArray(msg.channels) || msg.channels.some((c) => typeof c !== "string")) return;
      hi.channels = msg.channels.slice(0, 64);
      ws.serializeAttachment(hi);
      await this.tellGuests(hi.channels);
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
      send(ws, { t: "joined", rid: gi.id, user: gi.user.id, name: gi.user.name, channel: msg.channel });
      if (first) send(guest, { t: "welcome", user: gi.user, host: here });
      else if (changed) send(guest, { t: "host", here });
      if (here) await this.arrived(guest);
      return;
    }
    if (msg?.t === "state") {
      if (msg.data === null || msg.data === undefined) {
        await this.ctx.storage.delete(STATE);
        return;
      }
      const raw = JSON.stringify(msg.data);
      await this.ctx.storage.put(STATE, raw);
      const frame = `{"t":"data","data":${raw}}`;
      for (const g of this.guests()) {
        const gi = info(g);
        if (gi?.kind === "guest" && gi.state === "in" && gi.here) sendRaw(g, frame);
      }
      return;
    }
    if (msg?.t === "to") {
      const guest = typeof msg.rid === "string" ? this.ctx.getWebSockets(`g:${msg.rid}`)[0] : undefined;
      const gi = guest ? info(guest) : null;
      if (guest && gi?.kind === "guest" && gi.state === "in") send(guest, { t: "data", data: msg.data });
    }
  }

  /** The bot's channels changed (or the bot went away): tell each page whose answer changed. */
  private async tellGuests(channels: string[]): Promise<void> {
    for (const g of this.guests()) {
      const gi = info(g);
      if (gi?.kind !== "guest" || gi.state !== "in" || !gi.channel) continue;
      const here = channels.includes(gi.channel);
      if (gi.here === here) continue;
      gi.here = here;
      g.serializeAttachment(gi);
      send(g, { t: "host", here });
      if (here) await this.arrived(g);
    }
  }

  override async webSocketClose(ws: WebSocket): Promise<void> {
    const who = info(ws);
    if (who?.kind === "guest") {
      // The bot stops looking after a person who closed the Activity.
      const host = this.host();
      if (host && who.state === "in") send(host, { t: "left", rid: who.id });
      return;
    }
    if (who?.kind !== "host") return;
    // A replaced host's close arrives after the new host connected: then nothing changed.
    if (this.ctx.getWebSockets("host").some((h) => h !== ws && h.readyState === WebSocket.OPEN)) return;
    console.log("[room] the host disconnected");
    await this.ctx.storage.delete(STATE);
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
