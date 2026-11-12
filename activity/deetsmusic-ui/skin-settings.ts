// The host's skin settings (DeetsMusic › Settings › Skin settings, and the Motion rows the
// scenery obeys), as the look reads them. They ride in the state message (`look`, ACTIVITY.md
// §14.2): the host's choice, the same for everyone (F3). A key the host did not send keeps
// DeetsMusic's own default (src/settings-store.ts), so an older bot still draws the default look.
//
// This file does what DeetsMusic's skin-settings.ts does: the attributes and the custom
// properties on <html> that the copied stylesheets read. The drivers (vinyl.ts, ocean.ts …)
// read the values here with `setting`.

import type { SkinLook } from "../deetsmusic-protocol";

/** DeetsMusic's defaults (src/settings-store.ts, 1.4.2). */
export const SKIN_DEFAULTS: Required<SkinLook> = {
  fancyScrubber: true,
  backgroundMotion: "on",
  oceanEdges: "soft",
  oceanSand: 15,
  oceanCardOpacity: 80,
  oceanLight: 80,
  pressLight: 80,
  cyberLight: 80,
  albumWave: true,
  glassFancy: false,
  glassTint: 65,
  glassBacklight: 85,
  glassCanvasGlow: 40,
  glassCanvasDim: 10,
  glassCanvas: "covers",
  glassTiles: "one",
  glassDiffusion: 30,
  glassAuroraColor: "cover",
  pressVinyl: "spin",
  pressVinylWhere: "everywhere",
  pressVinylPlate: false,
  pressVinylSpeed: "16",
  pressVinylFps: "120",
  metalMaterial: "steel",
  metalGrain: "fine",
  metalCoverChange: "zoetrope",
  metalPeek: true,
  happySky: "follow",
  happyCardOpacity: 80,
  sun: null,
};
/** Glass holds these four while Fancy Glass is off (DeetsMusic's GLASS_LOCKED). */
const GLASS_LOCKED = { glassTint: 65, glassBacklight: 85, glassCanvasGlow: 40, glassCanvasDim: 10 } as const;

type Key = keyof SkinLook;
let current: Required<SkinLook> = { ...SKIN_DEFAULTS };
const subs = new Set<(key: Key) => void>();
const root = document.documentElement;

export function setting<K extends Key>(key: K): Required<SkinLook>[K] {
  return current[key];
}
/** Be told which key changed. Returns the unsubscribe. */
export function onSettingsChange(cb: (key: Key) => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}

const SLIDERS: Partial<Record<Key, [string, string]>> = {
  happyCardOpacity: ["--card-opacity", ""],
  glassDiffusion: ["--glass-diffusion", ""],
  glassTint: ["--glass-tint", "%"],
  glassBacklight: ["--glass-backlight", ""],
  glassCanvasGlow: ["--glass-canvas", ""],
  glassCanvasDim: ["--glass-canvas-dim", ""],
  oceanSand: ["--ocean-sand", ""],
  oceanCardOpacity: ["--ocean-card-opacity", ""],
  oceanLight: ["--ocean-light", ""],
  pressLight: ["--press-light", ""],
  cyberLight: ["--cyber-light", ""],
};
const clamp = (v: unknown): number => Math.max(0, Math.min(100, Math.round(Number(v) || 0)));

function paint(): void {
  const s = current;
  root.dataset.bgMotion = s.backgroundMotion;
  root.dataset.oceanEdges = s.oceanEdges;
  root.dataset.fancyScrub = s.fancyScrubber ? "on" : "off";
  root.dataset.glassFancy = s.glassFancy ? "on" : "off";
  root.dataset.pressVinyl = s.pressVinyl;
  root.dataset.pressVinylWhere = s.pressVinylWhere;
  root.dataset.pressVinylPlate = s.pressVinylPlate ? "on" : "off";
  root.dataset.metalMaterial = s.metalMaterial;
  root.dataset.metalCover = s.metalCoverChange;
  if (s.metalPeek && s.metalCoverChange === "zoetrope") root.dataset.metalPeek = "";
  else delete root.dataset.metalPeek;
  for (const [key, [prop, unit]] of Object.entries(SLIDERS) as [Key, [string, string]][]) {
    const v = key in GLASS_LOCKED && !s.glassFancy ? GLASS_LOCKED[key as keyof typeof GLASS_LOCKED] : s[key];
    root.style.setProperty(prop, `${clamp(v)}${unit}`);
  }
}

/** The host's values arrived (a state message). Only a change repaints and notifies. */
export function applySkinSettings(look: SkinLook | undefined): void {
  const next: Required<SkinLook> = { ...SKIN_DEFAULTS };
  for (const k of Object.keys(SKIN_DEFAULTS) as Key[]) {
    const v = look?.[k];
    if (v !== undefined && v !== null && typeof v === typeof SKIN_DEFAULTS[k]) (next as Record<Key, unknown>)[k] = v;
    else if (k === "sun" && v) (next as Record<Key, unknown>)[k] = v;
  }
  const changed = (Object.keys(next) as Key[]).filter((k) => JSON.stringify(next[k]) !== JSON.stringify(current[k]));
  current = next;
  if (!changed.length) return;
  paint();
  for (const k of changed) subs.forEach((cb) => cb(k));
}

export function initSkinSettings(): void {
  paint();
}
