// The relay's messages. One JSON object per WebSocket text frame, tagged by `t`.
//
// The relay knows Discord and the call. It knows nothing of DeetsMusic: what the bot and a
// page say to each other about music rides inside an opaque `data` field
// (deetsmusic-protocol `Request` and `Reply`). Discord ids travel as strings: a snowflake does
// not fit a JavaScript number.
//
// The page (activity/discord-shell) imports this file for its types. The bot's side is
// crates/relay-client, which has the same shapes in Rust: change both together, and bump WIRE.
//
// WIRE 2 (2026-11-11, build step 4): `data` both ways, `state`, `to`, `joined`, `left`.

export const WIRE = 2;

// ── a page ⇄ the relay ─────────────────────────────────────────────────────────

/** The page's first frame: the ticket from `POST /api/token`, and Discord's instance id. */
export interface GuestHello {
  t: "hello";
  v: number;
  ticket: string;
  instance: string;
}
/** A request for the bot. Passed on only while the bot is in this person's call. */
export interface GuestData {
  t: "data";
  data: unknown;
}
export type FromGuest = GuestHello | GuestData;

export type ToGuest =
  /** The person is signed in. `host`: the bot is in this Activity's voice channel now. */
  | { t: "welcome"; user: { id: string; name: string }; host: boolean }
  /** The bot came into this call, or left it (or its connection to the relay dropped). */
  | { t: "host"; here: boolean }
  /** From the bot: the state (to everyone in its call), or a reply to this person. */
  | { t: "data"; data: unknown }
  /** The relay closes the socket after this. `reason` is a line for the person. */
  | { t: "refused"; reason: string };

// ── the bot ⇄ the relay ────────────────────────────────────────────────────────

export type ToHost =
  /** Is this person in this Activity instance, and in which channel? The bot asks Discord
   *  with its own token, which never leaves the PC (ACTIVITY.md §6). */
  | { t: "check"; rid: string; user: string; instance: string }
  /** A checked person is in the Activity in `channel` (sent again after a bot reconnect). */
  | { t: "joined"; rid: string; user: string; name: string; channel: string }
  /** That person closed the Activity. */
  | { t: "left"; rid: string }
  /** A request from that person's page. */
  | { t: "data"; rid: string; data: unknown };

export type FromHost =
  /** The voice channels the bot is in now. Sent on connect and on every change. */
  | { t: "channels"; channels: string[] }
  /** The answer to a `check`. `ok` with the instance's `channel`, or not `ok` with a reason. */
  | { t: "checked"; rid: string; ok: boolean; channel?: string; reason?: string }
  /** The state, for every person whose call the bot is in. The relay keeps the last one for a
   *  person who comes in later; `null` forgets it. */
  | { t: "state"; data: unknown }
  /** Something for one person only (a reply, who they are). */
  | { t: "to"; rid: string; data: unknown };

// ── what the HTTP routes return ────────────────────────────────────────────────

export interface TokenReply {
  /** Discord's token for this person, for the SDK's `authenticate`. */
  access_token: string;
  user: { id: string; name: string };
  /** Proves to `/api/ws` who this is, for five minutes. */
  ticket: string;
}

/** WebSocket close codes the relay uses. */
export const CLOSE = { refused: 4003, replaced: 4000, tooLarge: 1009, badFrame: 4002 } as const;
