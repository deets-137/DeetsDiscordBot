// Card grow: a card takes the room of the card beside it for a while, as in DeetsMusic
// (src/card-grow.ts, docs/cards/CARD-GROW.md). The look's rules already lay a grown card out
// (`data-grow`, `.is-grown`, `.is-covered`); this module decides and moves.
//
//   Max   Search and History (the two content cards) widen over each other. The Queue in the
//         stage column grows UP over Now Playing (STAGE-COLUMN.md §7). The card in a slot does
//         not matter: after a Queue/History swap the Queue widens from the right slot.
//   Midi  the two cards widen over each other.
//   Mini, Wide, the tile: nothing grows.
//
// Ways in: the edge strip in the gap beside a card (`.grow-zone`, its bar shows on hover) and
// the Grow button in the card's header. Ways out: the button, the grown card's strip, Escape, a
// request for a covered card, the full player, a surface change (no motion).
// The motion is the app's first one (CARD-GROW.md §6): the clip opens from the old box, the
// body rebuilds at the new size and its rows come in; the covered card fades under it.

import { clipBetween, clipOpen, clipPad, cover, enterRows, motionReduced, rectOf, tokenMs, uncover } from "./motion";
import { surfaceState } from "./surface";

export type Slot = "np" | "queue" | "left" | "right";
type Dir = "left" | "right" | "up";
type Mode = "wide" | "tall";

interface Opts {
  hosts: Record<Slot, HTMLElement>;
  /** The card's title in a slot now, for the hints. */
  titleOf(slot: Slot): string;
  /** The full player is open: nothing grows, and every strip hides. */
  playerOpen(): boolean;
}

let opts: Opts | null = null;
let state: { slot: Slot; dir: Dir } | null = null;
let busy = false;
let gen = 0;
const subs = new Set<() => void>();

const ICON_GROW = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4h6v6M20 4l-7 7M10 20H4v-6M4 20l7-7"/></svg>';
const ICON_SHRINK = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 10h-6V4M14 10l7-7M4 14h6v6M10 14l-7 7"/></svg>';
/** Which edge of the strip the bar hugs (the card's edge), as the app's `BAR_SIDE`. */
const BAR_SIDE: Record<Dir, string> = { right: "left", left: "right", up: "bottom" };

/** The directions each slot can grow on the frame now. */
function neighbors(): Partial<Record<Slot, Partial<Record<Dir, Slot>>>> {
  const s = surfaceState();
  if (s.tile || s.wide) return {};
  if (s.surface === "max") return { left: { right: "right" }, right: { left: "left" }, queue: { up: "np" } };
  if (s.surface === "midi") return { left: { right: "right" }, right: { left: "left" } };
  return {};
}
const modeOf = (dir: Dir): Mode => (dir === "up" ? "tall" : "wide");
const coveredOf = (slot: Slot, dir: Dir): Slot | undefined => neighbors()[slot]?.[dir];
const hasCard = (slot: Slot): boolean => !!opts?.hosts[slot].dataset.mounted;

/** The card grown now, or null. */
export const grownState = (): { slot: Slot; dir: Dir } | null => state;
/** The card in this slot is under a grown card. */
export const isCovered = (slot: Slot): boolean => !!state && coveredOf(state.slot, state.dir) === slot;
export function onGrowChange(cb: () => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}
function emit(): void {
  refreshGrow();
  subs.forEach((cb) => cb());
}

function growHint(slot: Slot, dir: Dir): string {
  const me = opts?.titleOf(slot) ?? "this card";
  const over = opts?.titleOf(coveredOf(slot, dir)!) ?? "the next card";
  return dir === "up" ? `Make ${me} taller, over ${over}` : `Widen ${me} over ${over}`;
}
const collapseHint = (slot: Slot): string => `Collapse ${opts?.titleOf(slot) ?? "this card"} back to its place`;

/** The rows on screen in a grown card's body: each one comes in at the grow's own stagger. */
function visibleRows(panel: HTMLElement): HTMLElement[] {
  const box = rectOf(panel);
  return [...panel.querySelectorAll<HTMLElement>(".qnow, .qcard__label, .qrow, .search__song, .search__label, .qcard__empty")].filter((el) => {
    const r = rectOf(el);
    return r.height > 0 && r.bottom >= box.top && r.top <= box.bottom;
  });
}
function rowsIn(panel: HTMLElement): void {
  panel.classList.remove("is-grow-rebuild");
  if (motionReduced()) return;
  panel.classList.add("is-grow-rows");
  window.setTimeout(() => panel.classList.remove("is-grow-rows"), tokenMs("--grow-body-fade") + 50);
  enterRows(visibleRows(panel), 40);
}

function motion(collapse: boolean): { dur: number; ease: string } | null {
  if (motionReduced()) return null;
  const dur = collapse ? tokenMs("--grow-collapse-dur") || tokenMs("--grow-dur") : tokenMs("--grow-dur");
  if (dur <= 0) return null;
  return { dur, ease: getComputedStyle(document.documentElement).getPropertyValue("--grow-ease").trim() || "ease" };
}

/** Grow the card in `slot` toward `dir`. */
export async function growCard(slot: Slot, dir: Dir, cause: string): Promise<void> {
  if (!opts || busy || opts.playerOpen()) return;
  const n = coveredOf(slot, dir);
  if (!n || !hasCard(slot)) return;
  if (state) collapseNow("swap");
  const panel = opts.hosts[slot];
  const under = opts.hosts[n];
  const m = motion(false);
  const my = ++gen;
  busy = true;
  const from = rectOf(panel);
  state = { slot, dir };
  panel.dataset.grow = modeOf(dir);
  panel.classList.add("is-grown");
  console.info(`[deetsmusic-ui] grow: ${opts.titleOf(slot)} ${modeOf(dir)} over ${opts.titleOf(n)} (${cause})`);
  emit();
  if (!m) {
    cover(under);
    busy = false;
    return;
  }
  under.classList.add("is-covering");
  panel.classList.add("is-grow-rebuild");
  const to = rectOf(panel);
  const pad = clipPad();
  const radius = getComputedStyle(panel).borderRadius || "0px";
  const anim = panel.animate([{ clipPath: clipBetween(from, to, pad, radius) }, { clipPath: clipOpen(pad, radius) }], {
    duration: m.dur, easing: m.ease, fill: "both",
  });
  await anim.finished.catch(() => {});
  anim.cancel();
  if (my !== gen) return;
  busy = false;
  rowsIn(panel);
  cover(under);
}

/** End the grow at once, with no motion. */
function collapseNow(cause: string): void {
  if (!state || !opts) return;
  const panel = opts.hosts[state.slot];
  const n = coveredOf(state.slot, state.dir);
  gen++;
  busy = false;
  panel.getAnimations().forEach((a) => a.cancel());
  delete panel.dataset.grow;
  panel.classList.remove("is-grown", "is-grow-rebuild", "is-grow-rows");
  // The covered slot of the grow that was: a surface change may have changed the neighbors.
  for (const h of Object.values(opts.hosts)) if (h !== panel && (h.classList.contains("is-covered") || h.classList.contains("is-covering"))) uncover(h);
  if (n) uncover(opts.hosts[n]);
  console.info(`[deetsmusic-ui] grow: collapsed (${cause})`);
  state = null;
  emit();
}

/** Collapse the grown card. `withMotion` false snaps. */
export async function collapseGrow(cause: string, withMotion = true): Promise<void> {
  if (!state || !opts) return;
  const m = withMotion && !busy ? motion(true) : null;
  if (!m) return collapseNow(cause);
  const { slot, dir } = state;
  const panel = opts.hosts[slot];
  const n = coveredOf(slot, dir);
  const under = n ? opts.hosts[n] : null;
  const my = ++gen;
  busy = true;
  const to = rectOf(panel);
  // The resting box, measured with the grow off for one layout and no paint.
  delete panel.dataset.grow;
  const rest = rectOf(panel);
  panel.dataset.grow = modeOf(dir);
  if (under) {
    under.classList.remove("is-covered");
    under.inert = false;
    under.style.transition = "none";
    under.classList.add("is-covering");
    void under.offsetWidth;
    under.style.transition = "";
    under.classList.remove("is-covering");
    under.classList.add("is-uncovering");
  }
  panel.classList.add("is-grow-rebuild");
  const pad = clipPad();
  const radius = getComputedStyle(panel).borderRadius || "0px";
  const anim = panel.animate([{ clipPath: clipOpen(pad, radius) }, { clipPath: clipBetween(rest, to, pad, radius) }], {
    duration: m.dur, easing: m.ease, fill: "both",
  });
  await anim.finished.catch(() => {});
  if (my !== gen) return;
  anim.cancel();
  delete panel.dataset.grow;
  panel.classList.remove("is-grown");
  uncover(under);
  state = null;
  busy = false;
  console.info(`[deetsmusic-ui] grow: collapsed (${cause})`);
  emit();
  rowsIn(panel);
}

/** The Grow button's one press: grow toward the neighbor, or collapse. */
function pressButton(slot: Slot): void {
  if (state?.slot === slot) return void collapseGrow("button");
  const dirs = Object.keys(neighbors()[slot] ?? {}) as Dir[];
  if (dirs.length) void growCard(slot, dirs[0], "button");
}

// ── the strips and the button ─────────────────────────────────────────────────
/** Put the strips and the button into each host again (a card writes its host when it
 *  mounts) and show the ones that act now. */
export function refreshGrow(): void {
  if (!opts) return;
  const map = neighbors();
  const off = opts.playerOpen();
  for (const slot of Object.keys(opts.hosts) as Slot[]) {
    const host = opts.hosts[slot];
    const dirs = hasCard(slot) ? (Object.keys(map[slot] ?? {}) as Dir[]) : [];
    paintZone(host, slot, dirs, off);
    paintButton(host, slot, dirs, off);
  }
}

function paintZone(host: HTMLElement, slot: Slot, dirs: Dir[], off: boolean): void {
  let el = host.querySelector<HTMLElement>(":scope > .grow-zone[data-grow-slot]");
  const grown = state?.slot === slot;
  const show = !off && dirs.length > 0 && !isCovered(slot);
  if (!show) {
    if (el) el.hidden = true;
    return;
  }
  if (!el) {
    el = document.createElement("div");
    el.dataset.growSlot = slot;
    el.setAttribute("role", "button");
    el.innerHTML = `<span class="grow-zone__bar" aria-hidden="true"></span>`;
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      if (busy) return;
      if (state?.slot === slot) void collapseGrow("zone");
      else void growCard(slot, el!.dataset.side as Dir, "zone");
    });
    host.appendChild(el);
  }
  const dir = grown ? state!.dir : dirs[0];
  el.hidden = false;
  el.dataset.side = dir;
  // A grown card's strip is on the frame's edge (sideways) or under the title bar (up); at rest
  // it is half the gap: the neighbor's own strip takes the other half.
  el.dataset.reach = grown ? (dir === "up" ? "full" : "edge") : "half";
  el.dataset.act = grown ? "collapse" : "grow";
  el.className = `grow-zone grow-zone--${BAR_SIDE[dir]}`;
  const hint = grown ? collapseHint(slot) : growHint(slot, dir);
  el.dataset.hint = hint;
  el.setAttribute("aria-label", hint);
}

function paintButton(host: HTMLElement, slot: Slot, dirs: Dir[], off: boolean): void {
  const head = host.querySelector<HTMLElement>(":scope > .panel__head");
  if (!head) return;
  let box = head.querySelector<HTMLElement>(":scope > .grow-btns");
  if (off || !dirs.length || isCovered(slot)) {
    box?.remove();
    return;
  }
  if (!box) {
    box = document.createElement("div");
    box.className = "grow-btns";
    box.innerHTML = `<button class="grow-btn" type="button"></button>`;
    box.querySelector("button")!.addEventListener("click", (e) => {
      e.stopPropagation();
      pressButton(slot);
    });
    head.appendChild(box);
  }
  const grown = state?.slot === slot;
  const btn = box.querySelector<HTMLButtonElement>("button")!;
  btn.innerHTML = grown ? ICON_SHRINK : ICON_GROW;
  const hint = grown ? collapseHint(slot) : growHint(slot, dirs[0]);
  btn.dataset.hint = hint;
  btn.setAttribute("aria-label", hint);
  placeButton(head);
}

/** The button sits just right of the title's text, so nothing in the header moves. */
function placeButton(head: HTMLElement): void {
  const title = head.querySelector<HTMLElement>(".panel__title");
  if (!title) return;
  const range = document.createRange();
  range.selectNodeContents(title);
  const textRight = range.getBoundingClientRect().right;
  const left = Math.min(textRight, rectOf(title).right) - rectOf(head).left;
  head.style.setProperty("--grow-btn-left", `${Math.max(0, left)}px`);
}

export function initGrow(o: Opts): void {
  opts = o;
  // Escape collapses, unless a menu or a text field takes it first.
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !state || busy || e.defaultPrevented) return;
    const t = e.target;
    if (t instanceof Element && t.closest("input, textarea, [contenteditable]")) return;
    if (document.querySelector(".dm-menu:not([hidden]), .slot-picker__menu:not([hidden])")) return;
    void collapseGrow("esc");
  });
  // The titles can change width (a font that loads late, a resize): the button follows.
  const heads = new ResizeObserver(() => document.querySelectorAll<HTMLElement>(".panel__head:has(> .grow-btns)").forEach(placeButton));
  for (const h of Object.values(o.hosts)) heads.observe(h);
  refreshGrow();
}

/** The surface changed: a grow ends with no motion (the slots may move). */
export function growSurfaceChanged(): void {
  if (state) collapseNow("surface");
  else refreshGrow();
}
