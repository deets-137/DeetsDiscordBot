// Motion helpers, as DeetsMusic's own (src/motion.ts, src/boot-cover.ts `tokenMs`, src/pop.ts
// `enterRows`). The app reads its Motion setting; a frame has no settings, so it follows the
// device's "reduce motion" choice and puts `data-motion="reduced"` on <html>, the attribute the
// copied look already reads.

const root = document.documentElement;
const query = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;

function paint(): void {
  if (query?.matches) root.dataset.motion = "reduced";
  else delete root.dataset.motion;
}

export function initMotion(): void {
  paint();
  query?.addEventListener("change", () => {
    paint();
    console.info(`[deetsmusic-ui] motion ${query.matches ? "reduced: the device asks for less motion" : "full"}`);
  });
}

/** The device asks for less motion: every moving part snaps or stays still. */
export const motionReduced = (): boolean => !!query?.matches;

/** A time token of the look in milliseconds (`0.34s` or `340ms`), scaled by --motion-scale. */
export function tokenMs(name: string): number {
  const cs = getComputedStyle(root);
  const v = cs.getPropertyValue(name).trim();
  const n = parseFloat(v);
  if (!Number.isFinite(n)) return 0;
  const scale = parseFloat(cs.getPropertyValue("--motion-scale")) || 1;
  return (v.endsWith("ms") ? n : n * 1000) * scale;
}

/** A length token of the look in px (0 when unset). */
export const pxToken = (name: string): number => parseFloat(getComputedStyle(root).getPropertyValue(name)) || 0;

/** Slide `els` in one after another (the look's `.pop-enter`). Only the first `max` move. */
export function enterRows(els: Iterable<Element>, max = 12): void {
  if (motionReduced()) return;
  const rows = [...els].slice(0, max) as HTMLElement[];
  if (!rows.length) return;
  if (rows.some((el) => el.classList.contains("pop-enter"))) {
    rows.forEach((el) => el.classList.remove("pop-enter"));
    void rows[0].offsetWidth;
  }
  rows.forEach((el, i) => {
    el.style.setProperty("--pop-i", String(i));
    el.classList.add("pop-enter");
    const done = (e: AnimationEvent): void => {
      if (e.target !== el) return;
      el.classList.remove("pop-enter");
      el.removeEventListener("animationend", done);
    };
    el.addEventListener("animationend", done);
  });
}

export const rectOf = (el: Element): DOMRect => el.getBoundingClientRect();

type Edges = Pick<DOMRect, "left" | "top" | "right" | "bottom">;
/** The grow's clip pad (--grow-clip-pad): the clip opens past the box by it, for the shadow. */
export const clipPad = (): number => pxToken("--grow-clip-pad");
/** `inset(…)` that shows the `from` box inside the `to` box, opened past it by `pad`. */
export const clipBetween = (from: Edges, to: Edges, pad: number, radius: string): string =>
  `inset(${from.top - to.top - pad}px ${to.right - from.right - pad}px ${to.bottom - from.bottom - pad}px ${from.left - to.left - pad}px round ${radius})`;
export const clipOpen = (pad: number, radius: string): string => `inset(${-pad}px round ${radius})`;

/** A card under a grown card or the full player: hidden and inert once the motion is done. */
export function cover(h: HTMLElement | null | undefined): void {
  if (!h) return;
  h.classList.remove("is-covering", "is-uncovering");
  h.classList.add("is-covered");
  h.inert = true;
}
export function uncover(h: HTMLElement | null | undefined): void {
  if (!h) return;
  h.classList.remove("is-covered", "is-covering", "is-uncovering");
  h.inert = false;
}
