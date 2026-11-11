// deetsmusic-protocol: the shapes that travel between DeetsMusic's host and a remote UI
// (docs/features/ACTIVITY.md §4, §5). The names follow DeetsMusic's bridge
// (docs/architecture/BRIDGE.md §3), so the bot passes them on with little change.
//
// This file knows DeetsMusic. It knows nothing of Discord, the relay or the DOM.

export const PROTOCOL_VERSION = 1;

/** One song, as DeetsMusic's bridge gives it. */
export interface Track {
  catalogId: string;
  title: string;
  artistName: string;
  albumName?: string;
  durationMs?: number;
  contentRating?: "explicit" | "clean";
  /** Apple's artwork template: `{w}x{h}` are replaced with a pixel size. */
  artworkTemplate?: string;
}

/** What plays now. The page runs the clock itself from `positionMs` and `at`. */
export interface NowPlaying {
  track: Track | null;
  playing: boolean;
  /** The position when the host read it. */
  positionMs: number;
  /** Epoch milliseconds when `positionMs` was true. */
  at: number;
}

/** A song that played. The bridge's history has no play time today, so `playedAt` is optional. */
export interface HistoryEntry {
  track: Track;
  playedAt?: number;
}

/** DeetsMusic › Settings › Rooms: what a guest may do. Any value but `everyone` is host only. */
export type GuestRule = "everyone" | "host";
export interface GuestRules {
  playPause: GuestRule;
  skip: GuestRule;
  add: GuestRule;
  changeQueue: GuestRule;
  seek: GuestRule;
}

/** Who is looking at this UI. */
export interface Viewer {
  name: string;
  host: boolean;
}

/** The whole state, sent on a change (never on a timer). */
export interface HostState {
  v: typeof PROTOCOL_VERSION;
  /** False while the host is not playing DeetsMusic into this call. */
  active: boolean;
  now: NowPlaying;
  upNext: Track[];
  /** Newest first. */
  history: HistoryEntry[];
  /** DeetsMusic's theme and skin ids (`data-theme`, `data-skin`). */
  theme: string;
  skin: string;
  rules: GuestRules;
  you: Viewer;
}

/** What a remote UI may ask for. Each maps to one bridge route (ACTIVITY.md §5). */
export type Action =
  | { kind: "play" }
  | { kind: "pause" }
  | { kind: "next" }
  | { kind: "previous" }
  | { kind: "seek"; positionMs: number }
  | { kind: "enqueue"; catalogId: string; mode: "next" | "later" }
  /** The song plays now; Up Next stays. `dropCopy`: the person's own add put a copy of this
   *  song in Up Next a moment ago (the toast's Play Now): the host takes the last copy out. */
  | { kind: "playNow"; catalogId: string; dropCopy?: boolean }
  /** `catalogId`: the song the person saw in that row. The host refuses when Up Next changed
   *  under them, so the wrong song is never removed or moved. */
  | { kind: "queueRemove"; index: number; catalogId?: string }
  | { kind: "queueMove"; index: number; to: number; catalogId?: string };

/** The rule that guards each action. `null`: host only, whatever the rules say. */
export const ACTION_RULE: Record<Action["kind"], keyof GuestRules | null> = {
  play: "playPause",
  pause: "playPause",
  next: "skip",
  previous: "skip",
  seek: "seek",
  enqueue: "add",
  // Also needs `add` (see `allowed`): it cuts the song off AND plays a song the guest chose.
  playNow: "skip",
  queueRemove: "changeQueue",
  queueMove: "changeQueue",
};

/** May this viewer do this kind of action? The host decides again on its side: this only
 *  draws the control the right way. */
export function allowed(state: Pick<HostState, "rules" | "you" | "active">, kind: Action["kind"]): boolean {
  if (!state.active) return false;
  if (state.you.host) return true;
  if (kind === "playNow") return state.rules.skip === "everyone" && state.rules.add === "everyone";
  const rule = ACTION_RULE[kind];
  return rule !== null && state.rules[rule] === "everyone";
}

export type ActionResult =
  | { ok: true; notices?: string[] }
  /** `reason` is the host's own line, shown to this viewer only. */
  | { ok: false; reason: string };

// ── over the relay (build step 4) ──────────────────────────────────────────────
// What rides in the relay's opaque `data` field (activity/relay/src/wire.ts). The bot's side
// is crates/logic/src/deetsmusic_remote.rs: change both together.

/** Page → host. `q` pairs the reply with its request. */
export type Request = { q: number; act: Action } | { q: number; search: string };

/** Host → page. `state` goes to everyone in the call; the rest to one person. */
export type Reply =
  /** The host's state, without `you` (each page knows its own). On a change only. */
  | { kind: "state"; state: Omit<HostState, "you"> }
  /** Whether this person is the host. Once, when the bot learns they are in the Activity. */
  | { kind: "you"; host: boolean }
  | { kind: "answer"; q: number; result: ActionResult }
  /** Search results; `error` is the host's line when the search was refused or failed. */
  | { kind: "found"; q: number; tracks: Track[]; error?: string | null };

/** The one seam a UI talks to: state in, actions out. The Activity's transport is the relay;
 *  the mock's is fake data. */
export interface Transport {
  /** Calls `cb` with the current state at once, then on every change. Returns the unsubscribe. */
  subscribe(cb: (state: HostState) => void): () => void;
  act(action: Action): Promise<ActionResult>;
  /** Songs from the host's DeetsMusic search. */
  search(term: string): Promise<Track[]>;
}

/** Fill Apple's artwork template for a square of `px` pixels. */
export function artworkURL(track: Track | null | undefined, px: number): string | null {
  const t = track?.artworkTemplate;
  if (!t) return null;
  const s = String(px);
  return t.replace("{w}", s).replace("{h}", s).replace("{f}", "jpg");
}

/** The position now, from the last state and the clock. */
export function positionNow(now: NowPlaying, clock = Date.now()): number {
  const d = now.track?.durationMs ?? 0;
  const p = now.playing ? now.positionMs + (clock - now.at) : now.positionMs;
  return Math.max(0, d ? Math.min(p, d) : p);
}
