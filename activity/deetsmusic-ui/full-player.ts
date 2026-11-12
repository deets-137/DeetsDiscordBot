// The full player: Now Playing grows so the cover is as large as the frame allows. A port of
// DeetsMusic's src/full-player.ts and src/full-player-layout.ts (fullscreen-player.md, shipped
// 0.26.1); ACTIVITY.md §14.1.
//
//   Midi  the card takes both rows, over the two cards. Row 1 keeps the strip's height while
//         the cards fade, so they do not jump.
//   Max   the card takes the content columns, over Search and History; the stage card under it
//         (the Queue, or History after a swap) takes the whole stage column. A side column
//         shows the other of the two beside the cover when the card is wide enough that the
//         cover keeps its size.
//   Stacked  a frame taller than wide, with room under a full-width cover: the player across
//         the top, the cards in the band under it.
// In each, the controls go under the cover when there is room or the card is wide, else over
// its bottom on the dark band (`rowsPlace`). The look's rules do the layout from the
// attributes written here (`data-player`, `data-np-rows`, `data-fp-history`, `data-fp-stack`).
//
// Ways in (his call 2A, 2026-11-11): the edge strips on Now Playing (Midi: top and bottom;
// Max: all four sides), and a tap on the cover. Ways out: the close square over the cover, the
// same strips, Escape, a request for a covered card, a surface change (no motion).
// Each person's own frame: nothing here crosses the relay.
//
// Left out of the app's: the Queue's side setting and its handle (the Activity has no
// settings), the side column's own picker, the rules engine and the Compass.

import { fullArtPx, restArtPx, setFullArtPx } from "./art-size";
import { collapseGrow, grownState, type Slot } from "./grow";
import { clipBetween, clipOpen, clipPad, cover, enterRows, motionReduced, pxToken, rectOf as rect, tokenMs, uncover } from "./motion";
import { onSurface, surfaceState } from "./surface";

interface Opts {
  hosts: Record<Slot, HTMLElement>;
  bento: HTMLElement;
  /** The side column's host (`.panel.fp-history`), and the way to fill it. */
  side: HTMLElement;
  mountSide(host: HTMLElement): () => void;
  /** The content slots the player covers (Search and History in Max; Midi's two). */
  slots(): Slot[];
  /** A hook for a skin that sizes something to the cover's new box (Metal's drum). */
  onFlight?(fly: HTMLElement): void;
}

type RowsPlace = "over" | "below";
interface Box { left: number; top: number; right: number; bottom: number }
interface Motion { dur: number; ease: string; rowsAt: number; collapseDur: number }
interface Layout { place: RowsPlace; history: boolean; stack: boolean; stackH: number }

let opts: Opts | null = null;
let full = false;
let busy = false;
let gen = 0;
let artPx = 0;
let watch: ResizeObserver | null = null;
let measuredShare = 1;
let wantHistory = false;
let resizedInMotion = false;
let openedOn: "midi" | "max" | null = null;
let stack = false;
let slideGen = 0;
const subs = new Set<(on: boolean) => void>();

const root = document.documentElement;
const fp = (): HTMLElement => opts?.bento ?? root;
const surfaceNow = (): "midi" | "max" | null => {
  const s = surfaceState();
  if (s.tile || s.wide) return null;
  return s.surface === "midi" || s.surface === "max" ? s.surface : null;
};
const log = (what: string): void => console.info(`[deetsmusic-ui] player: ${what}`);

export const isFullPlayer = (): boolean => full;
export function onFullPlayerChange(cb: (on: boolean) => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}
/** It can open now: the frame is Midi or Max (wider than 460 px; the tile and Wide never). */
export const fullPlayerAvailable = (): boolean => !!opts && surfaceNow() !== null;

function motion(): Motion | null {
  if (motionReduced()) return null;
  const dur = tokenMs("--player-dur");
  if (dur <= 0) return null;
  const cs = getComputedStyle(root);
  const ease = cs.getPropertyValue("--player-ease").trim() || "ease";
  const at = parseFloat(cs.getPropertyValue("--player-rows-at"));
  const rowsAt = Number.isFinite(at) ? Math.min(1, Math.max(0, at)) * dur : dur;
  return { dur, ease, rowsAt, collapseDur: tokenMs("--player-collapse-dur") || dur };
}
const coverShare = (): number => pxToken("--fp-cover-share") || 1;
const metaBelow = (): boolean => pxToken("--fp-meta-below") > 0;

// ── the pure rules (DeetsMusic's full-player-layout.ts) ─────────────────────────
function coverSide(w: number, h: number, share = 1): number {
  return Math.max(0, Math.min(w * share, h));
}
function rowsPlace(cardW: number, cardH: number, wideW: number, controlsH: number, gap: number, share: number, below: boolean): RowsPlace {
  if (below || cardW - cardH >= wideW) return "below";
  return cardH - coverSide(cardW, cardH, share) >= controlsH + gap ? "below" : "over";
}
function fullClipFrom(rest: Box, to: Box): Box {
  return { left: Math.max(rest.left, to.left), right: Math.min(rest.right, to.right), top: Math.max(rest.top, to.top), bottom: Math.min(rest.bottom, to.bottom) };
}
function flightFrom(from: Box, to: Box): { dx: number; dy: number; scale: number } {
  const toW = to.right - to.left;
  return { dx: from.left - to.left, dy: from.top - to.top, scale: toW > 0 ? (from.right - from.left) / toW : 1 };
}

// ── the parts ───────────────────────────────────────────────────────────────────
const npHost = (): HTMLElement | null => opts?.hosts.np ?? null;
const stageHost = (): HTMLElement | null => (surfaceNow() === "max" ? opts?.hosts.queue ?? null : null);
const artOf = (panel: HTMLElement): HTMLElement | null => panel.querySelector<HTMLElement>(".np__art");
const npOf = (panel: HTMLElement): HTMLElement | null => panel.querySelector<HTMLElement>(".np");
const contentHosts = (): HTMLElement[] => (opts?.slots() ?? []).map((s) => opts!.hosts[s]);
const coveredHosts = (): HTMLElement[] => (stack && (full ? openedOn : surfaceNow()) === "midi" ? [] : contentHosts());
const historyRoom = (): number => (fp().dataset.fpHistory === "on" && !stack ? pxToken("--full-history-w") + pxToken("--panel-gap") : 0);

// ── stacked (a frame taller than wide) ──────────────────────────────────────────
function stackNow(panel: HTMLElement, controlsH: number, gap: number): { on: boolean; h: number } {
  const bento = opts?.bento;
  if (!bento || !surfaceNow() || !Number.isFinite(controlsH)) return { on: false, h: 0 };
  const b = rect(bento);
  const cs = getComputedStyle(panel);
  const px = (v: string): number => parseFloat(v) || 0;
  const boxW = px(cs.paddingLeft) + px(cs.paddingRight) + px(cs.borderLeftWidth) + px(cs.borderRightWidth);
  const boxH = px(cs.paddingTop) + px(cs.paddingBottom) + px(cs.borderTopWidth) + px(cs.borderBottomWidth);
  const h = Math.ceil(coverSide(b.width - boxW, Infinity, measuredShare) + gap + controlsH + boxH);
  const band = pxToken("--full-stack-band");
  const tall = stack ? window.innerHeight > window.innerWidth - band : window.innerHeight > window.innerWidth + band;
  return { on: tall && b.height - h - pxToken("--panel-gap") >= pxToken("--full-stack-room"), h };
}
function setStack(on: boolean, h: number): void {
  if (h > 0) opts?.bento.style.setProperty("--fp-stack-h", `${h}px`);
  if (on === stack) return;
  stack = on;
  fp().toggleAttribute("data-fp-stack", on);
  log(`stacked ${on ? "on" : "off"}`);
  if (surfaceNow() === "midi") contentHosts().forEach((c) => (on ? uncover(c) : cover(c)));
  paintZones();
}

/** Where everything goes in the card's full box. Called with the full layout on and no paint
 *  between: the controls are measured on the band; then stacked, the rows' place, the column. */
function layoutNow(panel: HTMLElement): Layout {
  const np = npOf(panel);
  if (!np) return { place: "over", history: false, stack: false, stackH: 0 };
  measuredShare = coverShare();
  const below = metaBelow();
  panel.toggleAttribute("data-np-meta-below", below);
  const r = rect(np);
  const w = r.width + historyRoom();
  const gap = parseFloat(getComputedStyle(np).rowGap) || 0;
  const was = panel.dataset.npRows;
  panel.dataset.npRows = "over";
  const scrub = panel.querySelector<HTMLElement>(".np__scrub");
  const last = panel.querySelector<HTMLElement>(".np__bottom");
  const meta = panel.querySelector<HTMLElement>(".np__meta");
  const metaH = below && meta ? rect(meta).height + gap : 0;
  const h = scrub && last ? rect(last).bottom - rect(scrub).top + metaH : Infinity;
  if (was) panel.dataset.npRows = was;
  else delete panel.dataset.npRows;
  if (Number.isFinite(h)) panel.style.setProperty("--fp-controls-h", `${h}px`);
  const st = stackNow(panel, h, gap);
  if (st.on) return { place: "below", history: surfaceNow() === "max", stack: true, stackH: st.h };
  const share = measuredShare;
  const place = rowsPlace(w, r.height, pxToken("--full-wide-w"), h, gap, share, below);
  const side = coverSide(w, place === "below" ? r.height - h - gap : r.height, share);
  const history = surfaceNow() === "max" && coverSide(w - pxToken("--full-history-w") - pxToken("--panel-gap"), Infinity, share) >= side;
  return { place, history, stack: false, stackH: st.h };
}

// ── the side column (Max) ───────────────────────────────────────────────────────
let sideUndo: (() => void) | null = null;
let historyGen = 0;
const awayX = (): string => `translateX(calc(100% + ${pxToken("--panel-gap")}px))`;
function mountSide(): void {
  if (sideUndo || !opts) return;
  sideUndo = opts.mountSide(opts.side);
}
function dropHistory(keep = true): void {
  historyGen++;
  opts?.side.getAnimations().forEach((a) => a.cancel());
  delete fp().dataset.fpHistory;
  if (keep) return;
  sideUndo?.();
  sideUndo = null;
}
function setHistory(on: boolean, m: Motion | null): (() => void) | null {
  const host = opts?.side;
  if (!host) return null;
  const my = ++historyGen;
  host.getAnimations().forEach((a) => a.cancel());
  if (on) {
    fp().dataset.fpHistory = "on";
    mountSide();
    paintZones();
    log("side column in");
    return m ? () => void host.animate([{ transform: awayX() }, { transform: "none" }], { duration: m.dur, easing: m.ease }) : null;
  }
  log("side column out");
  if (!m) {
    delete fp().dataset.fpHistory;
    paintZones();
    return null;
  }
  fp().dataset.fpHistory = "leaving";
  paintZones();
  return () => {
    const anim = host.animate([{ transform: "none" }, { transform: awayX() }], { duration: m.dur, easing: m.ease, fill: "forwards" });
    void anim.finished.then(() => {
      if (my !== historyGen) return;
      delete fp().dataset.fpHistory;
      anim.cancel();
    }, () => {});
  };
}

/** A resize moved the rows between the band and the space under the cover: each part slides
 *  from its old place to its new one, and the cover moves and scales to its new box. */
function slideRows(panel: HTMLElement, next: Layout): void {
  const from = panel.dataset.npRows;
  const parts = [...panel.querySelectorAll<HTMLElement>(".np__art, .np__meta, .np__scrub, .np__times, .np__bottom")];
  parts.forEach((p) => p.getAnimations().forEach((a) => a.id === "rows-slide" && a.cancel()));
  const m = motion();
  const before = m ? parts.map(rect) : [];
  const restack = next.stack !== stack;
  const columns = restack ? [stageHost(), opts?.side].filter((p): p is HTMLElement => !!p && !!p.offsetParent) : [];
  const colsBefore = m ? columns.map(rect) : [];
  setStack(next.stack, next.stackH);
  if (next.place !== from) {
    panel.dataset.npRows = next.place;
    log(`rows ${from ?? ""} → ${next.place}`);
  }
  const column = next.history !== (fp().dataset.fpHistory === "on") ? setHistory(next.history, m) : null;
  if (!m) return;
  column?.();
  columns.forEach((c, i) => {
    const a = colsBefore[i];
    const b = rect(c);
    const dx = a.left - b.left;
    const dy = a.top - b.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
    c.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: m.dur, easing: m.ease }).id = "rows-slide";
  });
  panel.classList.add("is-rows-sliding");
  const my = ++slideGen;
  const anims: Animation[] = [];
  parts.forEach((p, i) => {
    const a = before[i];
    const b = rect(p);
    if (!a.width || !b.width) return;
    const s = a.width / b.width;
    const dx = a.left - b.left;
    const dy = a.top - b.top;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1 && Math.abs(s - 1) < 0.01) return;
    const start = p.classList.contains("np__art") ? `translate(${dx}px, ${dy}px) scale(${s})` : `translate(${dx}px, ${dy}px)`;
    const anim = p.animate([{ transform: start, transformOrigin: "0 0" }, { transform: "none", transformOrigin: "0 0" }], { duration: m.dur, easing: m.ease });
    anim.id = "rows-slide";
    anims.push(anim);
  });
  void Promise.allSettled(anims.map((a) => a.finished)).then(() => {
    if (my === slideGen) panel.classList.remove("is-rows-sliding");
  });
}

function apply(on: boolean, place?: RowsPlace, row1?: number): void {
  const panel = npHost();
  if (on) {
    if (row1 !== undefined) opts?.bento.style.setProperty("--player-row1", `${row1}px`);
    fp().dataset.player = "full";
    fp().toggleAttribute("data-fp-stack", stack);
    if (panel && place) panel.dataset.npRows = place;
    if (panel) panel.toggleAttribute("data-np-meta-below", metaBelow());
  } else {
    delete fp().dataset.player;
    fp().removeAttribute("data-fp-stack");
    if (panel) {
      delete panel.dataset.npRows;
      panel.removeAttribute("data-np-meta-below");
    }
    opts?.bento.style.removeProperty("--player-row1");
  }
}

/** Apple's image server takes any `{w}x{h}` in the path. */
function sizedUrl(src: string, px: number): string {
  return src.replace(/\/(\d+)x(\d+)([a-z]*)\.(jpe?g|png|webp)(?=$|\?)/i, `/${px}x${px}$3.$4`);
}
function preload(src: string, ms: number): { ready: boolean; wait: Promise<void> } {
  const img = new Image();
  img.decoding = "async";
  img.src = src;
  if (img.complete && img.naturalWidth > 0) return { ready: true, wait: Promise.resolve() };
  return { ready: false, wait: Promise.race([img.decode().catch(() => {}), new Promise<void>((r) => window.setTimeout(r, ms))]) };
}
function askArt(boxPx: number): void {
  const px = fullArtPx(boxPx, window.devicePixelRatio, restArtPx());
  if (px === artPx) return;
  artPx = px;
  setFullArtPx(px);
}

/** The copy of the cover that flies: the art box, cloned, laid out at `at`. A turning Press
 *  record flies turning, from the angle it shows now, at the real disc's rate. */
function makeFlight(art: HTMLElement, at: DOMRect, src: string | null): HTMLElement | null {
  const real = art.querySelector<HTMLElement>(".vinyl:not(.vinyl--out)");
  if (!real || real.classList.contains("vinyl--glyph")) return null;
  const fly = art.cloneNode(false) as HTMLElement;
  fly.removeAttribute("id");
  fly.classList.add("np__fly");
  const disc = real.cloneNode(true) as HTMLElement;
  const spin = real.querySelector<HTMLElement>(".vinyl__spin");
  const still = disc.querySelector<HTMLElement>(".vinyl__spin");
  if (spin && still) {
    const turning = spin.getAnimations()[0];
    const timing = turning?.effect?.getTiming();
    if (turning && timing && turning.currentTime !== null) {
      const copy = still.animate([{ transform: "rotate(0turn)" }, { transform: "rotate(1turn)" }], {
        duration: timing.duration, iterations: Infinity, easing: timing.easing ?? "linear",
      });
      copy.currentTime = turning.currentTime;
      copy.playbackRate = turning.playbackRate;
      if (turning.playState !== "running") copy.pause();
    } else {
      still.style.transform = getComputedStyle(spin).transform;
    }
  }
  const img = disc.querySelector<HTMLImageElement>("img");
  if (img && src) img.src = src;
  art.querySelectorAll<HTMLElement>(":scope > .metal-peek").forEach((p) => fly.appendChild(p.cloneNode(false)));
  fly.appendChild(disc);
  fly.style.setProperty("--vinyl", getComputedStyle(art).getPropertyValue("--vinyl").trim() || "0");
  fly.style.left = `${at.left}px`;
  fly.style.top = `${at.top}px`;
  fly.style.width = `${at.width}px`;
  fly.style.height = `${at.height}px`;
  document.body.appendChild(fly);
  opts?.onFlight?.(fly);
  return fly;
}
function flyAnim(fly: HTMLElement, from: Box, to: Box, dur: number, ease: string, reverse: boolean): Animation {
  const f = flightFrom(from, to);
  const small = `translate(${f.dx}px, ${f.dy}px) scale(${f.scale})`;
  const frames = reverse ? [{ transform: "none" }, { transform: small }] : [{ transform: small }, { transform: "none" }];
  return fly.animate(frames, { duration: dur, easing: ease, fill: "both" });
}

// ── Max: the resting place's still, the moving card box, the held cards ────────
function bentoOrigin(): { left: number; top: number } {
  const bento = opts!.bento;
  const b = rect(bento);
  return { left: b.left + bento.clientLeft, top: b.top + bento.clientTop };
}
function bentoBox(r: DOMRect, o: { left: number; top: number }): { left: number; top: number; width: number; height: number } {
  return { left: r.left - o.left, top: r.top - o.top, width: r.width, height: r.height };
}
function place(el: HTMLElement, r: DOMRect): void {
  const box = bentoBox(r, bentoOrigin());
  el.style.left = `${box.left}px`;
  el.style.top = `${box.top}px`;
  el.style.width = `${box.width}px`;
  el.style.height = `${box.height}px`;
}

/** A still copy of Now Playing at rest, in its resting box under the stage card (Max). */
function restCopy(panel: HTMLElement): HTMLElement | null {
  if (!opts || surfaceNow() !== "max") return null;
  const copy = panel.cloneNode(true) as HTMLElement;
  copy.removeAttribute("data-slot");
  copy.removeAttribute("data-np-rows");
  copy.removeAttribute("id");
  copy.classList.remove("is-player-rebuild", "is-rows-sliding");
  copy.classList.add("np-still");
  copy.querySelectorAll("[id]").forEach((el) => el.removeAttribute("id"));
  // Happy at the cover (pet.ts) is not copied: a frozen dog would stand in the still.
  copy.querySelectorAll(".grow-zone, .np__close, .pet-push").forEach((el) => el.remove());
  copy.inert = true;
  copy.setAttribute("aria-hidden", "true");
  place(copy, rect(panel));
  return copy;
}
let still: HTMLElement | null = null;
function placeStill(copy: HTMLElement | null): void {
  dropStill();
  if (!copy || !opts) return;
  still = copy;
  opts.bento.appendChild(copy);
  opts.hosts.queue.style.setProperty("z-index", "1");
}
function dropStill(): void {
  if (!still) return;
  still.remove();
  still = null;
  opts?.hosts.queue.style.removeProperty("z-index");
}

const pinned = new Set<HTMLElement>();
function pin(els: HTMLElement[], boxes: DOMRect[]): void {
  if (!opts) return;
  const b = bentoOrigin();
  els.forEach((el, i) => {
    const box = bentoBox(boxes[i], b);
    el.classList.add("fp-pin");
    el.style.left = `${box.left}px`;
    el.style.top = `${box.top}px`;
    el.style.width = `${box.width}px`;
    el.style.height = `${box.height}px`;
    pinned.add(el);
  });
}
function unpin(els: Iterable<HTMLElement> = [...pinned]): void {
  for (const el of els) {
    el.classList.remove("fp-pin");
    for (const k of ["left", "top", "width", "height"]) el.style.removeProperty(k);
    pinned.delete(el);
  }
}

/** The moving card box (Max, fullscreen-player.md §14.10): an empty card that moves and changes
 *  size between the resting box and the full one while the real card waits unseen. The album
 *  glow rides along, centred on the flying cover. */
let ghost: HTMLElement | null = null;
function dropGhost(): void {
  ghost?.remove();
  ghost = null;
}
function ghostMotion(panel: HTMLElement, a: DOMRect, b: DOMRect, artA: DOMRect, artB: DOMRect, rest: DOMRect, dur: number, ease: string): Animation[] {
  dropGhost();
  if (!opts) return [];
  const g = document.createElement("div");
  g.className = "panel fp-ghost";
  g.inert = true;
  g.setAttribute("aria-hidden", "true");
  const np = panel.querySelector<HTMLElement>(".np");
  const aurora = np?.querySelector<HTMLElement>(":scope > .np__aurora");
  if (np && aurora) {
    g.style.cssText = np.style.cssText;
    g.appendChild(aurora.cloneNode(false));
  }
  place(g, a);
  opts.bento.appendChild(g);
  ghost = g;
  const o = bentoOrigin();
  const at = (r: DOMRect): Keyframe => {
    const x = bentoBox(r, o);
    return { left: `${x.left}px`, top: `${x.top}px`, width: `${x.width}px`, height: `${x.height}px` };
  };
  const timing = { duration: dur, easing: ease, fill: "both" as const };
  const anims = [g.animate([at(a), at(b)], timing), panel.animate([{ opacity: 0 }, { opacity: 0 }], timing)];
  const glow = g.querySelector<HTMLElement>(".np__aurora");
  if (glow) {
    const reach = parseFloat(getComputedStyle(glow).getPropertyValue("--album-aurora-reach")) || 1;
    const gs = getComputedStyle(g);
    const edge = parseFloat(gs.borderLeftWidth) || 0;
    const radius = Math.max(0, (parseFloat(gs.borderTopLeftRadius) || 0) - edge);
    const halo = (box: DOMRect, art: DOMRect): Keyframe => {
      const side = art.width * reach;
      const x = art.left + art.width / 2 - box.left - edge - side / 2;
      const y = art.top + art.height / 2 - box.top - edge - side / 2;
      const w = box.width - 2 * edge;
      const h = box.height - 2 * edge;
      return { left: `${x}px`, top: `${y}px`, width: `${side}px`, height: `${side}px`, clipPath: `inset(${-y}px ${x + side - w}px ${y + side - h}px ${-x}px round ${radius}px)` };
    };
    anims.push(glow.animate([halo(a, artA), halo(b, artB)], timing));
  }
  if (still) {
    still.classList.add("is-bare");
    const pad = clipPad();
    const cut = (r: DOMRect): string => `inset(${-pad}px ${rest.right - r.left}px ${-pad}px ${-pad}px)`;
    anims.push(still.animate([{ clipPath: cut(a) }, { clipPath: cut(b) }], timing));
  }
  return anims;
}

/** Max: the stage card between its resting box (under Now Playing) and the whole column. It
 *  moves with its top edge; a clip holds its bottom edge where the column ends. `back`: close. */
const QUEUE_DY = "--fp-queue-dy";
let dyRegistered = false;
function queueRise(queue: HTMLElement, rest: DOMRect, whole: DOMRect, pad: number, dur: number, ease: string, back: boolean): Animation[] {
  const dy = rest.top - whole.top;
  const r = Math.min(parseFloat(getComputedStyle(queue).borderBottomLeftRadius) || 0, whole.width / 2);
  const w = whole.width;
  const s = still?.getBoundingClientRect();
  const shape = (cut: number): string =>
    `path("M ${-pad} ${-pad} H ${w + pad} V ${cut - r} H ${w} A ${r} ${r} 0 0 1 ${w - r} ${cut} H ${r} A ${r} ${r} 0 0 1 0 ${cut - r} H ${-pad} Z")`;
  if (!dyRegistered) {
    dyRegistered = true;
    try {
      CSS.registerProperty({ name: QUEUE_DY, syntax: "<length>", inherits: false, initialValue: "0px" });
    } catch {
      /* registered already */
    }
  }
  const low = { [QUEUE_DY]: `${dy}px`, clipPath: shape(whole.height - dy - (whole.bottom - rest.bottom)) };
  const up = { [QUEUE_DY]: "0px", clipPath: shape(whole.height) };
  queue.style.transform = `translateY(var(${QUEUE_DY}))`;
  const a = queue.animate(back ? [up, low] : [low, up], { duration: dur, easing: ease, fill: "both" });
  a.id = "queue-rise";
  const anims = [a];
  const list = queue.querySelector<HTMLElement>(".qcard__list, .panel__body");
  if (list) {
    const at = (cardH: number): string => `${Math.max(0, list.offsetHeight - (queue.offsetHeight - cardH))}px`;
    const short = { maxHeight: at(rest.height) };
    const tall = { maxHeight: at(whole.height) };
    const b = list.animate(back ? [tall, short] : [short, tall], { duration: dur, easing: ease, fill: "both" });
    b.id = "queue-rise";
    anims.push(b);
  }
  if (still && s) {
    const cut = (top: number): string => {
      const y = top - s.top;
      return `polygon(${-pad}px ${-pad}px, ${s.width + pad}px ${-pad}px, ${s.width + pad}px ${y}px, ${-pad}px ${y}px)`;
    };
    const c = still.animate(back ? [{ clipPath: cut(whole.top) }, { clipPath: cut(rest.top) }] : [{ clipPath: cut(rest.top) }, { clipPath: cut(whole.top) }], { duration: dur, easing: ease, fill: "both" });
    c.id = "queue-rise";
    anims.push(c);
  }
  a.addEventListener("cancel", () => {
    anims.forEach((x) => x !== a && x.cancel());
    if (!queue.getAnimations().some((x) => x.id === "queue-rise" && x !== a)) queue.style.removeProperty("transform");
  });
  return anims;
}
function endQueueRise(queue: HTMLElement | null): void {
  if (!queue) return;
  queue.getAnimations({ subtree: true }).forEach((a) => a.id === "queue-rise" && a.cancel());
  queue.style.removeProperty("transform");
}

/** The rows on the band (and the band) come in. */
function enterBand(panel: HTMLElement): void {
  const center = panel.querySelector<HTMLElement>(".np__center");
  panel.classList.remove("is-player-rebuild");
  if (!center) return;
  const below = fp().dataset.player === "full" && panel.dataset.npRows === "below";
  const dur = motionReduced() ? 0 : tokenMs("--grow-body-fade");
  if (dur > 0) {
    const easing = getComputedStyle(panel).getPropertyValue("--ease-ui").trim() || "ease";
    const close = panel.querySelector<HTMLElement>(".np__close");
    if (close?.offsetParent) close.animate([{ opacity: 0, offset: 0 }], { duration: dur, easing });
    (below ? [...center.children] : [center]).forEach((el) => el.animate([{ opacity: 0, offset: 0 }], { duration: dur, easing }));
  }
  const rows = [...center.children].filter((el) => (el as HTMLElement).offsetParent !== null);
  enterRows(rows, rows.length);
}

function emit(): void {
  subs.forEach((cb) => cb(full));
  paintZones();
}

// ── open ────────────────────────────────────────────────────────────────────────
export async function openFullPlayer(cause: string): Promise<void> {
  if (full || busy || !fullPlayerAvailable()) return;
  const surface = surfaceNow();
  const panel = npHost();
  const art = panel ? artOf(panel) : null;
  if (!surface || !panel || !art) return;
  busy = true;
  const my = ++gen;
  resizedInMotion = false;
  try {
    if (grownState()) await collapseGrow("player", false);
    const m = motion();
    const from = rect(panel);
    const fromArt = rect(art);
    const queue = stageHost();
    const fromQueue = queue ? rect(queue) : null;
    const restStill = m && queue ? restCopy(panel) : null;
    const row1 = surface === "midi" ? from.height : undefined;
    const cards = surface === "max" ? contentHosts() : [];
    pin(cards, cards.map(rect));
    apply(true, undefined, row1);
    const first = layoutNow(panel);
    stack = first.stack;
    fp().toggleAttribute("data-fp-stack", stack);
    if (first.stackH > 0) opts?.bento.style.setProperty("--fp-stack-h", `${first.stackH}px`);
    const where = first.place;
    panel.dataset.npRows = where;
    const box = rect(art).width * coverShare();
    wantHistory = first.history;
    const stacked = stack && surface === "max" && first.history;
    if (stacked) mountSide();
    const px = fullArtPx(box, window.devicePixelRatio, restArtPx());
    const img = art.querySelector<HTMLImageElement>(".vinyl:not(.vinyl--out) img");
    const big = img?.src ? sizedUrl(img.src, px) : null;
    // The large file loads before the open, so the flight never shows a soft picture.
    const pre = big && m ? preload(big, tokenMs("--player-art-wait")) : null;
    if (pre && !pre.ready) {
      apply(false);
      await pre.wait;
    }
    if (my !== gen || !fullPlayerAvailable() || surfaceNow() !== surface) {
      if (!full) {
        apply(false);
        unpin();
      }
      return;
    }
    artPx = 0;
    askArt(box);

    full = true;
    openedOn = surface;
    apply(true, where, row1);
    const covered = coveredHosts();
    covered.forEach((h) => (m ? h.classList.add("is-covering") : cover(h)));
    log(`open (${cause}, ${surface}, rows ${where}${stack ? ", stacked" : ""})`);
    emit();
    watchSize(panel);
    if (!m) {
      covered.forEach(cover);
      if (stacked) fp().dataset.fpHistory = "on";
      busy = false;
      afterOpen(panel, my); // the snap still brings the side column (the break-it pass)
      return;
    }

    panel.classList.add("is-player-rebuild");
    const to = rect(panel);
    const toArt = rect(art);
    const pad = clipPad();
    const radius = getComputedStyle(panel).borderRadius || "0px";
    const fly = makeFlight(art, toArt, big);
    if (fly) art.style.visibility = "hidden";
    placeStill(restStill);
    const moving = surface === "max";
    const anims: Animation[] = moving
      ? ghostMotion(panel, from, to, fromArt, toArt, from, m.dur, m.ease)
      : [panel.animate([{ clipPath: clipBetween(fullClipFrom(from, to), to, pad, radius) }, { clipPath: clipOpen(pad, radius) }], { duration: m.dur, easing: m.ease, fill: "both" })];
    if (fly) anims.push(flyAnim(fly, fromArt, toArt, m.dur, m.ease, false));
    if (queue && fromQueue && stacked) {
      // Stacked: the stage card fades from its column and rises in under the player.
      queue.style.opacity = "0";
    } else if (queue && fromQueue) {
      anims.push(...queueRise(queue, fromQueue, rect(queue), pad, m.dur, m.ease, false));
    }
    let rowsIn = false;
    const rows = (): void => {
      if (rowsIn) return;
      rowsIn = true;
      enterBand(panel);
    };
    const rowsTimer = moving ? 0 : window.setTimeout(rows, m.rowsAt);
    await Promise.all(anims.map((a) => a.finished.catch(() => {})));
    window.clearTimeout(rowsTimer);
    anims.forEach((a) => a.cancel());
    endQueueRise(queue);
    fly?.remove();
    art.style.visibility = "";
    if (my === gen) {
      dropGhost();
      dropStill();
    }
    if (full && my === gen) {
      window.setTimeout(() => {
        if (full && my === gen) covered.forEach(cover);
      }, m.dur);
      rows();
      if (stacked && queue) {
        queue.style.removeProperty("opacity");
        fp().dataset.fpHistory = "on";
        paintZones();
        enterRows([queue, opts!.side], 2);
      }
    }
  } finally {
    if (my === gen) busy = false;
  }
  afterOpen(panel, my);
}

/** The side column waits for the open to land, then slides in when the card is wide. */
function afterOpen(panel: HTMLElement, my: number): void {
  if (!full || my !== gen) return;
  if (surfaceNow() === "max") mountSide();
  requestAnimationFrame(() => requestAnimationFrame(() => {
    if (!full || my !== gen) return;
    if (resizedInMotion || wantHistory !== (fp().dataset.fpHistory === "on")) refit(panel);
  }));
}

/** The player is open or on its way (the open waits for the large cover first). */
export const fullPlayerBusyOrOpen = (): boolean => full || busy;

// ── close ───────────────────────────────────────────────────────────────────────
/** Close it. `withMotion` false snaps (a surface change, a card request). */
export async function closeFullPlayer(cause: string, withMotion = true): Promise<void> {
  if (!full) return;
  const panel = npHost();
  const art = panel ? artOf(panel) : null;
  const surface = surfaceNow();
  const my = ++gen;
  const m = withMotion && !busy && surface === openedOn ? motion() : null;
  const midway = ghost ? rect(ghost) : null;
  dropGhost();
  panel?.getAnimations().forEach((a) => a.cancel());
  endQueueRise(opts?.hosts.queue ?? null);
  opts?.hosts.queue.style.removeProperty("opacity");
  document.querySelectorAll(".np__fly").forEach((f) => f.remove());
  dropStill();
  if (art) art.style.visibility = "";
  dropHistory();
  panel?.getAnimations({ subtree: true }).forEach((a) => a.id === "rows-slide" && a.cancel());
  busy = true;
  const covered = coveredHosts();
  const end = (): void => {
    full = false;
    dropGhost();
    dropHistory(cause !== "surface");
    unpin();
    apply(false);
    stack = false;
    opts?.bento.style.removeProperty("--fp-stack-h");
    covered.forEach(uncover);
    contentHosts().forEach(uncover);
    watch?.disconnect();
    watch = null;
    artPx = 0;
    requestAnimationFrame(() => {
      if (!full) setFullArtPx(0);
    });
    panel?.classList.remove("is-player-rebuild", "is-rows-sliding");
    log(`close (${cause}${m ? "" : ", no motion"})`);
    if (my === gen) busy = false;
    emit();
  };
  if (!m || !panel || !art || !surface) {
    end();
    return;
  }
  const to = rect(panel);
  const toArt = rect(art);
  const queue = stageHost();
  const toQueue = queue ? rect(queue) : null;
  const row1 = opts?.bento.style.getPropertyValue("--player-row1");
  const where = panel.dataset.npRows as RowsPlace | undefined;
  const wasStack = stack;
  const cards = surface === "max" ? contentHosts() : [];
  unpin(cards);
  apply(false);
  const rest = rect(panel);
  const restArt = rect(art);
  const restQueue = queue ? rect(queue) : null;
  const restCards = cards.map(rect);
  const restStill = queue ? restCopy(panel) : null;
  apply(true, where, row1 ? parseFloat(row1) : undefined);
  fp().toggleAttribute("data-fp-stack", wasStack);
  pin(cards, restCards);

  panel.classList.add("is-player-rebuild");
  covered.forEach((h) => {
    h.classList.remove("is-covered");
    h.inert = false;
    h.style.transition = "none";
    h.classList.add("is-covering");
  });
  void panel.offsetWidth;
  covered.forEach((h) => {
    h.style.transition = "";
    h.classList.remove("is-covering");
    h.classList.add("is-uncovering");
  });
  const pad = clipPad();
  const radius = getComputedStyle(panel).borderRadius || "0px";
  const fly = makeFlight(art, toArt, null);
  if (fly) art.style.visibility = "hidden";
  placeStill(restStill);
  const moving = surface === "max";
  const anims: Animation[] = moving
    ? ghostMotion(panel, midway ?? to, rest, toArt, restArt, rest, m.collapseDur, m.ease)
    : [panel.animate([{ clipPath: clipOpen(pad, radius) }, { clipPath: clipBetween(fullClipFrom(rest, to), to, pad, radius) }], { duration: m.collapseDur, easing: m.ease, fill: "both" })];
  if (fly) anims.push(flyAnim(fly, restArt, toArt, m.collapseDur, m.ease, true));
  if (queue && toQueue && restQueue) {
    if (wasStack) {
      // Stacked: the band's stage card fades in at its column under the shrinking player.
      pin([queue], [restQueue]);
      const a = queue.animate([{ opacity: 0 }, { opacity: 0, offset: 0.3 }, { opacity: 1 }], { duration: m.collapseDur, easing: m.ease, fill: "both" });
      anims.push(a);
    } else {
      anims.push(...queueRise(queue, restQueue, toQueue, pad, m.collapseDur, m.ease, true));
    }
  }
  await Promise.all(anims.map((a) => a.finished.catch(() => {})));
  if (my !== gen) return;
  end();
  anims.forEach((a) => a.cancel());
  endQueueRise(queue);
  fly?.remove();
  art.style.visibility = "";
  covered.forEach((h) => h.classList.remove("is-uncovering"));
  if (moving) {
    dropGhost();
    dropStill();
    enterBand(panel);
  } else {
    enterBand(panel);
  }
}

export function toggleFullPlayer(cause: string): void {
  if (full) void closeFullPlayer(cause);
  else void openFullPlayer(cause);
}

/** While full: the rows' place, the column and the cover file follow the card's size. */
function watchSize(panel: HTMLElement): void {
  watch?.disconnect();
  const np = npOf(panel);
  if (!np) return;
  let started = false;
  watch = new ResizeObserver(() => {
    if (!started) return void (started = true);
    refit(panel);
  });
  watch.observe(np);
  if (opts?.bento) watch.observe(opts.bento);
}
function refit(panel: HTMLElement): void {
  if (!full) return;
  const art = artOf(panel);
  const next = layoutNow(panel);
  const histNow = fp().dataset.fpHistory === "on";
  if (busy) {
    if (next.stack !== stack || panel.dataset.npRows !== next.place || next.history !== wantHistory) resizedInMotion = true;
    setStack(next.stack, next.stackH);
    if (panel.dataset.npRows !== next.place) panel.dataset.npRows = next.place;
  } else if (panel.dataset.npRows !== next.place || next.history !== histNow || next.stack !== stack) {
    slideRows(panel, next);
  } else {
    setStack(stack, next.stackH);
  }
  if (art) askArt(rect(art).width * coverShare());
}
/** A skin change can change the cover's share or the title's place with no change of size. */
export function fullPlayerLookChanged(): void {
  const panel = npHost();
  if (full && panel && !busy) refit(panel);
}

// ── the edge strips on Now Playing ──────────────────────────────────────────────
type Side = "left" | "right" | "up" | "down";
const BAR_SIDE: Record<Side, string> = { right: "left", left: "right", down: "top", up: "bottom" };
const zonesOn = (): boolean => fullPlayerAvailable() && grownState()?.slot !== "queue";

function reachOf(side: Side): "half" | "full" | "edge" | null {
  const s = surfaceNow();
  if (full && stack) {
    if (side === "down" || side === "up") return "full";
    return s === "max" ? "edge" : null;
  }
  if (s === "midi") {
    if (side === "up") return "full";
    if (side === "down") return full ? "edge" : "full";
    return null;
  }
  if (s === "max") {
    if (full) {
      if (side === "up") return "full";
      if (side === "down") return "edge";
      if (side === "left") return "half";
      return fp().dataset.fpHistory === "on" ? "half" : "edge";
    }
    if (side === "left") return "edge";
    if (side === "right") return "full";
    if (side === "up") return "full";
    return "half"; // the bottom gap: the stage card's own strip (its grow up) takes the other half
  }
  return null;
}

/** Show and place the strips (put in again when the card has written its host). */
export function paintZones(): void {
  const host = npHost();
  if (!host) return;
  let zones = [...host.querySelectorAll<HTMLElement>(":scope > .grow-zone[data-player-side]")];
  if (!zones.length) {
    zones = (["left", "right", "up", "down"] as Side[]).map((side) => {
      const el = document.createElement("div");
      el.className = "grow-zone";
      el.dataset.playerSide = side;
      el.hidden = true;
      el.setAttribute("role", "button");
      el.innerHTML = `<span class="grow-zone__bar" aria-hidden="true"></span>`;
      el.addEventListener("click", (e) => {
        e.stopPropagation();
        if (busy || !zonesOn()) return;
        toggleFullPlayer("strip");
      });
      host.appendChild(el);
      return el;
    });
  }
  const on = zonesOn();
  for (const el of zones) {
    const side = el.dataset.playerSide as Side;
    const reach = on ? reachOf(side) : null;
    if (!reach) {
      el.hidden = true;
      continue;
    }
    el.hidden = false;
    el.dataset.side = side;
    el.dataset.reach = reach;
    el.dataset.act = full ? "collapse" : "grow";
    el.className = `grow-zone grow-zone--${BAR_SIDE[side]}`;
    const hint = full ? "Back to the cards" : "Opens the full player";
    el.dataset.hint = hint;
    el.setAttribute("aria-label", hint);
  }
}

export function initFullPlayer(o: Opts): void {
  opts = o;
  // A surface change ends it with no motion, before the layout remounts the slots.
  onSurface(() => {
    if (full) void closeFullPlayer("surface", false);
    else dropHistory(false);
    paintZones();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !full || busy || e.defaultPrevented) return;
    const t = e.target;
    if (t instanceof Element && t.closest("input, textarea, [contenteditable]")) return;
    if (document.querySelector(".dm-menu:not([hidden]), .slot-picker__menu:not([hidden])")) return;
    void closeFullPlayer("esc");
  });
  paintZones();
}

/** The side card changed (a Queue/History swap): mount the new one next time. */
export function resetSide(): void {
  if (full) {
    const had = fp().dataset.fpHistory === "on";
    sideUndo?.();
    sideUndo = null;
    if (had) mountSide();
  } else {
    dropHistory(false);
  }
}
