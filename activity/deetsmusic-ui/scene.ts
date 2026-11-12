// The scene around the cards (--hook-scene: on; the Happy skin): a port of DeetsMusic's
// src/scene.ts (its docs/features/happy.md §1.2; ACTIVITY.md §14.2). The pictures are the
// skin's (skin.css --scene-*); this module sizes and places them and paints the sky:
//   - --scene-px on <html>: the art's pixel size in whole device pixels (2× in Max, 1.5×
//     elsewhere), so the pixel art stays sharp.
//   - the sky: the host's Sky setting picks the part of the day, once a minute. *Follow the
//     time* walks night → dawn → day → dusk → night on the sun's times: the host's (F3) when the
//     bot sends them, else the viewer's own time zone (ACTIVITY.md §14.3).
//   - the sun or moon at the top-right card's corner, the far clouds behind the cards, the
//     edge clouds on the cards' outer edges.
// pet.ts puts Deets and Happy on the floor this module reports (`sceneFloor`). Inert under
// every other skin: the layers are display:none and nothing here runs.

import { ALIASES, ZONES } from "./look/src/sun-zones";
import { skyAt } from "./look/src/sky-phase";
import { setting, onSettingsChange } from "./skin-settings";
import { onSurface } from "./surface";
import type { SunTimes } from "../deetsmusic-protocol";

const root = document.documentElement;
const active = (): boolean => root.dataset.hookScene === "on";
let body: HTMLElement | null = null;
let back: HTMLElement | null = null;
let front: HTMLElement | null = null;
let skyTimer = 0;
let placing = 0;
const listeners = new Set<() => void>();
const log = (what: string): void => console.info(`[deetsmusic-ui] scene: ${what}`);

/** The art pixel size now, in CSS px (whole device pixels). */
export function scenePx(): number {
  const want = root.dataset.surface === "max" ? 2 : 1.5;
  const dpr = window.devicePixelRatio || 1;
  return Math.max(1, Math.round(want * dpr)) / dpr;
}

/** The line feet stand on, in .app-body's own px from its top, and its box. */
export function sceneFloor(): { y: number; w: number; h: number } | null {
  if (!body || !active()) return null;
  const cs = getComputedStyle(root);
  const px = scenePx();
  const tileH = num(cs, "--scene-grass-h") * px;
  const feet = num(cs, "--scene-grass-feet") * px;
  return { y: body.clientHeight - tileH + feet, w: body.clientWidth, h: body.clientHeight };
}
export function onScenePlace(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

const num = (cs: CSSStyleDeclaration, p: string): number => parseFloat(cs.getPropertyValue(p)) || 0;
/** The picture a token names: url("…") → the address, or "". */
export const urlOf = (v: string): string => /url\(\s*["']?([^"')]+)["']?\s*\)/.exec(v)?.[1] ?? "";

const sizes = new Map<string, Promise<{ w: number; h: number }>>();
const fails = new Map<string, number>();
const PICTURE_WAIT_MS = 3000;
const RETRY_PLACE_MS = 2000;
/** A picture's own size (cached), 0 × 0 when it cannot load (then a few retries). */
export function pictureSize(url: string): Promise<{ w: number; h: number }> {
  if (!url) return Promise.resolve({ w: 0, h: 0 });
  let p = sizes.get(url);
  if (!p) {
    p = new Promise((done) => {
      const img = new Image();
      const end = (w: number, h: number): void => {
        window.clearTimeout(limit);
        img.onload = img.onerror = null;
        if (!w || !h) {
          sizes.delete(url);
          const n = (fails.get(url) ?? 0) + 1;
          fails.set(url, n);
          if (n <= 5) window.setTimeout(schedulePlace, RETRY_PLACE_MS);
        }
        done({ w, h });
      };
      const limit = window.setTimeout(() => end(0, 0), PICTURE_WAIT_MS);
      img.onload = () => end(img.naturalWidth, img.naturalHeight);
      img.onerror = () => end(0, 0);
      img.src = url;
    });
    sizes.set(url, p);
  }
  return p;
}

// ── the sun (DeetsMusic's look-schedule.ts sunTimes, for the viewer's own zone) ──
const RAD = Math.PI / 180;
const DAY_MS = 86_400_000;
function place(): { lat: number; lon: number } {
  let zone = "";
  try {
    zone = Intl.DateTimeFormat().resolvedOptions().timeZone ?? "";
  } catch {
    /* no Intl time zone: the UTC offset below */
  }
  const id = ZONES[zone] ? zone : ALIASES[zone];
  const at = id ? ZONES[id] : undefined;
  if (at) return { lat: at[0], lon: at[1] };
  return { lat: 40, lon: -new Date().getTimezoneOffset() / 4 };
}
function viewerSun(day: Date): SunTimes {
  const p = place();
  const n = Math.round(Date.UTC(day.getFullYear(), day.getMonth(), day.getDate(), 12) / DAY_MS + 2440587.5 - 2451545);
  const j = n - p.lon / 360;
  const m = (357.5291 + 0.98560028 * j) % 360;
  const c = 1.9148 * Math.sin(m * RAD) + 0.02 * Math.sin(2 * m * RAD) + 0.0003 * Math.sin(3 * m * RAD);
  const l = (m + c + 180 + 102.9372) % 360;
  const transit = 2451545 + j + 0.0053 * Math.sin(m * RAD) - 0.0069 * Math.sin(2 * l * RAD);
  const dec = Math.asin(Math.sin(l * RAD) * Math.sin(23.4397 * RAD));
  const cosW = (Math.sin(-0.833 * RAD) - Math.sin(p.lat * RAD) * Math.sin(dec)) / (Math.cos(p.lat * RAD) * Math.cos(dec));
  if (cosW > 1) return "night";
  if (cosW < -1) return "day";
  const w = Math.acos(cosW) / RAD / 360;
  const ms = (jd: number): number => (jd - 2440587.5) * DAY_MS;
  return { rise: ms(transit - w), set: ms(transit + w) };
}
/** The host's sun when the bot sends it (F3), else the viewer's own. */
const sunNow = (): SunTimes => setting("sun") ?? viewerSun(new Date());

function paintSky(): void {
  if (!back) return;
  const s = skyAt(setting("happySky"), Date.now(), sunNow());
  const from = back.querySelector<HTMLElement>(".scene__sky--from");
  const to = back.querySelector<HTMLElement>(".scene__sky--to");
  if (!from || !to) return;
  if (from.dataset.part !== s.from) {
    to.style.transition = "none";
    to.style.setProperty("--sky-t", "0");
    void to.offsetWidth;
    to.style.transition = "";
  }
  from.dataset.part = s.from;
  to.dataset.part = s.to;
  to.style.setProperty("--sky-t", String(s.t));
  if (front) front.toggleAttribute("data-moon", s.moon);
}

const PLAYER_COPIES = ".np-still, .fp-still, .fp-ghost, .fp-history";
/** The visible cards' boxes, relative to .app-body. */
function cards(): DOMRect[] {
  if (!body) return [];
  const b = body.getBoundingClientRect();
  return [...body.querySelectorAll<HTMLElement>(`.bento .panel:not(${PLAYER_COPIES})`)]
    .filter((p) => p.offsetParent !== null && p.getClientRects().length > 0)
    .map((p) => {
      const r = p.getBoundingClientRect();
      return new DOMRect(r.left - b.left, r.top - b.top, r.width, r.height);
    })
    .filter((r) => r.width > 40 && r.height > 40);
}

async function placeAll(): Promise<void> {
  if (!body || !front || !back || !active()) return;
  const mine = ++placing;
  restSize = body.querySelector(".bento[data-player]") ? "" : sizeKey();
  const cs = getComputedStyle(root);
  const px = scenePx();
  root.style.setProperty("--scene-px", String(px));
  const W = body.clientWidth;
  back.style.setProperty("--scene-w", `${W}px`);
  const boxes = cards();
  const [sun, c1, c2, c3] = await Promise.all(["--scene-sun", "--scene-cloud-1", "--scene-cloud-2", "--scene-cloud-3"].map((p) => pictureSize(urlOf(cs.getPropertyValue(p)))));
  if (mine !== placing) return;

  // The sun: the frame's top-right corner, its lower part over the top-right card's corner.
  const sunEl = front.querySelector<HTMLElement>(".scene__sun");
  if (sunEl) {
    const d = sun.w * px;
    const inset = Math.max(0, num(cs, "--scene-sun-inset")) * px;
    const bar = body.getBoundingClientRect().top;
    sunEl.style.setProperty("--sun-w", `${d}px`);
    sunEl.style.setProperty("--sun-x", `${W - d - inset}px`);
    sunEl.style.setProperty("--sun-y", `${inset - bar}px`);
    sunHome = d ? { x: W - d / 2 - inset, y: inset - bar + d / 2, r: d / 2, top: -bar, w: W } : null;
    sunAway = false;
    sunEl.toggleAttribute("data-away", false);
    sunEl.style.removeProperty("--sun-dx");
    sunEl.style.removeProperty("--sun-dy");
    const rays = await pictureSize(urlOf(cs.getPropertyValue("--scene-sun-rays")));
    if (mine !== placing) return;
    sunEl.style.setProperty("--rays-k", String(sun.w ? rays.w / sun.w : 1));
  }

  const pics = [c1, c2, c3];
  const H = body.clientHeight;
  const farEls = cloudEls(back, "far", Math.round(num(cs, "--scene-far-clouds")));
  farEls.forEach((el, i) => {
    const c = cloudsFar[i];
    const s = pics[c.pic];
    el.hidden = !s.w;
    el.style.setProperty("--cloud-img", `var(--scene-cloud-${c.pic + 1})`);
    el.style.setProperty("--cloud-w", `${s.w * px}px`);
    el.style.setProperty("--cloud-h", `${s.h * px}px`);
    el.style.setProperty("--cloud-y", `${Math.round(H * c.y)}px`);
    el.style.setProperty("--cloud-alpha", String(c.alpha));
    el.style.setProperty("--cloud-trip", `calc(var(--scene-cloud-trip) * ${c.trip.toFixed(3)})`);
    el.style.setProperty("--cloud-at", `calc(var(--scene-cloud-trip) * ${(-c.trip * c.at).toFixed(3)})`);
  });

  const minL = boxes.length ? Math.min(...boxes.map((r) => r.left)) : 0;
  const maxR = boxes.length ? Math.max(...boxes.map((r) => r.right)) : 0;
  const sides = [boxes.filter((r) => r.left <= minL + 2).sort((a, b) => a.top - b.top), boxes.filter((r) => r.right >= maxR - 2).sort((a, b) => a.top - b.top)];
  const overC = Math.min(0.5, Math.max(0, num(cs, "--scene-cloud-over")));
  const edgeEls = cloudEls(front, "edge", Math.round(num(cs, "--scene-edge-clouds")));
  const onCard = new Map<DOMRect, number[]>();
  const spots = edgeEls.map((_, i) => {
    const side = i % 2;
    const list = sides[side];
    if (!list.length) return null;
    const card = list[Math.floor(cloudsEdge[i].card * list.length)];
    const n = onCard.get(card) ?? [];
    n.push(i);
    onCard.set(card, n);
    return { side, card };
  });
  edgeEls.forEach((el, i) => {
    const spot = spots[i];
    const c = cloudsEdge[i];
    const s = pics[c.pic];
    if (!spot || !s.w) {
      el.hidden = true;
      return;
    }
    el.hidden = false;
    const { side, card } = spot;
    const mates = onCard.get(card)!;
    const k = mates.indexOf(i);
    const n = mates.length;
    const y = card.top + card.height * ((k + 0.15 + c.y * 0.7) / n);
    const w = s.w * px;
    const h = s.h * px;
    el.style.setProperty("--cloud-img", `var(--scene-cloud-${c.pic + 1})`);
    el.style.setProperty("--cloud-w", `${w}px`);
    el.style.setProperty("--cloud-h", `${h}px`);
    el.style.setProperty("--cloud-x", `${side === 0 ? card.left - w + overC * w : card.right - overC * w}px`);
    el.style.setProperty("--cloud-y", `${Math.round(y - h / 2)}px`);
    el.style.setProperty("--cloud-bob", `calc(var(--scene-bob-dur) * ${c.bob.toFixed(2)})`);
    el.style.setProperty("--cloud-at", `${(-c.at * 10).toFixed(2)}s`);
  });
  listeners.forEach((cb) => cb());
}

// ── the sun makes way for the pointer ───────────────────────────────────────────
let sunHome: { x: number; y: number; r: number; top: number; w: number } | null = null;
let sunAway = false;
const SUN_NEAR = 1.3;
const SUN_CLEAR = 1.9;
const SUN_PEEK = 0.35;
function sunMove(away: { dx: number; dy: number } | null): void {
  const el = front?.querySelector<HTMLElement>(".scene__sun");
  if (!el) return;
  sunAway = !!away;
  el.toggleAttribute("data-away", sunAway);
  el.style.setProperty("--sun-dx", `${Math.round(away?.dx ?? 0)}px`);
  el.style.setProperty("--sun-dy", `${Math.round(away?.dy ?? 0)}px`);
}
function sunDodge(e: PointerEvent): void {
  const h = sunHome;
  if (!h || !body || !active()) return;
  const b = body.getBoundingClientRect();
  const px = e.clientX - b.left;
  const py = e.clientY - b.top;
  const dist = Math.hypot(px - h.x, py - h.y);
  if (!sunAway && dist < h.r * SUN_NEAR) {
    const fromBelow = py > h.y - h.r * 0.3;
    sunMove(fromBelow ? { dx: 0, dy: h.top + h.r * SUN_PEEK - (h.y + h.r) } : { dx: h.w - h.r * SUN_PEEK - (h.x - h.r), dy: 0 });
  } else if (sunAway && dist > h.r * SUN_CLEAR) sunMove(null);
}

// ── the clouds' draw: once a visit, from a seed ─────────────────────────────────
interface FarCloud { pic: number; y: number; trip: number; at: number; alpha: number }
interface EdgeCloud { pic: number; card: number; y: number; bob: number; at: number }
const cloudsFar: FarCloud[] = [];
const cloudsEdge: EdgeCloud[] = [];
let seed = (Math.random() * 2 ** 32) >>> 0;
function rnd(): number {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pickPic = (): number => {
  const r = rnd();
  return r < 0.4 ? 0 : r < 0.75 ? 1 : 2;
};
function drawFar(n: number): void {
  const bands = [...Array(n).keys()].sort(() => rnd() - 0.5);
  for (let i = cloudsFar.length; i < n; i++) {
    const pic = pickPic();
    cloudsFar.push({ pic, y: 0.03 + ((bands[i] + rnd() * 0.85) / n) * 0.57, trip: [1.3, 1.05, 0.85][pic] * (0.85 + rnd() * 0.3), at: rnd(), alpha: [0.8, 0.92, 1][pic] });
  }
}
function drawEdge(n: number): void {
  for (let i = cloudsEdge.length; i < n; i++) cloudsEdge.push({ pic: rnd() < 0.55 ? 0 : 1, card: rnd(), y: rnd(), bob: 0.7 + rnd() * 0.6, at: rnd() });
}
function cloudEls(layer: HTMLElement, kind: "far" | "edge", n: number): HTMLElement[] {
  n = Math.max(0, Math.min(40, n || 0));
  (kind === "far" ? drawFar : drawEdge)(n);
  const els = [...layer.querySelectorAll<HTMLElement>(`.scene__cloud--${kind}`)];
  while (els.length > n) els.pop()!.remove();
  while (els.length < n) {
    const el = document.createElement("div");
    el.className = `scene__cloud scene__cloud--${kind}`;
    if (kind === "edge") layer.querySelector(".scene__grass")?.before(el);
    else layer.append(el);
    els.push(el);
  }
  return els;
}

let placeFrame = 0;
function schedulePlace(): void {
  if (!active()) return;
  cancelAnimationFrame(placeFrame);
  placeFrame = requestAnimationFrame(() => void placeAll());
}

// The full player's copies and its open and close do not move the outer edges the clouds rest
// on: a player change places nothing while the frame keeps the size of the last resting place.
let restSize = "";
const sizeKey = (): string => (body ? `${body.clientWidth}x${body.clientHeight}` : "");
const playerOwn = (r: MutationRecord): boolean =>
  r.type === "attributes" ? r.attributeName === "data-player" : [...r.addedNodes, ...r.removedNodes].every((n) => n instanceof HTMLElement && n.matches(PLAYER_COPIES));
const holdForPlayer = (recs: MutationRecord[]): boolean => !!restSize && restSize === sizeKey() && recs.every(playerOwn);

function enter(): void {
  if (!body) return;
  paintSky();
  window.clearInterval(skyTimer);
  skyTimer = window.setInterval(paintSky, 60_000);
  schedulePlace();
  log(`on (sky: ${setting("happySky")}, ${setting("sun") ? "the host's sun" : "the viewer's sun"})`);
}
function leave(): void {
  window.clearInterval(skyTimer);
  skyTimer = 0;
  sunHome = null;
  sunAway = false;
  root.style.removeProperty("--scene-px");
  log("off");
}

export function initScene(): void {
  body = document.querySelector<HTMLElement>(".app-body");
  back = document.querySelector<HTMLElement>(".scene--back");
  front = document.querySelector<HTMLElement>(".scene--front");
  if (!body || !back || !front) return;
  let on = active();
  if (on) enter();
  new MutationObserver(() => {
    const now = active();
    if (now && !on) enter();
    else if (!now && on) leave();
    else if (now) schedulePlace();
    on = now;
  }).observe(root, { attributes: true, attributeFilter: ["data-hook-scene", "data-skin"] });
  onSettingsChange((k) => {
    if ((k === "happySky" || k === "sun") && active()) paintSky();
  });
  onSurface(() => schedulePlace());
  window.addEventListener("pointermove", sunDodge, { passive: true });
  root.addEventListener("pointerleave", () => {
    if (sunAway) sunMove(null);
  });
  new ResizeObserver(() => schedulePlace()).observe(body);
  const bento = body.querySelector(".bento");
  if (bento) new MutationObserver((recs) => {
    if (!holdForPlayer(recs)) schedulePlace();
  }).observe(bento, { attributes: true, childList: true, subtree: false, attributeFilter: ["data-player", "data-screen", "class"] });
  matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`).addEventListener("change", () => schedulePlace());
}
