// The album light on the card edges: a port of DeetsMusic's src/album-light.ts (its
// ALBUM-COLOR.md § The album light; ACTIVITY.md §14.2). A skin opts in with --hook-light, whose
// value names its slider (Press: pressLight, Cyber: cyberLight); the host's value rides in the
// state. This module follows the song and sets the neon roles; the skin's tokens turn them into
// its edges, and Cyber's bolts take the second colour at their own loop seam (scenery.ts).
//
// The wave (the host's Album wave row): a new album's colour turns in around each card,
// clockwise from 12 o'clock. A copy of a card's edges in the new colour sits in a box that turns
// and clips it, and an inner box turns the other way, so the copy stays still. Only `transform`
// moves. At the end the real cards take the roles and the copies go, in one frame. No wave on
// the first song, on a theme or skin change, with reduced motion, or with the frame hidden; a
// card that moves during a wave ends it.

import { readPalette } from "./album-color";
import { albumNeon, isDark, type Neon } from "./album-slots";
import { motionReduced } from "./motion";
import { currentTrack, onPlayerState } from "./player-bus";
import { stormTakeAlbum } from "./scenery";
import { onSettingsChange, setting } from "./skin-settings";

const LIGHT_KEYS = ["pressLight", "cyberLight"] as const;
type LightKey = (typeof LIGHT_KEYS)[number];
const root = document.documentElement;
const MOVERS = ".panel, .stage";
const BOX_PROPS = new Set(["transform", "translate", "scale", "rotate", "opacity", "left", "top", "width", "height"]);
const INSIDE_ONLY = new Set(["is-drill-swap", "is-grow-rows", "is-grow-rebuild", "is-player-rebuild", "is-rows-sliding"]);
const END_SLACK_MS = 400;
const log = (what: string): void => console.info(`[deetsmusic-ui] album light: ${what}`);

function lightKey(): LightKey | null {
  const v = getComputedStyle(root).getPropertyValue("--hook-light").trim();
  return (LIGHT_KEYS as readonly string[]).includes(v) ? (v as LightKey) : null;
}
/** Is the ground under the cards dark? */
function groundIsDark(): boolean {
  const probe = document.createElement("i");
  probe.style.cssText = "position:absolute;visibility:hidden;color:var(--canvas)";
  document.body.appendChild(probe);
  const c = getComputedStyle(probe).color;
  probe.remove();
  const m = c.match(/[\d.]+/g)?.map(Number);
  const rgb = m && m.length >= 3 ? (c.startsWith("color(") ? [m[0], m[1], m[2]] : [m[0] / 255, m[1] / 255, m[2] / 255]) : null;
  return rgb ? isDark(rgb as [number, number, number]) : true;
}
function setNeon(el: HTMLElement, n: Neon | null): void {
  if (!n) {
    ["--album-neon-core", "--album-neon-bloom", "--album-neon-2"].forEach((p) => el.style.removeProperty(p));
    return;
  }
  el.style.setProperty("--album-neon-core", n.core);
  el.style.setProperty("--album-neon-bloom", n.bloom);
  el.style.setProperty("--album-neon-2", n.second);
}

// ── the wave ────────────────────────────────────────────────────────────────────
interface Wave { host: HTMLElement; neon: Neon; timer: number; next: Neon | null; unwatch: () => void }
let wave: Wave | null = null;

function endWave(why: string): void {
  const w = wave;
  if (!w) return;
  wave = null;
  clearTimeout(w.timer);
  w.unwatch();
  const last = w.next ?? w.neon;
  const queued = why === "done" && w.next;
  setNeon(root, queued ? w.neon : last);
  stormTakeAlbum();
  w.host.remove();
  log(`wave ended (${why})`);
  if (queued && !startWave(queued)) {
    setNeon(root, queued);
    stormTakeAlbum();
  }
}

function addWheel(host: HTMLElement, from: HTMLElement, kind: string, ox: number, oy: number, pad: number): HTMLElement {
  const r = from.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const cx = Math.round((r.left + r.width / 2) * dpr) / dpr;
  const cy = Math.round((r.top + r.height / 2) * dpr) / dpr;
  const wheel = document.createElement("div");
  wheel.className = "album-wave__wheel";
  wheel.style.setProperty("--wheel-x", `${cx - ox}px`);
  wheel.style.setProperty("--wheel-y", `${cy - oy}px`);
  wheel.style.setProperty("--wheel-r", `${Math.ceil(Math.hypot(r.width, r.height) / 2 + pad + 2)}px`);
  wheel.style.setProperty("--edge-x", `${r.left - cx}px`);
  wheel.style.setProperty("--edge-y", `${r.top - cy}px`);
  wheel.style.setProperty("--edge-w", `${r.width}px`);
  wheel.style.setProperty("--edge-h", `${r.height}px`);
  let strike!: HTMLElement;
  for (const side of ["right", "left"]) {
    const half = document.createElement("div");
    half.className = `album-wave__half album-wave__half--${side}`;
    strike = document.createElement("div");
    strike.className = "album-wave__strike";
    const hold = document.createElement("div");
    hold.className = "album-wave__hold";
    const copy = document.createElement("div");
    copy.className = kind;
    hold.appendChild(copy);
    strike.appendChild(hold);
    half.appendChild(strike);
    wheel.appendChild(half);
  }
  host.appendChild(wheel);
  return strike;
}

function startWave(neon: Neon): boolean {
  const bento = document.querySelector<HTMLElement>(".bento");
  if (!bento) return false;
  const moving = bento.getAnimations({ subtree: true }).some((a) => {
    const t = (a.effect as KeyframeEffect | null)?.target;
    return t instanceof Element && t.matches(MOVERS);
  });
  if (moving) return false;
  const css = getComputedStyle(root);
  const pad = parseFloat(css.getPropertyValue("--grow-clip-pad")) || 0;
  const box = bento.getBoundingClientRect();
  const ox = box.left - pad;
  const oy = box.top - pad;
  const host = document.createElement("div");
  host.className = "album-wave";
  host.setAttribute("aria-hidden", "true");
  setNeon(host, neon);
  let last: HTMLElement | null = null;
  const shown = (el: HTMLElement): boolean => {
    const cs = getComputedStyle(el);
    return el.getClientRects().length > 0 && cs.visibility !== "hidden" && cs.opacity !== "0";
  };
  bento.querySelectorAll<HTMLElement>(".panel").forEach((el) => {
    if (shown(el)) last = addWheel(host, el, "album-wave__edge", ox, oy, pad);
  });
  if (!last) return false;
  const end: HTMLElement = last;
  bento.querySelectorAll<HTMLElement>(".vinyl__plate").forEach((el) => {
    if (shown(el) && getComputedStyle(el).boxShadow !== "none") addWheel(host, el, "album-wave__plate", ox, oy, pad);
  });
  const moved = (e: Event): void => {
    if (e instanceof TransitionEvent && !BOX_PROPS.has(e.propertyName)) return;
    if (e.target instanceof Element && e.target.matches(MOVERS)) endWave("moved");
  };
  const boxChange = (r: MutationRecord): boolean => {
    if (r.type === "childList") return [...r.addedNodes, ...r.removedNodes].some((n) => n !== host);
    if (r.attributeName !== "class") return true;
    const was = new Set((r.oldValue ?? "").split(/\s+/).filter(Boolean));
    const now = new Set((r.target as Element).classList);
    return [...was, ...now].some((c) => was.has(c) !== now.has(c) && !INSIDE_ONLY.has(c));
  };
  const cardsChanged = new MutationObserver((records) => {
    if (records.some(boxChange)) endWave("moved");
  });
  let measured = false;
  const sized = new ResizeObserver(() => {
    if (measured) endWave("moved");
    measured = true;
  });
  const ms = parseFloat(css.getPropertyValue("--album-wave-dur")) * 1000 || 0;
  wave = {
    host, neon, next: null,
    unwatch: () => {
      bento.removeEventListener("animationstart", moved, true);
      bento.removeEventListener("transitionrun", moved, true);
      cardsChanged.disconnect();
      sized.disconnect();
    },
    timer: window.setTimeout(() => endWave("late"), ms + END_SLACK_MS),
  };
  end.addEventListener("animationend", (e) => {
    if (e.target === end) endWave("done");
  });
  bento.addEventListener("animationstart", moved, true);
  bento.addEventListener("transitionrun", moved, true);
  bento.appendChild(host);
  cardsChanged.observe(bento, { childList: true, attributes: true, attributeFilter: ["data-player"] });
  cardsChanged.observe(root, { attributes: true, attributeFilter: ["data-surface"] });
  sized.observe(bento);
  bento.querySelectorAll<HTMLElement>(".panel").forEach((el) => {
    cardsChanged.observe(el, { attributes: true, attributeFilter: ["class"], attributeOldValue: true });
    sized.observe(el);
  });
  log("wave started");
  return true;
}

// ── following the song ──────────────────────────────────────────────────────────
export function initAlbumLight(): void {
  let liveCover: string | null | undefined;
  let seen = false;
  let asked = 0;
  let pourNext = false;
  let shownNeon: Neon | null = null;

  const show = (n: Neon | null, pour: boolean): void => {
    if (n?.core === shownNeon?.core && n?.second === shownNeon?.second) {
      if (n) stormTakeAlbum();
      return;
    }
    const was = shownNeon;
    shownNeon = n;
    if (wave && n && !pour) {
      if (wave.next) wave.next = n;
      else {
        wave.neon = n;
        setNeon(wave.host, n);
      }
      return;
    }
    if (wave && n) {
      wave.next = n;
      return;
    }
    endWave("next");
    const allowed = pour && !!n && setting("albumWave") && !motionReduced() && !document.hidden && root.dataset.ambient !== "paused";
    if (allowed && startWave(n)) return;
    setNeon(root, n);
    stormTakeAlbum();
    if (!was !== !n) log(n ? "on" : "off");
  };

  const follow = (force = false): void => {
    const t = currentTrack();
    const cover = t?.artworkTemplate ?? null;
    if (!force && cover === liveCover) return;
    pourNext ||= cover !== liveCover && seen;
    liveCover = cover;
    seen ||= !!cover;
    const mine = ++asked;
    const k = lightKey();
    if (!k || setting(k) <= 0 || !t || !cover) {
      pourNext = false;
      show(null, false);
      return;
    }
    void readPalette(t).then((p) => {
      if (mine !== asked) return;
      const pour = pourNext;
      pourNext = false;
      show(p ? albumNeon(p, groundIsDark()) : null, pour);
    });
  };

  onPlayerState(() => follow());
  onSettingsChange((k) => {
    if ((LIGHT_KEYS as readonly string[]).includes(k)) follow(true);
  });
  new MutationObserver((records) => {
    if (records.some((r) => r.attributeName === "data-skin")) endWave("look");
    requestAnimationFrame(() => follow(true));
  }).observe(root, { attributes: true, attributeFilter: ["data-theme", "data-skin"] });
  follow();
}
