// What the scenery drivers read about the music: the stand-in for DeetsMusic's player.ts and
// queue.ts (the app's drivers import those; here the host's state is the source). It also
// carries two moments the drivers answer: a play the person asked for (Ocean ripples where
// they tapped) and a row dropped in the Queue.

import type { HostState, Track } from "../deetsmusic-protocol";
import type { Store } from "./store";

let state: HostState | null = null;
const subs = new Set<() => void>();
const intents = new Set<() => void>();
const lands = new Set<() => void>();

export function initPlayerBus(store: Store): void {
  store.watch((s) => {
    state = s;
    subs.forEach((cb) => cb());
  });
}

export const currentTrack = (): Track | null => state?.now.track ?? null;
export const isPlayingNow = (): boolean => !!state?.now.playing && !!state.now.track;
export const upNext = (): Track[] => state?.upNext ?? [];
export const history = (): Track[] => (state?.history ?? []).map((h) => h.track);

/** Be told on every state message (a song, a play state, the queue). */
export function onPlayerState(cb: () => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}

/** A play this person asked for (play, next, previous, Play Now). */
export function onPlayIntent(cb: () => void): () => void {
  intents.add(cb);
  return () => intents.delete(cb);
}
export const playIntent = (): void => intents.forEach((cb) => cb());

/** A row was dropped in the Queue. */
export function onDragLand(cb: () => void): () => void {
  lands.add(cb);
  return () => lands.delete(cb);
}
export const dragLanded = (): void => lands.forEach((cb) => cb());
