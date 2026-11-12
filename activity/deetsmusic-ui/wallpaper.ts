// Glass's cover wallpaper: a port of DeetsMusic's src/wallpaper.ts (its COVER-WALLPAPER.md;
// ACTIVITY.md §14.2). Under Glass with the host's Canvas set to Covers, the canvas behind the
// cards shows the playing album's cover large, with Up Next's next albums (then History's) as
// tiles around it (the host's Tiles row).
//
//   SHARP  the `.wallpaper` layer under the aurora, one element per tile, so one tile can fade
//          alone. The tiles load Apple's covers directly.
//   SOFT   one baked JPEG (wallpaper-worker.ts, copied from the app: small, blurred, saturated)
//          that the painted frost lays under each card. Fancy Glass blurs the sharp layer live.
// The aurora over it takes the album's colours (the host's Aurora colour row).
//
// The host's own picture (Canvas › Picture) never leaves the PC: a host on Picture shows Covers
// here. Nothing is saved: object URLs in memory only.

import { artworkURL, type Track } from "../deetsmusic-protocol";
import { readPalette } from "./album-color";
import { auroraSlots, type AlbumPalette } from "./album-slots";
import { motionReduced, tokenMs } from "./motion";
import { currentTrack, history, onPlayerState, upNext } from "./player-bus";
import { onSettingsChange, setting } from "./skin-settings";
import type { WallJob, WallTile } from "./wallpaper-worker";

const TILE_COUNT = { one: 0, few: 6, some: 12, many: 20 } as const;
const FETCH_PX = [120, 240, 480, 720, 1000, 1400];
const SOFT_SCALE = 0.25;
const PRELOAD_MS = 3000;
const RESIZE_MS = 300;

interface Rect { x: number; y: number; w: number; h: number }
interface Slot extends Rect {
  anchor: boolean;
  el: HTMLElement;
  key: string | null;
}

const root = document.documentElement;
let layer: HTMLElement | null = null;
let set: HTMLElement | null = null;
let slots: Slot[] = [];
let geom = "";
let shown = false;
let anchorKey: string | null = null;
let dirty = false;
let seq = 0;
let pending = "";
let softUrl: string | null = null;
let softSeq = 0;
let worker: Worker | null = null;
let resizeTimer = 0;

const hidden = (): boolean => root.dataset.ambient === "paused" || "tile" in root.dataset; // the tile covers it: draw nothing, catch up after
/** Covers or the plain aurora. Picture is the host's own file: Covers here. */
const on = (): boolean => root.dataset.hookWallpaper === "on" && setting("glassCanvas") !== "aurora";
const ms = (token: string): number => (motionReduced() ? 0 : tokenMs(token));
const fadeEase = (): string => getComputedStyle(root).getPropertyValue("--wallpaper-fade-ease").trim() || "ease";
const num = (token: string, fallback: number): number => parseFloat(getComputedStyle(root).getPropertyValue(token)) || fallback;
const log = (what: string): void => console.info(`[deetsmusic-ui] wallpaper: ${what}`);

// ── the layout ──────────────────────────────────────────────────────────────────
function hash(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}
function random(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
/** The anchor and `n` tiles over a `w × h` canvas (DeetsMusic's layout, unchanged). */
function layout(w: number, h: number, n: number): { anchor: Rect; tiles: Rect[] } {
  const bleed = num("--wallpaper-bleed", 48);
  if (n === 0) return { anchor: { x: -bleed, y: -bleed, w: w + 2 * bleed, h: h + 2 * bleed }, tiles: [] };
  const rnd = random(hash(`${w}x${h}:${n}`));
  const share = num("--wallpaper-anchor-share", 0.6);
  const rise = num("--wallpaper-anchor-rise", 0.3);
  const A = Math.min(w, h) * share;
  let k = 2;
  let s = A / k;
  let C = Math.ceil((w + 2 * bleed) / s);
  let R = Math.ceil((h + 2 * bleed) / s);
  while (C * R - k * k < n && k < 8) {
    k++;
    s = A / k;
    C = Math.ceil((w + 2 * bleed) / s);
    R = Math.ceil((h + 2 * bleed) / s);
  }
  const ox = (w - C * s) / 2;
  const oy = (h - R * s) / 2;
  const ac = Math.floor((C - k) / 2);
  const ar = Math.max(0, Math.round((R - k) * rise));
  const used: boolean[][] = Array.from({ length: R }, (_, r) => Array.from({ length: C }, (_, c) => r >= ar && r < ar + k && c >= ac && c < ac + k));
  const fits = (c: number, r: number, cw: number, ch: number): boolean => {
    if (c + cw > C || r + ch > R) return false;
    for (let y = r; y < r + ch; y++) for (let x = c; x < c + cw; x++) if (used[y][x]) return false;
    return true;
  };
  const rect = (c: number, r: number, cw: number, ch: number): Rect => ({
    x: Math.round(ox + c * s),
    y: Math.round(oy + r * s),
    w: Math.round(ox + (c + cw) * s) - Math.round(ox + c * s),
    h: Math.round(oy + (r + ch) * s) - Math.round(oy + r * s),
  });
  let free = C * R - k * k;
  let left = n;
  const tiles: Rect[] = [];
  for (let r = 0; r < R; r++) {
    for (let c = 0; c < C; c++) {
      if (used[r][c]) continue;
      const want = left > 0 ? (free / left) * (0.7 + 0.6 * rnd()) : 4;
      const wide = rnd() < 0.5;
      const order: [number, number][] =
        want >= 3 ? [[2, 2], wide ? [2, 1] : [1, 2], wide ? [1, 2] : [2, 1], [1, 1]] : want >= 1.6 ? [wide ? [2, 1] : [1, 2], wide ? [1, 2] : [2, 1], [1, 1]] : [[1, 1]];
      const [cw, ch] = order.find(([a, b]) => fits(c, r, a, b) && free - a * b >= Math.max(0, left - 1)) ?? [1, 1];
      for (let y = r; y < r + ch; y++) for (let x = c; x < c + cw; x++) used[y][x] = true;
      tiles.push(rect(c, r, cw, ch));
      free -= cw * ch;
      left--;
    }
  }
  const anchor = rect(ac, ar, k, k);
  const cx = anchor.x + anchor.w / 2;
  const cy = anchor.y + anchor.h / 2;
  const dist = (t: Rect): number => Math.hypot(t.x + t.w / 2 - cx, t.y + t.h / 2 - cy);
  tiles.sort((a, b) => dist(a) - dist(b));
  return { anchor, tiles };
}

// ── the picture set: Up Next's next distinct albums, then History newest first ──
function tileKeys(anchor: string, n: number): string[] {
  const seen = new Set([anchor]);
  const out: string[] = [];
  for (const t of [...upNext(), ...history()]) {
    if (out.length >= n) break;
    const k = t.artworkTemplate;
    if (k && !seen.has(k)) {
      seen.add(k);
      out.push(k);
    }
  }
  const pool = out.length ? [...out] : [anchor];
  while (out.length < n) out.push(pool[out.length % pool.length]);
  return out;
}
function linkFor(key: string, r: Rect): string {
  const want = Math.max(r.w, r.h) * (window.devicePixelRatio || 1);
  const px = FETCH_PX.find((p) => p >= want) ?? FETCH_PX[FETCH_PX.length - 1];
  return artworkURL({ artworkTemplate: key } as Track, px) ?? "";
}
function preload(url: string): Promise<void> {
  const img = new Image();
  img.src = url;
  return Promise.race([img.decode().catch(() => {}), new Promise<void>((r) => setTimeout(r, PRELOAD_MS))]);
}

// ── the sharp layer ─────────────────────────────────────────────────────────────
function imgEl(url: string): HTMLElement {
  const i = document.createElement("i");
  i.className = "wallpaper__img";
  i.style.backgroundImage = `url("${url}")`;
  return i;
}
function swapSlot(slot: Slot, key: string, from?: Rect): void {
  slot.key = key;
  const next = imgEl(linkFor(key, slot));
  const old = [...slot.el.children] as HTMLElement[];
  slot.el.appendChild(next);
  const dur = ms("--wallpaper-tile-dur");
  if (!dur) return old.forEach((o) => o.remove());
  const ease = getComputedStyle(root).getPropertyValue("--wallpaper-tile-ease").trim() || "ease";
  const scale = num("--wallpaper-tile-scale", 1.04);
  const start = from ? `translate(${from.x - slot.x}px, ${from.y - slot.y}px) scale(${from.w / slot.w}, ${from.h / slot.h})` : `scale(${scale})`;
  const origin = from ? "0 0" : "50% 50%";
  if (from) slot.el.style.zIndex = "1";
  next
    .animate([{ opacity: from ? 1 : 0, transform: start, transformOrigin: origin }, { opacity: 1, transform: "none", transformOrigin: origin }], { duration: dur, easing: ease })
    .finished.then(() => (slot.el.style.zIndex = ""), () => (slot.el.style.zIndex = ""));
  old.forEach((o) => o.animate([{ opacity: 1 }, { opacity: 0 }], { duration: dur, easing: ease, fill: "forwards" }).finished.then(() => o.remove(), () => o.remove()));
}
function buildSlots(w: number, h: number, n: number): void {
  const { anchor, tiles } = layout(w, h, n);
  const el = document.createElement("div");
  el.className = "wallpaper__set";
  const mk = (r: Rect, isAnchor: boolean): Slot => {
    const box = document.createElement("div");
    box.className = isAnchor ? "wallpaper__tile wallpaper__tile--anchor" : "wallpaper__tile";
    box.style.cssText = `left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px`;
    el.appendChild(box);
    return { ...r, anchor: isAnchor, el: box, key: null };
  };
  slots = [...tiles.map((t) => mk(t, false)), mk(anchor, true)];
  const old = set;
  set = el;
  layer!.appendChild(el);
  const dur = ms("--wallpaper-fade-dur");
  if (old) {
    if (dur) {
      el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: dur, easing: fadeEase() });
      old.animate([{ opacity: 1 }, { opacity: 0 }], { duration: dur, easing: fadeEase(), fill: "forwards" }).finished.then(() => old.remove(), () => old.remove());
    } else old.remove();
  }
}

// ── the soft copy (the painted frost's picture) ─────────────────────────────────
let fadeTimer = 0;
function inFade(change: () => void): void {
  const dur = ms("--wallpaper-fade-dur");
  if (!dur) return change();
  root.dataset.wallFade = "";
  requestAnimationFrame(() => {
    change();
    window.clearTimeout(fadeTimer);
    fadeTimer = window.setTimeout(() => delete root.dataset.wallFade, dur + 50);
  });
}
function setSoft(url: string | null): void {
  const old = softUrl;
  softUrl = url;
  inFade(() => {
    if (url) root.style.setProperty("--wallpaper-soft", `url("${url}")`);
    else root.style.removeProperty("--wallpaper-soft");
  });
  if (old) window.setTimeout(() => URL.revokeObjectURL(old), ms("--wallpaper-fade-dur") + 200);
}
function placeSoft(): void {
  const box = document.querySelector<HTMLElement>(".app-body")?.getBoundingClientRect();
  if (!box) return;
  root.style.setProperty("--wallpaper-x", `${box.left}px`);
  root.style.setProperty("--wallpaper-y", `${box.top}px`);
  root.style.setProperty("--wallpaper-w", `${box.width}px`);
  root.style.setProperty("--wallpaper-h", `${box.height}px`);
}
async function bakeSoft(w: number, h: number): Promise<void> {
  if (setting("glassFancy")) {
    if (softUrl) setSoft(null);
    return;
  }
  const mine = ++softSeq;
  // The worker fetches the covers itself: give it absolute links (it has no page to resolve against).
  const tiles: WallTile[] = slots.filter((s) => s.key).map((s) => ({ src: new URL(linkFor(s.key!, s), location.href).href, x: s.x, y: s.y, w: s.w, h: s.h }));
  if (!tiles.length) return;
  const cs = getComputedStyle(root);
  const job: Omit<WallJob, "id"> = {
    w, h, scale: SOFT_SCALE,
    blur: parseFloat(cs.getPropertyValue("--wallpaper-blur")) || 24,
    sat: parseFloat(cs.getPropertyValue("--glass-frost-sat")) || 1,
    bg: cs.getPropertyValue("--canvas").trim() || "#000",
    tiles,
  };
  const blob = await new Promise<Blob | null>((resolve) => {
    try {
      worker ??= new Worker(new URL("./wallpaper-worker.ts", import.meta.url), { type: "module" });
    } catch {
      return resolve(null);
    }
    worker.onmessage = (e: MessageEvent<{ id: number; blob: Blob | null }>) => {
      if (e.data.id === mine) resolve(e.data.blob);
    };
    worker.onerror = () => resolve(null);
    worker.postMessage({ id: mine, ...job });
  });
  if (mine !== softSeq || !shown || !blob) return;
  placeSoft();
  setSoft(URL.createObjectURL(blob));
}

// ── the aurora's colours ────────────────────────────────────────────────────────
const STOPS = ["--canvas-go", "--canvas-stop", "--canvas-pause"] as const;
function applyColors(p: AlbumPalette | null): void {
  const use = p && setting("glassAuroraColor") === "cover" ? auroraSlots(p) : null;
  inFade(() => {
    if (!use || !use[1]) return STOPS.forEach((s) => root.style.removeProperty(s));
    const [under, rim, halo] = use;
    root.style.setProperty("--canvas-go", rim!);
    root.style.setProperty("--canvas-stop", halo ?? rim!);
    root.style.setProperty("--canvas-pause", under ?? rim!);
  });
}

// ── enter, update, leave ────────────────────────────────────────────────────────
function leave(fade: boolean): void {
  ++seq;
  ++softSeq;
  pending = "";
  if (!shown && !root.hasAttribute("data-wallpaper")) return;
  shown = false;
  anchorKey = null;
  geom = "";
  const dur = fade ? ms("--wallpaper-fade-dur") : 0;
  const done = (): void => {
    if (shown) return;
    delete root.dataset.wallpaper;
    set?.remove();
    set = null;
    slots = [];
    if (layer) layer.style.opacity = "";
  };
  applyColors(null);
  setSoft(null);
  log("off");
  if (dur && layer) layer.animate([{ opacity: 1 }, { opacity: 0 }], { duration: dur, easing: fadeEase(), fill: "forwards" }).finished.then(done, done);
  else done();
}

async function update(reason: string): Promise<void> {
  if (!layer) return;
  if (!on()) return leave(root.dataset.hookWallpaper === "on");
  if (hidden()) {
    dirty = true;
    return;
  }
  dirty = false;
  const track = currentTrack();
  const key = track?.artworkTemplate ?? null;
  if (!key) return leave(true);
  const body = layer.parentElement!;
  const w = body.clientWidth;
  const h = body.clientHeight;
  if (!w || !h) return;
  const n = TILE_COUNT[setting("glassTiles")] ?? 0;
  const g = `${w}x${h}:${n}`;
  if (g === geom && key === anchorKey && shown) return;
  if (pending === `${g}|${key}`) return;
  pending = `${g}|${key}`;
  const mine = ++seq;
  const newGeom = g !== geom || !slots.length;
  const plan: { slot: Slot; key: string; from?: Rect }[] = [];
  try {
    if (newGeom) {
      buildSlots(w, h, n);
      geom = g;
      const keys = tileKeys(key, slots.length - 1);
      slots.forEach((s, i) => plan.push({ slot: s, key: s.anchor ? key : keys[i] ?? key }));
    } else {
      const keys = tileKeys(key, slots.length - 1);
      const anchorSlot = slots.find((s) => s.anchor)!;
      const tiles = slots.filter((s) => !s.anchor);
      const want = [...keys];
      const keep = new Set<Slot>();
      for (const s of tiles) {
        const i = s.key ? want.indexOf(s.key) : -1;
        if (i >= 0) {
          keep.add(s);
          want.splice(i, 1);
        }
      }
      const box = (s: Slot): Rect => ({ x: s.x, y: s.y, w: s.w, h: s.h });
      if (anchorSlot.key !== key) {
        // The swap: the new album grows out of its tile; the old one shrinks into it.
        const was = tiles.find((s) => s.key === key && !keep.has(s));
        plan.push({ slot: anchorSlot, key, from: was ? box(was) : undefined });
        const old = anchorSlot.key;
        const into = was ?? tiles.find((s) => !keep.has(s));
        if (old && into) {
          const dup = want.indexOf(old);
          if (dup >= 0) want.splice(dup, 1);
          keep.add(into);
          plan.push({ slot: into, key: old, from: box(anchorSlot) });
        }
      }
      for (const s of tiles) if (!keep.has(s)) plan.push({ slot: s, key: want.shift() ?? key });
    }
    await Promise.all(plan.map((p) => preload(linkFor(p.key, p.slot))));
  } finally {
    if (pending === `${g}|${key}`) pending = "";
  }
  if (mine !== seq || !on()) return;
  const entering = !shown;
  shown = true;
  anchorKey = key;
  root.dataset.wallpaper = "covers";
  if (entering) {
    layer.getAnimations().forEach((a) => a.cancel());
    const dur = ms("--wallpaper-fade-dur");
    if (dur) layer.animate([{ opacity: 0 }, { opacity: 1 }], { duration: dur, easing: fadeEase() });
  }
  plan.forEach((p) => swapSlot(p.slot, p.key, newGeom ? undefined : p.from));
  log(`drawn (${reason}, ${n} tiles, ${plan.length} changed)`);
  if (track) void readPalette(track).then((p) => mine === seq && applyColors(p));
  void bakeSoft(w, h);
}

export function initWallpaper(): void {
  layer = document.querySelector<HTMLElement>(".wallpaper");
  if (!layer) return;
  const body = layer.parentElement!;
  onPlayerState(() => {
    if (on() && (currentTrack()?.artworkTemplate ?? null) !== anchorKey) void update("album");
  });
  new MutationObserver(() => (on() ? void update("skin") : leave(false))).observe(root, { attributes: true, attributeFilter: ["data-hook-wallpaper", "data-skin"] });
  onSettingsChange((k) => {
    if (k === "glassCanvas" || k === "glassTiles") void update(k);
    else if (k === "glassAuroraColor" && shown) {
      const t = currentTrack();
      if (t) void readPalette(t).then(applyColors);
    } else if (k === "glassFancy" && shown) void bakeSoft(body.clientWidth, body.clientHeight);
  });
  new MutationObserver(() => {
    if (!hidden() && dirty) void update("show");
  }).observe(root, { attributes: true, attributeFilter: ["data-ambient", "data-tile"] });
  new ResizeObserver(() => {
    if (!shown) return;
    placeSoft();
    window.clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(() => void update("resize"), RESIZE_MS);
  }).observe(body);
  void update("boot");
}
