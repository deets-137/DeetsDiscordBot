// A fake host for the mock page: a small player in memory that obeys the same rules the bot
// will (deetsmusic-protocol `allowed`). No Discord, no relay, no DeetsMusic.

import {
  PROTOCOL_VERSION,
  allowed,
  positionNow,
  type Action,
  type ActionResult,
  type GuestRules,
  type HistoryEntry,
  type HostState,
  type Track,
  type Transport,
} from "../deetsmusic-protocol";
import { SONGS } from "./data";

const RULE_NAME: Record<keyof GuestRules, string> = {
  playPause: "Guests may play and pause",
  skip: "Guests may skip",
  add: "Guests may add songs",
  changeQueue: "Guests may change Up Next",
  seek: "Guests may seek",
};

export interface MockHost extends Transport {
  /** The mock page's own switches. A real transport has none of these. */
  set(patch: Partial<Pick<HostState, "theme" | "skin" | "active">> & { host?: boolean; rules?: Partial<GuestRules> }): void;
  state(): HostState;
}

export function mockHost(): MockHost {
  const subs = new Set<(s: HostState) => void>();
  let upNext: Track[] = SONGS.slice(1, 9);
  let history: HistoryEntry[] = SONGS.slice(9).map((track, i) => ({ track, playedAt: Date.now() - (i + 1) * 240_000 }));
  let s: HostState = {
    v: PROTOCOL_VERSION,
    active: true,
    now: { track: SONGS[0], playing: true, positionMs: 42_000, at: Date.now() },
    upNext,
    history,
    theme: "lilac",
    skin: "press",
    rules: { playPause: "everyone", skip: "everyone", add: "everyone", changeQueue: "everyone", seek: "everyone" },
    you: { name: "You", host: false },
  };
  let endTimer = 0;

  const emit = (): void => {
    s = { ...s, upNext: [...upNext], history: [...history] };
    for (const cb of subs) cb(s);
    armEnd();
  };

  /** The song ends by itself, as the real player's would. */
  function armEnd(): void {
    clearTimeout(endTimer);
    const t = s.now.track;
    if (!t || !s.now.playing || !t.durationMs) return;
    endTimer = window.setTimeout(next, Math.max(0, t.durationMs - positionNow(s.now)) + 50);
  }

  function next(): void {
    if (s.now.track) history = [{ track: s.now.track, playedAt: Date.now() }, ...history];
    const track = upNext.shift() ?? null;
    s = { ...s, now: { track, playing: !!track, positionMs: 0, at: Date.now() } };
    emit();
  }

  function previous(): void {
    const last = history.shift();
    if (!last) {
      s = { ...s, now: { ...s.now, positionMs: 0, at: Date.now() } };
    } else {
      if (s.now.track) upNext.unshift(s.now.track);
      s = { ...s, now: { track: last.track, playing: true, positionMs: 0, at: Date.now() } };
    }
    emit();
  }

  const find = (id: string): Track | undefined => SONGS.find((t) => t.catalogId === id);

  return {
    subscribe(cb) {
      subs.add(cb);
      cb(s);
      armEnd();
      return () => subs.delete(cb);
    },

    async act(a: Action): Promise<ActionResult> {
      await new Promise((r) => setTimeout(r, 120)); // a network's worth of wait
      if (!s.active) return { ok: false, reason: "The host is not playing here" };
      if (!allowed(s, a.kind)) {
        const rule = (a.kind === "playNow" && s.rules.skip === "everyone" ? "add" : { play: "playPause", pause: "playPause", next: "skip", previous: "skip", seek: "seek", enqueue: "add", playNow: "skip", queueRemove: "changeQueue", queueMove: "changeQueue" }[a.kind as string]) as keyof GuestRules | undefined;
        return { ok: false, reason: rule ? `The host has set “${RULE_NAME[rule]}” to host only in DeetsMusic` : "Only the host can do that" };
      }
      switch (a.kind) {
        case "play":
        case "pause":
          s = { ...s, now: { ...s.now, positionMs: positionNow(s.now), at: Date.now(), playing: a.kind === "play" && !!s.now.track } };
          break;
        case "next":
          next();
          return { ok: true };
        case "previous":
          previous();
          return { ok: true };
        case "seek":
          s = { ...s, now: { ...s.now, positionMs: a.positionMs, at: Date.now() } };
          break;
        case "enqueue": {
          const t = find(a.catalogId);
          if (!t) return { ok: false, reason: "DeetsMusic could not find that song" };
          if (a.mode === "next") upNext.unshift(t);
          else upNext.push(t);
          if (!s.now.track) {
            next();
            return { ok: true };
          }
          break;
        }
        case "playNow": {
          const t = find(a.catalogId);
          if (!t) return { ok: false, reason: "DeetsMusic could not find that song" };
          if (a.dropCopy) {
            const copy = upNext.map((u) => u.catalogId).lastIndexOf(a.catalogId);
            if (copy >= 0) upNext.splice(copy, 1);
          }
          if (s.now.track) history = [{ track: s.now.track, playedAt: Date.now() }, ...history];
          s = { ...s, now: { track: t, playing: true, positionMs: 0, at: Date.now() } };
          break;
        }
        case "queueRemove":
          upNext.splice(a.index, 1);
          break;
        case "queueMove": {
          const [t] = upNext.splice(a.index, 1);
          if (t) upNext.splice(a.to, 0, t);
          break;
        }
      }
      emit();
      return { ok: true };
    },

    async search(term) {
      await new Promise((r) => setTimeout(r, 250));
      const q = term.trim().toLowerCase();
      if (!q) return [];
      return SONGS.filter((t) => `${t.title} ${t.artistName} ${t.albumName ?? ""}`.toLowerCase().includes(q));
    },

    set(patch) {
      const { host, rules, ...rest } = patch;
      s = { ...s, ...rest, rules: { ...s.rules, ...rules }, you: host === undefined ? s.you : { ...s.you, host } };
      emit();
    },

    state: () => s,
  };
}
