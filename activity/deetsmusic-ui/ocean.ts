// The Ocean sea: a port of DeetsMusic's src/ocean.ts (its docs/features/OCEAN.md; ACTIVITY.md
// §14.2). CSS moves the layers; this module gives them something to show:
//
//  - The swell. A worker (ocean-worker.ts, copied from the app) paints three depth bands at the
//    sea's height, in the theme's water and ink. Each lands as a plain background. A theme
//    change, a new sea height or a new display scale repaints them.
//  - The glow from the deep: the album's colour as --ocean-glow-color; CSS fades it.
//  - The neon (the host's Album light, above 0): the worker paints each band's crest lines
//    alone, in the album's colour, onto a layer inside the band. A new album crossfades.
//  - Ripples: a play this person asked for ripples where they tapped; a dropped Queue row too.
// Not here (his call 1B, 2026-11-11): the heave. The app lifts the bands with the music's
// loudness; the page cannot hear the call, and the bot sends no level.
//
// Only while Ocean is the skin. Animate backgrounds Off, reduced motion or a hidden frame: no
// ripples (the bands also hold still, in CSS).

import { readPalette } from "./album-color";
import { albumColor, asNeon, fromOKLCH, isDark, parseColor, toOKLCH, type RGB } from "./album-slots";
import { motionReduced } from "./motion";
import { BANDS, seaRows, type LayerSpec } from "./ocean-texture";
import type { LayerJob } from "./ocean-worker";
import { currentTrack, onDragLand, onPlayerState, onPlayIntent } from "./player-bus";
import { onSettingsChange, setting } from "./skin-settings";

const GESTURE_MS = 1200;
const SAME_RIPPLE_MS = 500;
const SAME_RIPPLE_PX = 48;
const MAX_RIPPLES = 4;
const REPAINT_PX = 24;

const root = document.documentElement;
let sea: HTMLElement | null = null;
let probe: HTMLElement | null = null;
let paintSeq = 0;
let paintedHeight = 0;
let neonPainted = "";
let neonSeq = 0;
let glow: RGB | null = null;
let worker: Worker | null = null;
let jobSeq = 0;
const pending = new Map<number, (b: Blob | null) => void>();
const urls = new WeakMap<HTMLElement, string>();

const active = (): boolean => root.dataset.hookSea === "on" && !!sea;
const responsive = (): boolean => active() && root.dataset.bgMotion !== "off" && !motionReduced() && root.dataset.ambient !== "paused";
const log = (what: string): void => console.info(`[deetsmusic-ui] ocean: ${what}`);

// ── colours ─────────────────────────────────────────────────────────────────────
// Any CSS colour (a color-mix, a var) → sRGB, through a 1×1 canvas (DeetsMusic's album-neon.ts).
const swatch = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
swatch.canvas.width = swatch.canvas.height = 1;
function toRGB(css: string): RGB | null {
  swatch.clearRect(0, 0, 1, 1);
  swatch.fillStyle = "#000";
  swatch.fillStyle = css;
  swatch.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = swatch.getImageData(0, 0, 1, 1).data;
  return a ? [r / 255, g / 255, b / 255] : null;
}
/** A token resolved inside the sea (so skin and theme both apply). */
function resolve(expr: string): RGB | null {
  if (!sea) return null;
  if (!probe) {
    probe = document.createElement("span");
    probe.style.cssText = "position:absolute;width:0;height:0;overflow:hidden";
    sea.appendChild(probe);
  }
  probe.style.color = "";
  probe.style.color = expr;
  return toRGB(getComputedStyle(probe).color);
}
const to255 = (c: RGB): [number, number, number] => c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)) as [number, number, number];
const hex = (c: RGB): string => "#" + to255(c).map((v) => v.toString(16).padStart(2, "0")).join("");
function asGlow(c: RGB, darkWater: boolean): RGB {
  const [L, C, h] = toOKLCH(c);
  return fromOKLCH([darkWater ? Math.min(Math.max(L, 0.45), 0.6) : Math.min(Math.max(L, 0.55), 0.7), Math.min(C, 0.14), h]);
}

// ── the bands ───────────────────────────────────────────────────────────────────
let workerFailed = false;
function draw(spec: LayerSpec): Promise<Blob | null> {
  // A frame that refuses the worker (a content policy) keeps the plain sea: nothing waits forever.
  if (workerFailed) return Promise.resolve(null);
  try {
    worker ??= (() => {
      const w = new Worker(new URL("./ocean-worker.ts", import.meta.url), { type: "module" });
      w.onmessage = (e: MessageEvent<{ id: number; blob: Blob | null; err?: string }>) => {
        if (e.data.err) console.warn(`[deetsmusic-ui] ocean: a paint failed: ${e.data.err}`);
        pending.get(e.data.id)?.(e.data.blob);
        pending.delete(e.data.id);
      };
      w.onerror = () => {
        workerFailed = true;
        console.warn("[deetsmusic-ui] ocean: the painter could not start; the sea stays plain");
        pending.forEach((done) => done(null));
        pending.clear();
      };
      return w;
    })();
  } catch {
    workerFailed = true;
    return Promise.resolve(null);
  }
  const id = ++jobSeq;
  return new Promise((res) => {
    pending.set(id, res);
    worker!.postMessage({ id, spec } satisfies LayerJob);
  });
}

async function setImage(el: HTMLElement, blob: Blob, size: string): Promise<void> {
  const url = URL.createObjectURL(blob);
  const img = new Image();
  img.src = url;
  await img.decode().catch(() => {});
  const old = urls.get(el);
  el.style.backgroundImage = `url("${url}")`;
  el.style.backgroundSize = size;
  urls.set(el, url);
  if (old) URL.revokeObjectURL(old);
}

async function paintSwell(): Promise<void> {
  if (!active() || !sea) return;
  const height = sea.clientHeight;
  if (height < 40) return;
  const mine = ++paintSeq;
  paintedHeight = height;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const top = resolve("var(--ocean-water-top)") ?? [0, 0, 0];
  const bottom = resolve("var(--ocean-water-bottom)") ?? top;
  const ink = resolve("var(--ocean-swell-ink)") ?? [0.5, 0.5, 0.5];
  const css = getComputedStyle(sea);
  const shade = parseFloat(css.getPropertyValue(isDark(bottom) ? "--ocean-trough" : "--ocean-trough-light")) || 0.2;
  const rows = seaRows(height);
  const t0 = performance.now();
  await Promise.all(
    BANDS.map(async (band, i) => {
      const blob = await draw({ w: Math.round(band.tile * dpr), h: Math.round(height * dpr), px: dpr, rows: rows[band.name], ink: to255(ink), top: to255(top), bottom: to255(bottom), shade, seed: 3 + i * 8 });
      const el = sea?.querySelector<HTMLElement>(`.ocean__train--${band.name}`);
      if (!blob || !el || mine !== paintSeq) return;
      el.style.setProperty("--tw", `${band.tile}px`);
      await setImage(el, blob, `${band.tile}px 100%`);
      el.toggleAttribute("data-on", true);
    }),
  );
  if (mine !== paintSeq) return;
  log(`swell painted (${Math.round(performance.now() - t0)} ms, ${height} px)`);
  neonPainted = "";
  void paintNeon();
}

// ── the album light ─────────────────────────────────────────────────────────────
const neonLayers = (band: string): HTMLElement[] => [...(sea?.querySelectorAll<HTMLElement>(`.ocean__train--${band} .ocean__neon`) ?? [])];

async function paintNeon(): Promise<void> {
  if (!active() || !sea || setting("oceanLight") <= 0 || !paintedHeight) return;
  if (!glow) {
    sea.querySelectorAll(".ocean__neon[data-on]").forEach((n) => n.removeAttribute("data-on"));
    neonPainted = "";
    return;
  }
  const water = resolve("var(--ocean-water-bottom)") ?? [0, 0, 0];
  const { core, bloom } = asNeon(glow, isDark(water));
  const height = paintedHeight;
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const key = `${hex(core)}${hex(bloom)}|${height}|${dpr}`;
  if (key === neonPainted) return;
  neonPainted = key;
  const mine = ++neonSeq;
  const rows = seaRows(height);
  const next: HTMLElement[] = [];
  await Promise.all(
    BANDS.map(async (band, i) => {
      const blob = await draw({
        w: Math.round(band.tile * dpr), h: Math.round(height * dpr), px: dpr, rows: rows[band.name],
        ink: [0, 0, 0], top: [0, 0, 0], bottom: [0, 0, 0], shade: 0, seed: 3 + i * 8,
        neon: { core: to255(core), bloom: to255(bloom) },
      });
      const layers = neonLayers(band.name);
      const off = layers.find((n) => !n.hasAttribute("data-on")) ?? layers[0];
      if (!blob || !off || mine !== neonSeq) return;
      await setImage(off, blob, `${band.tile}px 100%`);
      next.push(off);
    }),
  );
  if (mine !== neonSeq) return;
  for (const band of BANDS) neonLayers(band.name).forEach((n) => n.toggleAttribute("data-on", next.includes(n)));
  log(`neon painted in ${hex(bloom)}`);
}

// ── the glow from the deep ──────────────────────────────────────────────────────
let liveCover: string | null | undefined;
function applyGlow(): void {
  if (!sea) return;
  if (!glow) return void sea.style.setProperty("--ocean-glow-color", "transparent");
  const water = resolve("var(--ocean-water-bottom)") ?? [0, 0, 0];
  sea.style.setProperty("--ocean-glow-color", hex(asGlow(glow, isDark(water))));
}
function followAlbum(): void {
  if (!active()) return;
  const t = currentTrack();
  const cover = t?.artworkTemplate ?? null;
  if (cover === liveCover) return;
  liveCover = cover;
  if (!t || !cover) {
    glow = null;
    applyGlow();
    void paintNeon();
    return;
  }
  void readPalette(t).then((p) => {
    if (liveCover !== cover) return;
    const c = albumColor(p);
    glow = (c && parseColor(c)) || null;
    applyGlow();
    void paintNeon();
  });
}

// ── ripples ─────────────────────────────────────────────────────────────────────
let lastUp = { x: 0, y: 0, t: -Infinity };
let lastRipple = { x: 0, y: 0, t: -Infinity };
function ripple(x: number, y: number): void {
  if (!responsive() || !sea) return;
  const now = performance.now();
  if (now - lastRipple.t < SAME_RIPPLE_MS && Math.hypot(x - lastRipple.x, y - lastRipple.y) < SAME_RIPPLE_PX) return;
  lastRipple = { x, y, t: now };
  const host = sea.querySelector<HTMLElement>(".ocean__ripples");
  if (!host) return;
  while (host.childElementCount >= MAX_RIPPLES * 2) host.firstElementChild?.remove();
  const box = sea.getBoundingClientRect();
  const gap = getComputedStyle(sea).getPropertyValue("--ocean-ripple-gap").trim() || "0.35s";
  for (const delay of ["0s", gap]) {
    const ring = document.createElement("i");
    ring.className = "ocean__ripple";
    ring.style.setProperty("--x", `${x - box.left}px`);
    ring.style.setProperty("--y", `${y - box.top}px`);
    ring.style.setProperty("--delay", delay);
    ring.addEventListener("animationend", () => ring.remove(), { once: true });
    host.appendChild(ring);
  }
}

// ── wiring ──────────────────────────────────────────────────────────────────────
function enter(): void {
  liveCover = undefined;
  log("on");
  requestAnimationFrame(() => {
    void paintSwell();
    followAlbum();
  });
}

export function initOcean(): void {
  sea = document.querySelector<HTMLElement>(".ocean");
  if (!sea) return;
  let was = active();
  if (was) enter();
  // The skin (data-hook-sea) and the theme (the water and the ink) come from the host.
  new MutationObserver(() =>
    requestAnimationFrame(() => {
      const now = active();
      if (now && !was) enter();
      else if (now) {
        void paintSwell();
        applyGlow();
      } else if (was) log("off");
      was = now;
    }),
  ).observe(root, { attributes: true, attributeFilter: ["data-theme", "data-hook-sea"] });
  onSettingsChange((k) => {
    if (k === "oceanLight") void paintNeon();
  });
  let resizeTimer: number | undefined;
  new ResizeObserver(() => {
    if (!active() || Math.abs(sea!.clientHeight - paintedHeight) < REPAINT_PX) return;
    window.clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(() => void paintSwell(), 300);
  }).observe(sea);
  const watchDpr = (): void => {
    window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`).addEventListener("change", () => {
      void paintSwell();
      watchDpr();
    }, { once: true });
  };
  watchDpr();
  onPlayerState(followAlbum);
  document.addEventListener("pointerup", (e) => (lastUp = { x: e.clientX, y: e.clientY, t: performance.now() }), { capture: true, passive: true });
  onPlayIntent(() => {
    if (performance.now() - lastUp.t < GESTURE_MS) ripple(lastUp.x, lastUp.y);
  });
  onDragLand(() => ripple(lastUp.x, lastUp.y));
}
