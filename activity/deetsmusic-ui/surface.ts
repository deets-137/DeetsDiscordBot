// Surfaces: DeetsMusic's Mini, Midi and Max, picked from the frame's size with DeetsMusic's own
// bands and dead-bands (../../../DeetsMusic/src/surface.ts; ACTIVITY.md §3). The app lets the
// user pick a surface and resize inside its band. A frame in Discord has no such pick, so the
// size alone decides, and the dead-bands keep a frame near an edge from flickering.
//
// Two compositions the app does not have (ACTIVITY.md §13.5, his calls 1 and 3):
//   wide  a short, wide frame (a phone in landscape): Now Playing as the player on the left,
//         one card on the right. The app never meets a window that short.
//   tile  Discord's picture-in-picture and grid tiles: the cover with the title, the artist
//         and the album. No controls: a tile takes no click.
// Both are drawn as Mini's player (`data-surface="mini" data-mini="player"`), so every rule of
// the look for the player holds; `data-wide` and `data-tile` add the rest (ui.css).

export type SurfaceName = "mini" | "midi" | "max";
/** What Mini shows: Now Playing plus one card, or the player (the Now Playing card alone). */
export type MiniView = "cards" | "player";

// DeetsMusic's numbers (the owner's, 2026-09-15 and 2026-09-17).
const MINI_CEIL = 460;
const MIDI_CEIL = 820;
const HYST = 40;
const MINI_HYST = 5;
const MAX_FLOOR_H = 745;
// Ours: under this height a frame wider than Mini is "wide". Midi's two cards need more.
const WIDE_CEIL_H = 500;
const WIDE_HYST = 20;

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

/** Is the frame short and wide? `surface` is the band after its own flip; `cur` is the answer
 *  now, so the height edge has a dead-band of its own. */
export function wideFor(h: number, surface: SurfaceName, cur: boolean): boolean {
  if (surface === "mini") return false;
  return cur ? h < WIDE_CEIL_H + WIDE_HYST : h < WIDE_CEIL_H;
}

export interface SurfaceState {
  surface: SurfaceName;
  mini: MiniView;
  /** A short, wide frame: the player on the left, one card on the right. */
  wide: boolean;
  /** Discord's picture-in-picture and grid tiles: the tile card alone. */
  tile: boolean;
}

const root = document.documentElement;
const listeners = new Set<(s: SurfaceState) => void>();
const first = bandFor(window.innerWidth, window.innerHeight);
let state: SurfaceState = { surface: first, mini: "cards", wide: wideFor(window.innerHeight, first, false), tile: false };

const flag = (name: string, on: boolean): void => {
  if (on) root.dataset[name] = "";
  else delete root.dataset[name];
};

function paint(): void {
  const asPlayer = state.tile || state.wide;
  root.dataset.surface = asPlayer ? "mini" : state.surface;
  root.dataset.mini = asPlayer ? "player" : state.mini;
  flag("tile", state.tile);
  flag("wide", state.wide && !state.tile);
}

function set(next: Partial<SurfaceState>): void {
  const merged = { ...state, ...next };
  if (merged.surface === state.surface && merged.mini === state.mini && merged.wide === state.wide && merged.tile === state.tile) return;
  state = merged;
  paint();
  for (const cb of listeners) cb(state);
}

export const surfaceState = (): SurfaceState => state;
export const setMiniView = (mini: MiniView): void => set({ mini });
/** The shell calls this when Discord reports picture-in-picture or grid. */
export const setTile = (tile: boolean): void => set({ tile });

export function onSurface(cb: (s: SurfaceState) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export function initSurface(): void {
  paint();
  const read = (): void => {
    const surface = flipFor(window.innerWidth, window.innerHeight, state.surface);
    set({ surface, wide: wideFor(window.innerHeight, surface, state.wide) });
  };
  window.addEventListener("resize", read);
  read();
}
