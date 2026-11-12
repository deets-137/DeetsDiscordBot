// The Metal skin's moving parts: a port of DeetsMusic's src/metal.ts (its docs/features/metal.md;
// ACTIVITY.md §14.2).
// - The material: the card, page and control textures (metal-material.ts).
// - The peek: in Max, the player view and the full player, the previous and next songs' covers
//   wait behind the cover (.metal-peek), and the zoetrope (vinyl.ts, --cover-swap) turns between
//   them. The slot receiving the old cover shows only as it lands; the slot the new cover left
//   refills during the turn.
// - The words: the title, artist and album leave and arrive with the turn.
// The sides come from the host's History (before) and Up Next (after). Every motion runs on one
// clock (turnMs), and nothing under reduced motion or a hidden frame.

import { artworkURL } from "../deetsmusic-protocol";
import { artPxNow } from "./art-size";
import { initMetalMaterial } from "./metal-material";
import { motionReduced } from "./motion";
import { currentTrack, history, onPlayerState, upNext } from "./player-bus";
import { onSettingsChange } from "./skin-settings";
import { drumBack, drumPerspective } from "./vinyl";

const root = document.documentElement;
const isMetal = (): boolean => root.dataset.hookMaterial === "metal";

// The drum holds pictures, not songs: a side shows the nearest neighbour whose cover differs
// from the one on show, so an album played in order keeps the album before and after.
const PEEK_REACH = 60;
function peekArt(side: "prev" | "next"): string | null {
  const px = artPxNow();
  const cur = currentTrack();
  const now = cur ? artworkURL(cur, px) : null;
  // History is newest first; Up Next is in play order: both start nearest the song.
  const list = side === "next" ? upNext() : history();
  for (let i = 0; i < Math.min(PEEK_REACH, list.length); i++) {
    const src = artworkURL(list[i], px);
    if (src && src !== now) return src;
  }
  return null;
}
const source = { seen: null as string | null, seenAt: 0, wait: 0, end: 0 };
const songNowOf = (): string | null => currentTrack()?.catalogId ?? null;

/** The side covers exist only where they show: Max, Mini's player view (and Wide), the full player. */
const onStage = (): boolean => !("tile" in root.dataset) && (root.dataset.surface === "max" || (root.dataset.surface === "mini" && root.dataset.mini === "player") || !!document.querySelector('.bento[data-player="full"]'));

function turnMs(el: Element): number {
  if (motionReduced() || root.dataset.ambient === "paused" || document.hidden) return 0;
  const cs = getComputedStyle(el);
  const k = parseFloat(cs.getPropertyValue("--motion-scale"));
  return (parseFloat(cs.getPropertyValue("--vinyl-swap-dur")) || 0) * 1000 * (Number.isFinite(k) ? k : 1);
}

// The words cross-fade with the turn, so the text and the picture change together.
const WORDS = "metal-words";
function wireTextFade(): void {
  document.querySelectorAll<HTMLElement>(".np__meta").forEach((meta) => {
    const title = meta.querySelector<HTMLElement>(".np__title");
    if (!title || title.dataset.metalFade !== undefined) return;
    title.dataset.metalFade = "";
    const copy = (): HTMLElement => {
      const c = meta.cloneNode(true) as HTMLElement;
      c.querySelectorAll("[id]").forEach((e) => e.removeAttribute("id"));
      c.removeAttribute("id");
      c.setAttribute("aria-hidden", "true");
      c.inert = true;
      c.dataset.metalGhost = "";
      return c;
    };
    let ghost: HTMLElement | null = isMetal() ? copy() : null;
    let last = title.textContent;
    let shown: HTMLElement | null = null;
    new MutationObserver(() => {
      if (!isMetal()) {
        last = title.textContent;
        ghost = null;
        return;
      }
      if (title.textContent === last) {
        ghost = copy();
        return;
      }
      last = title.textContent;
      const old = ghost;
      ghost = copy();
      if (!old) return;
      const art = meta.closest(".np")?.querySelector<HTMLElement>(".np__art");
      const dur = turnMs(art ?? meta);
      const host = meta.offsetParent as HTMLElement | null;
      shown?.remove();
      shown = null;
      if (!dur || !host) return;
      const card = meta.closest(".np");
      const parts = [meta, ...(card?.querySelectorAll<HTMLElement>(".np__times") ?? [])];
      parts.forEach((e) => e.getAnimations().forEach((x) => x.id === WORDS && x.cancel()));
      Object.assign(old.style, { position: "absolute", left: `${meta.offsetLeft}px`, top: `${meta.offsetTop}px`, width: `${meta.offsetWidth}px`, height: `${meta.offsetHeight}px`, margin: "0", pointerEvents: "none" });
      host.append(old);
      shown = old;
      const t = { duration: dur, easing: "linear", id: WORDS } as const;
      old.animate([{ opacity: 1, translate: "0 0", easing: "cubic-bezier(0.4, 0, 1, 1)" }, { opacity: 0, translate: "-10px 0", offset: 0.28 }, { opacity: 0, translate: "-10px 0" }], t)
        .finished.then(() => {
          old.remove();
          if (shown === old) shown = null;
        }, () => old.remove());
      meta.animate([{ opacity: 0, translate: "10px 0" }, { opacity: 0, translate: "10px 0", offset: 0.32, easing: "cubic-bezier(0, 0, 0.2, 1)" }, { opacity: 1, translate: "0 0", offset: 0.62 }, { opacity: 1, translate: "0 0" }], t);
      parts.slice(1).forEach((e) => e.animate([{ opacity: 0 }, { opacity: 0, offset: 0.32 }, { opacity: 1, offset: 0.62 }, { opacity: 1 }], t));
    }).observe(meta, { characterData: true, childList: true, subtree: true });
  });
}

// A neighbour's picture is fetched and decoded as soon as it is known, so a side cover that
// comes forward is ready. A side shows its picture only once decoded.
const ready = new Map<string, boolean>();
function warm(src: string | null): void {
  if (!src || ready.has(src)) return;
  ready.set(src, false);
  if (ready.size > 40) ready.delete(ready.keys().next().value as string);
  const im = new Image();
  im.decoding = "async";
  im.src = src;
  im.decode().then(() => {
    ready.set(src, true);
    peekSync();
  }, () => ready.delete(src));
}
const isReady = (src: string | null): src is string => !!src && ready.get(src) === true;
/** No neighbour: a blank sleeve (the brushed plate); a picture on its way stays hidden. */
function paint(peek: HTMLElement, src: string | null): boolean {
  peek.style.removeProperty("--peek-art");
  delete peek.dataset.blank;
  delete peek.dataset.empty;
  if (src === null) {
    peek.dataset.blank = "";
    return true;
  }
  if (isReady(src)) {
    peek.style.setProperty("--peek-art", `url("${src}")`);
    return true;
  }
  peek.dataset.empty = "";
  return false;
}

const giving = new Map<HTMLElement, () => void>();
const drumSized = new WeakSet<HTMLElement>();
const drumWatch = new ResizeObserver((entries) => entries.forEach((e) => sizeDrum(e.target as HTMLElement)));
/** Set a cover box's drum depth from its side slots' width (also the full player's flight copy). */
export function sizeDrum(box: HTMLElement): void {
  const w = box.querySelector<HTMLElement>(":scope > .metal-peek")?.offsetWidth ?? 0;
  if (!w) return;
  const cs = getComputedStyle(box);
  const tok = (n: string, d: number): number => {
    const v = parseFloat(cs.getPropertyValue(n));
    return Number.isFinite(v) ? v : d;
  };
  const P = drumPerspective(tok("--zoetrope-perspective", 900), tok("--zoetrope-perspective-per-cover", 0), w);
  box.style.setProperty("--metal-perspective", `${P}px`);
  box.style.setProperty("--metal-drum-back", `${drumBack(P, tok("--zoetrope-scale", 0.86))}px`);
}
function watchDrum(box: HTMLElement, on: boolean): void {
  if (on && !drumSized.has(box)) {
    drumSized.add(box);
    drumWatch.observe(box);
  } else if (!on && drumSized.has(box)) {
    drumSized.delete(box);
    drumWatch.unobserve(box);
    box.style.removeProperty("--metal-perspective");
    box.style.removeProperty("--metal-drum-back");
  }
}

function peekSync(): void {
  wireTextFade();
  const on = root.dataset.metalPeek !== undefined && isMetal() && onStage();
  const songNow = songNowOf();
  if (songNow !== source.seen) {
    source.seen = songNow;
    source.seenAt = performance.now();
  }
  document.querySelectorAll<HTMLElement>(".bento .np__art").forEach((box) => {
    watchDrum(box, on);
    const dur = turnMs(box);
    const known = songNow !== null && box.dataset.turnSong === songNow;
    if (on && dur && songNow !== null && !known && performance.now() - source.seenAt < dur) {
      window.clearTimeout(source.wait);
      source.wait = window.setTimeout(peekSync, dur + 50);
      return;
    }
    const kind = known ? box.dataset.turn : "none";
    const turnAt = Number(box.dataset.turnAt ?? 0);
    const turning = dur > 0 && (kind === "fwd" || kind === "back") && performance.now() - turnAt < dur + 150;
    const landing: "prev" | "next" = kind === "back" ? "next" : "prev";
    for (const side of ["prev", "next"] as const) {
      let el = box.querySelector<HTMLElement>(`:scope > .metal-peek[data-side="${side}"]`);
      if (!on) {
        if (el) giving.delete(el);
        el?.remove();
        continue;
      }
      if (!el) {
        el = document.createElement("div");
        el.className = "metal-peek";
        el.dataset.side = side;
        el.setAttribute("aria-hidden", "true");
        box.prepend(el);
        if (side === "next") sizeDrum(box);
      }
      const peek = el;
      const src = peekArt(side);
      warm(src);
      const receives = turning && side === landing;
      if (receives) {
        peek.dataset.receiving = "";
        window.clearTimeout(source.end);
        source.end = window.setTimeout(peekSync, Math.max(0, turnAt + dur + 160 - performance.now()));
      } else delete peek.dataset.receiving;
      if ((peek.dataset.src ?? "") === (src ?? "")) {
        window.clearTimeout(Number(peek.dataset.timer ?? 0));
        giving.delete(peek);
        delete peek.dataset.want;
        peek.getAnimations().forEach((a) => a.cancel());
        if (peek.dataset.empty !== undefined && paint(peek, src) && !receives) peek.animate([{ opacity: 0 }, { opacity: 1 }], { duration: Math.min(160, dur || 160), easing: "ease-out" });
        continue;
      }
      if (peek.dataset.want !== undefined && peek.dataset.want === (src ?? "")) continue;
      peek.dataset.want = src ?? "";
      const still = !turning || peek.dataset.src === undefined;
      const take = (): void => {
        giving.delete(peek);
        window.clearTimeout(Number(peek.dataset.timer ?? 0));
        peek.dataset.src = src ?? "";
        delete peek.dataset.want;
        const shows = paint(peek, src);
        peek.getAnimations().forEach((a) => a.cancel());
        if (!still && !receives && shows) {
          const edge = side === "next" ? "60% 0" : "-60% 0";
          peek.animate([{ translate: edge }, { translate: "0 0" }], { duration: dur * 0.5, easing: "cubic-bezier(0.2, 0.8, 0.3, 1)" });
        }
      };
      window.clearTimeout(Number(peek.dataset.timer ?? 0));
      giving.delete(peek);
      const showing = peek.dataset.empty === undefined && peek.dataset.src !== undefined && peek.dataset.src !== "";
      const cs = getComputedStyle(box);
      const tok = (n: string, d: number): number => {
        const v = parseFloat(cs.getPropertyValue(n));
        return Number.isFinite(v) ? v : d;
      };
      if (receives && !still && showing) {
        // The cover leaving this slot is pushed off the drum, outward, while the old one arrives.
        const w = peek.offsetWidth;
        const k = side === "next" ? 1 : -1;
        const base = tok("--zoetrope-perspective", 900);
        const P = drumPerspective(base, tok("--zoetrope-perspective-per-cover", 0), w);
        const back = drumBack(P, tok("--zoetrope-scale", 0.86));
        const run = tok("--zoetrope-run", 0.26);
        const turn = tok("--zoetrope-turn", 30);
        const pose = (r: number, z: number, deg: number): string => `perspective(${P}px) translateX(${k * r * w}px) translateZ(${z}px) rotateY(${k * deg}deg)`;
        peek.getAnimations().forEach((a) => a.cancel());
        peek.animate(
          [{ transform: pose(run, back, turn), opacity: 1 }, { opacity: 1, offset: 0.6 }, { transform: pose(run + 0.32, back - (140 * P) / base, turn + 45), opacity: 0 }],
          { duration: dur * 0.35, easing: "cubic-bezier(0.4, 0, 0.9, 0.6)", fill: "forwards" },
        );
        peek.dataset.timer = String(window.setTimeout(take, Math.max(0, turnAt + dur * 0.35 - performance.now())));
      } else if (still || receives) take();
      else {
        // GIVING: the slot changes the moment the arriving cover is opaque in front of it.
        const stagger = tok("--zoetrope-stagger", 0);
        const rise = tok("--zoetrope-rise", 0.35);
        const coveredAt = turnAt + dur * (stagger + (1 - stagger) * rise);
        peek.getAnimations().forEach((a) => a.cancel());
        giving.set(peek, take);
        peek.dataset.timer = String(window.setTimeout(take, Math.max(0, coveredAt + 250 - performance.now())));
      }
    }
  });
}

export function initMetal(): void {
  initMetalMaterial();
  peekSync();
  new MutationObserver(() => peekSync()).observe(root, { attributes: true, attributeFilter: ["data-hook-material", "data-skin"] });
  onSettingsChange((k) => {
    if (k === "metalPeek" || k === "metalCoverChange") queueMicrotask(peekSync);
  });
  onPlayerState(() => peekSync());
  document.addEventListener("vinyl:turn", () => peekSync());
  document.addEventListener("vinyl:covered", () => [...giving.values()].forEach((take) => take()));
  const later = (): void => {
    peekSync();
    window.setTimeout(peekSync, 120);
    window.setTimeout(peekSync, 600);
  };
  new MutationObserver(later).observe(root, { attributes: true, attributeFilter: ["data-surface", "data-mini", "data-metal-peek", "data-tile"] });
  // A cover box that appears without its side covers (a card mounted, the full player).
  let looking = false;
  new MutationObserver(() => {
    if (looking || root.dataset.metalPeek === undefined || !isMetal() || !onStage()) return;
    looking = true;
    requestAnimationFrame(() => {
      looking = false;
      if ([...document.querySelectorAll(".bento .np__art")].some((b) => !b.querySelector(":scope > .metal-peek"))) peekSync();
    });
  }).observe(document.body, { childList: true, subtree: true });
}
