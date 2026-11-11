// The relay's messages. One JSON object per WebSocket text frame, tagged by `t`.
//
// The relay knows Discord and the call. It knows nothing of DeetsMusic: what the bot and a
// page say to each other about music will ride inside an opaque `data` field (build step 4).
// Discord ids travel as strings: a snowflake does not fit a JavaScript number.
//
// The page (activity/discord-shell) imports this file for its types. The bot's side is
// crates/relay-client, which has the same shapes in Rust: change both together, and bump WIRE.

export const WIRE = 1;

// ── a page ⇄ the relay ─────────────────────────────────────────────────────────

/** The page's first frame: the ticket from `POST /api/token`, and Discord's instance id. */
export interface GuestHello {
  t: "hello";
  v: number;
  ticket: string;
  instance: string;
}
export type FromGuest = GuestHello;

export type ToGuest =
  /** The person is signed in. `host`: the bot is in this Activity's voice channel now. */
  | { t: "welcome"; user: { id: string; name: string }; host: boolean }
  /** The bot came into this call, or left it (or its connection to the relay dropped). */
  | { t: "host"; here: boolean }
  /** The relay closes the socket after this. `reason` is a line for the person. */
  | { t: "refused"; reason: string };

// ── the bot ⇄ the relay ────────────────────────────────────────────────────────

export type ToHost =
  /** Is this person in this Activity instance, and in which channel? The bot asks Discord
   *  with its own token, which never leaves the PC (ACTIVITY.md §6). */
  { t: "check"; rid: string; user: string; instance: string };

export type FromHost =
  /** The voice channels the bot is in now. Sent on connect and on every change. */
  | { t: "channels"; channels: string[] }
  /** The answer to a `check`. `ok` with the instance's `channel`, or not `ok` with a reason. */
  | { t: "checked"; rid: string; ok: boolean; channel?: string; reason?: string };

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
