// Hover hints: DeetsMusic's themed hint box in place of the browser's own `title` box. A port
// of the app's src/hint.ts; styles/hint.css in the look draws it.
//
// Central by adoption, as in the app: a call site still writes `title="…"` or `el.title = …`.
// This module moves every `title` to `data-hint` and removes the attribute, so the browser
// never draws its own box. A MutationObserver does the same for every later element and write.
//
// Two kinds of hint:
//   a written hint  what a `title` said. One line.
//   a row hint      the song's name and the line under it, read off the row, shown when the
//                   row cut either off.
// The deeper element wins: a button inside a row shows its own hint.
//
// The app has three settings for hints. A frame in a call has no settings, so the app's
// defaults stand: hints on, the "normal" wait, a row hint only when a name is cut off. Left
// out: the writers' line (the bridge sends no credits). Not shown for a touch pointer.

import "./look/src/styles/hint.css";

const DELAY_MS = 600; // the app's "normal"
/** A song row waits longer than a button: scanning a list must stay quiet. */
const ROW_WAIT = 1.6;
/** Move to another anchor within this long of a hint closing and the next one is instant. */
const WARM_MS = 400;

interface Shape {
  row: string;
  title: string;
  sub?: string;
}
const SHAPES: Shape[] = [
  { row: ".qrow", title: ".qrow__title", sub: ".qrow__artist" },
  { row: ".qnow", title: ".qnow__title", sub: ".qnow__artist" },
  { row: ".search__song", title: ".search__song-title", sub: ".search__song-artist" },
  { row: ".np", title: ".np__title", sub: ".np__artist" },
];
const ROW_SEL = SHAPES.map((s) => s.row).join(",");

interface Hint {
  text: string;
  sub?: string;
  row: boolean;
}

let box: HTMLElement | null = null;
let titleEl: HTMLElement | null = null;
let subEl: HTMLElement | null = null;
let anchor: HTMLElement | null = null;
let timer = 0;
let closedAt = 0;
let showing = false;
/** Pressed under the pointer: quiet until the pointer moves off it. */
let pressed: HTMLElement | null = null;
/** Where the last press was. Until the pointer moves STILL_PX from there, no hint arms: a
 *  press can change what sits under a still pointer. */
let pressAt: { x: number; y: number } | null = null;
const STILL_PX = 3;
const stillSincePress = (x: number, y: number): boolean => {
  if (!pressAt) return false;
  if (Math.hypot(x - pressAt.x, y - pressAt.y) < STILL_PX) return true;
  pressAt = null;
  return false;
};

function ensureBox(): HTMLElement {
  if (box) return box;
  box = document.createElement("div");
  box.className = "hint";
  box.setAttribute("aria-hidden", "true");
  box.hidden = true;
  titleEl = document.createElement("span");
  titleEl.className = "hint__title";
  subEl = document.createElement("span");
  subEl.className = "hint__sub";
  box.append(titleEl, subEl);
  document.body.appendChild(box);
  return box;
}

// ── Adoption: every `title` becomes a `data-hint`, and the attribute goes ──────────

function adopt(el: Element): void {
  const raw = el.getAttribute("title");
  if (raw === null) return;
  el.removeAttribute("title");
  const text = raw.trim();
  const host = el as HTMLElement;
  if (!text) {
    delete host.dataset.hint;
  } else {
    host.dataset.hint = text;
    // The hint was the only name this control had: keep it for a screen reader.
    const named = el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby");
    if (!named && !(el.textContent ?? "").trim()) el.setAttribute("aria-label", text);
  }
  // A hint that changed under the pointer (Play → Pause): new words, centred again.
  if (anchor === host && showing && fill(host)) place(host, NaN);
}

function sweep(root: ParentNode): void {
  if (root instanceof Element && root.hasAttribute("title")) adopt(root);
  root.querySelectorAll?.("[title]").forEach(adopt);
}

// ── What the pointer is on ────────────────────────────────────────────────────────

const cutOff = (el: HTMLElement | null): boolean => !!el && el.scrollWidth > el.clientWidth + 1;

function rowHint(row: HTMLElement, shape: Shape): Hint | null {
  const t = row.querySelector<HTMLElement>(shape.title);
  const s = shape.sub ? row.querySelector<HTMLElement>(shape.sub) : null;
  const text = t?.textContent?.trim() ?? "";
  const sub = s?.textContent?.trim() ?? "";
  if (!text && !sub) return null;
  if (!cutOff(t) && !cutOff(s)) return null;
  return { text, sub: sub || undefined, row: true };
}

/** Walk up from the pointer. The first element that can say something owns the hint. */
function resolve(from: Element | null): { el: HTMLElement; hint: Hint } | null {
  for (let el = from as HTMLElement | null; el && el !== document.body; el = el.parentElement) {
    if (el === box) return null;
    const node = el;
    if (node.matches(ROW_SEL)) {
      const shape = SHAPES.find((s) => node.matches(s.row))!;
      const h = rowHint(node, shape);
      if (h) return { el: node, hint: h };
    }
    const written = node.dataset.hint;
    if (written) return { el: node, hint: { text: written, row: false } };
  }
  return null;
}

// ── Showing ───────────────────────────────────────────────────────────────────────

/** The floating tier: menus and the slot picker. A hint for a control inside one goes above it. */
const FLOATING = ".pop, .slot-picker__menu";

function fill(el: HTMLElement): boolean {
  const found = resolve(el);
  if (!found || found.el !== el) return false;
  const b = ensureBox();
  titleEl!.textContent = found.hint.text;
  subEl!.textContent = found.hint.sub ?? "";
  subEl!.hidden = !found.hint.sub;
  b.classList.toggle("hint--row", found.hint.row);
  b.classList.toggle("hint--over", !!el.closest(FLOATING));
  return true;
}

function place(el: HTMLElement, px: number): void {
  const b = ensureBox();
  const r = el.getBoundingClientRect();
  const gap = parseFloat(getComputedStyle(b).getPropertyValue("--hint-gap")) || 8;
  const edge = 6; // never touch the frame's edge
  // Measure at the left edge: a fixed box's width fits the room right of where it stands.
  b.style.left = "0px";
  const bw = b.offsetWidth;
  const bh = b.offsetHeight;
  // Follow the pointer across a row, so a wide row's hint sits where the person looks.
  const centre = Number.isFinite(px) ? px : r.left + r.width / 2;
  const left = Math.max(edge, Math.min(Math.round(centre - bw / 2), window.innerWidth - bw - edge));
  const below = Math.round(r.bottom + gap);
  const over = Math.round(r.top - gap - bh);
  const top = below + bh > window.innerHeight - edge ? over : below; // no room below: above
  b.style.left = `${left}px`;
  b.style.top = `${Math.max(edge, top)}px`;
}

function show(el: HTMLElement, px: number): void {
  if (!el.isConnected) return;
  const b = ensureBox();
  b.hidden = false;
  if (!fill(el)) {
    b.hidden = true;
    return;
  }
  place(el, px);
  void b.offsetWidth; // the box was display:none: start the fade from 0
  b.classList.add("is-on");
  showing = true;
}

/** Takes the hint down now (a menu opens over the control it explains). */
export function hideHint(): void {
  if (!anchor && !showing && !timer) return; // nothing is up: a scroll must cost nothing
  clearTimeout(timer);
  timer = 0;
  anchor = null;
  if (!box) return;
  if (showing) closedAt = performance.now();
  showing = false;
  box.classList.remove("is-on");
  box.hidden = true;
}

function enter(target: Element | null, px: number): void {
  const found = resolve(target);
  if (!found) {
    pressed = null;
    if (anchor) hideHint();
    return;
  }
  if (found.el !== pressed) pressed = null; // the pointer moved on: the press is spent
  if (found.el === anchor || found.el === pressed) return;
  const warm = showing || performance.now() - closedAt < WARM_MS;
  const el = found.el;
  const row = found.hint.row;
  hideHint();
  anchor = el;
  const wait = warm ? 0 : Math.round(row ? DELAY_MS * ROW_WAIT : DELAY_MS);
  if (wait === 0) show(el, px);
  else timer = window.setTimeout(() => show(el, px), wait);
}

let started = false;

/** Adopt every hint on the page and start listening. Call once, after the page's shell exists. */
export function initHints(): void {
  if (started) return;
  started = true;
  sweep(document);

  new MutationObserver((recs) => {
    for (const r of recs) {
      if (r.type === "attributes") {
        if (r.target instanceof Element) adopt(r.target);
      } else {
        r.addedNodes.forEach((n) => {
          if (n instanceof Element) sweep(n);
        });
      }
    }
    if (anchor && !anchor.isConnected) hideHint(); // the row under the pointer was drawn again
  }).observe(document.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ["title"] });

  document.addEventListener("pointerover", (e) => {
    if (e.pointerType === "touch") return; // nothing rests on a touchscreen
    if (stillSincePress(e.clientX, e.clientY)) return;
    enter(e.target as Element | null, e.clientX);
  });
  // The first real move after a press: no `pointerover` comes when the pointer stays inside
  // the element it rests on, so that element's hint arms here.
  document.addEventListener("pointermove", (e) => {
    if (!pressAt || e.pointerType === "touch" || stillSincePress(e.clientX, e.clientY)) return;
    enter(e.target as Element | null, e.clientX);
  });
  // A press is intent: the hint has said what it had to say.
  document.addEventListener(
    "pointerdown",
    (e) => {
      pressed = resolve(e.target as Element | null)?.el ?? null;
      pressAt = e.pointerType === "touch" ? null : { x: e.clientX, y: e.clientY };
      hideHint();
    },
    true,
  );
  document.addEventListener("pointerleave", hideHint);
  window.addEventListener("blur", hideHint);
  window.addEventListener("resize", hideHint);
  document.addEventListener("scroll", hideHint, true); // capture: a card's own scroller too
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") hideHint();
  });
  // Keyboard: the hint follows focus as it follows the pointer.
  document.addEventListener("focusin", (e) => {
    const el = e.target as Element | null;
    if (!el?.matches?.(":focus-visible")) return;
    enter(el, NaN);
  });
  document.addEventListener("focusout", hideHint);
}
