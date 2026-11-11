// The Activity's Transport (deetsmusic-protocol): state in from the bot, actions out to it,
// over the page's one WebSocket to the relay. It knows the relay's frames only as `data`.
//
// - The state the bot sends has no `you`; this page adds its own (the name from the sign-in,
//   the host flag the bot sends once).
// - `active` is the bot's word AND the relay's "the bot is in this call".
// - The position's time stamp is this device's clock at arrival, so a device whose clock is
//   off still runs the bar right.
// - Apple's covers come through Discord's proxy (`/.proxy/mz/is1/…`): the frame may load
//   nothing from another site (the URL mappings in the Developer Portal, ACTIVITY.md §12.3).

import {
  PROTOCOL_VERSION,
  type Action,
  type ActionResult,
  type HostState,
  type Reply,
  type Request,
  type Track,
  type Transport,
} from "../deetsmusic-protocol";

const NOT_HERE = "The host is not playing here";
const NO_ANSWER = "The host did not answer. Try again.";
const ANSWER_WITHIN_MS = 15_000;

/** `https://is3-ssl.mzstatic.com/…` → `/.proxy/mz/is3/…` (one URL mapping per host). */
const APPLE_ART = /^https:\/\/(is[1-5])-ssl\.mzstatic\.com\//;
function viaProxy(t: Track): Track {
  if (!t.artworkTemplate || !APPLE_ART.test(t.artworkTemplate)) return t;
  return { ...t, artworkTemplate: t.artworkTemplate.replace(APPLE_ART, (_, host: string) => `/.proxy/mz/${host}/`) };
}

export interface RelayTransport extends Transport {
  /** A `data` frame from the relay. */
  receive(data: unknown): void;
  /** The relay says whether the bot is in this person's call. */
  setHere(here: boolean): void;
  /** The latest state as the bot sent it (Apple's own cover addresses), for Rich Presence. */
  raw(): Omit<HostState, "you"> | null;
}

export function relayTransport(send: (req: Request) => void, me: { name: string }): RelayTransport {
  const subs = new Set<(s: HostState) => void>();
  let here = false;
  let host = false;
  let base: Omit<HostState, "you"> | null = null;
  let current: HostState;
  let q = 0;
  const waiting = new Map<number, { done: (r: Reply) => void; fail: (why: string) => void; timer: number }>();

  const empty = (): Omit<HostState, "you"> => ({
    v: PROTOCOL_VERSION,
    active: false,
    now: { track: null, playing: false, positionMs: 0, at: Date.now() },
    upNext: [],
    history: [],
    theme: "lilac",
    skin: "press",
    rules: { playPause: "host", skip: "host", add: "host", changeQueue: "host", seek: "host" },
  });

  function compose(): void {
    const b = base ?? empty();
    current = {
      ...b,
      active: b.active && here,
      now: { ...b.now, track: b.now.track ? viaProxy(b.now.track) : null },
      upNext: b.upNext.map(viaProxy),
      history: b.history.map((h) => ({ ...h, track: viaProxy(h.track) })),
      you: { name: me.name, host },
    };
    for (const cb of subs) cb(current);
  }
  compose();

  function ask(body: { act: Action } | { search: string }): Promise<Reply> {
    return new Promise((resolve, reject) => {
      const id = ++q;
      const timer = window.setTimeout(() => {
        waiting.delete(id);
        reject(new Error(NO_ANSWER));
      }, ANSWER_WITHIN_MS);
      waiting.set(id, {
        done: (r) => {
          clearTimeout(timer);
          resolve(r);
        },
        fail: (why) => {
          clearTimeout(timer);
          reject(new Error(why));
        },
        timer,
      });
      send({ q: id, ...body } as Request);
    });
  }

  return {
    subscribe(cb) {
      subs.add(cb);
      cb(current);
      return () => subs.delete(cb);
    },

    async act(action: Action): Promise<ActionResult> {
      if (!here) return { ok: false, reason: NOT_HERE };
      try {
        const r = await ask({ act: action });
        return r.kind === "answer" ? r.result : { ok: false, reason: NO_ANSWER };
      } catch (e) {
        return { ok: false, reason: e instanceof Error ? e.message : NO_ANSWER };
      }
    },

    async search(term: string): Promise<Track[]> {
      if (!here) throw new Error(NOT_HERE);
      const r = await ask({ search: term });
      if (r.kind !== "found") throw new Error(NO_ANSWER);
      if (r.error) throw new Error(r.error);
      return r.tracks.map(viaProxy);
    },

    receive(data: unknown): void {
      const r = data as Reply | null;
      if (!r || typeof r !== "object") return;
      if (r.kind === "state" && r.state && r.state.v === PROTOCOL_VERSION) {
        // This device's clock: the bot's `at` is the PC's, which may differ.
        base = { ...r.state, now: { ...r.state.now, at: Date.now() } };
        compose();
      } else if (r.kind === "you") {
        host = !!r.host;
        compose();
      } else if ((r.kind === "answer" || r.kind === "found") && typeof r.q === "number") {
        const w = waiting.get(r.q);
        if (w) {
          waiting.delete(r.q);
          w.done(r);
        }
      }
    },

    setHere(h: boolean): void {
      if (here === h) return;
      here = h;
      if (!h) {
        // The bot left: what it had is gone, and nothing it was asked will be answered.
        base = null;
        for (const [id, w] of waiting) {
          waiting.delete(id);
          w.fail(NOT_HERE);
        }
      }
      compose();
    },

    raw: () => base,
  };
}
