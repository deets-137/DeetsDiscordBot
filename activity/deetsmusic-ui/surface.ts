// Surfaces: DeetsMusic's Mini, Midi and Max, picked from the frame's size with DeetsMusic's own
// bands and dead-bands (../../../DeetsMusic/src/surface.ts; ACTIVITY.md §3). The app lets the
// user pick a surface and resize inside its band. A frame in Discord has no such pick, so the
// size alone decides, and the dead-bands keep a frame near an edge from flickering.

export type SurfaceName = "mini" | "midi" | "max";
/** What Mini shows: Now Playing plus one card, or the player (the Now Playing card alone). */
export type MiniView = "cards" | "player";

// DeetsMusic's numbers (the owner's, 2026-09-15 and 2026-09-17).
const MINI_CEIL = 460;
const MIDI_CEIL = 820;
const HYST = 40;
const MINI_HYST = 5;
const MAX_FLOOR_H = 745;

function bandFor(w: number, h: number): SurfaceName {
  if (w <= MINI_CEIL) return "mini";
  if (w <= MIDI_CEIL) return "midi";
  return h < MAX_FLOOR_H ? "midi" : "max";
}

/** Where a resize takes the current surface: a flip only past the threshold and its dead-band. */
export function flipFor(w: number, h: number, cur: SurfaceName): SurfaceName {
  switch (cur) {
    case "mini":
      return w > MINI_CEIL + MINI_HYST ? bandFor(w, h) : "mini";
    case "midi":
      if (w > MIDI_CEIL + HYST && h > MAX_FLOOR_H + HYST) return "max";
      if (w < MINI_CEIL - MINI_HYST) return "mini";
      return "midi";
    case "max":
      if (w < MIDI_CEIL - HYST) return bandFor(w, h);
      return h < MAX_FLOOR_H ? "midi" : "max";
  }
}

export interface SurfaceState {
  surface: SurfaceName;
  mini: MiniView;
  /** The cover alone: Discord's picture-in-picture and grid tiles (DeetsMusic's pin › Cover). */
  cover: boolean;
}

const root = document.documentElement;
const listeners = new Set<(s: SurfaceState) => void>();
let state: SurfaceState = { surface: bandFor(window.innerWidth, window.innerHeight), mini: "cards", cover: false };

function paint(): void {
  root.dataset.surface = state.cover ? "mini" : state.surface;
  // The cover view is the player view with the rest taken away (styles/pin.css).
  root.dataset.mini = state.cover ? "player" : state.mini;
  if (state.cover) root.dataset.cover = "";
  else delete root.dataset.cover;
}

function set(next: Partial<SurfaceState>): void {
  const merged = { ...state, ...next };
  if (merged.surface === state.surface && merged.mini === state.mini && merged.cover === state.cover) return;
  state = merged;
  paint();
  for (const cb of listeners) cb(state);
}

export const surfaceState = (): SurfaceState => state;
export const setMiniView = (mini: MiniView): void => set({ mini });
/** The shell calls this when Discord reports picture-in-picture or grid. */
export const setCover = (cover: boolean): void => set({ cover });

export function onSurface(cb: (s: SurfaceState) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export function initSurface(): void {
  paint();
  const read = (): void => set({ surface: flipFor(window.innerWidth, window.innerHeight, state.surface) });
  window.addEventListener("resize", read);
  read();
}
