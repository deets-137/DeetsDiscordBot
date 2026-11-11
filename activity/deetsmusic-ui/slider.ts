// The drag slider, as DeetsMusic's slider.ts: one pointer-capture loop, and the fill published
// to the look through `--slider-fill` (a percentage) and `--slider-frac` (a plain number). The
// look's `.scrub` rules read both, and `data-dragging` / `data-released` drive the skins' handles.

export interface SliderHandle {
  /** Show a value from outside (0..1). Does nothing while the person drags. */
  setValue(frac: number): void;
  readonly dragging: boolean;
}

const clamp01 = (n: number): number => Math.max(0, Math.min(1, n));
const RELEASE_HOLD_MS = 400;

export function makeSlider(
  el: HTMLElement,
  opts: { onDrag?: (frac: number) => void; onCommit?: (frac: number) => void; enabled?: () => boolean },
): SliderHandle {
  let dragging = false;
  let releasedTimer = 0;

  const fracAt = (x: number): number => {
    const r = el.getBoundingClientRect();
    return r.width > 0 ? clamp01((x - r.left) / r.width) : 0;
  };
  const reflect = (frac: number): void => {
    el.style.setProperty("--slider-fill", `${(clamp01(frac) * 100).toFixed(2)}%`);
    el.style.setProperty("--slider-frac", clamp01(frac).toFixed(4));
  };

  el.addEventListener("pointerdown", (e) => {
    if (opts.enabled && !opts.enabled()) return;
    dragging = true;
    el.dataset.dragging = "";
    window.clearTimeout(releasedTimer);
    delete el.dataset.released;
    el.setPointerCapture(e.pointerId);
    const f = fracAt(e.clientX);
    reflect(f);
    opts.onDrag?.(f);
  });
  el.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const f = fracAt(e.clientX);
    reflect(f);
    opts.onDrag?.(f);
  });
  const end = (e: PointerEvent): void => {
    if (!dragging) return;
    dragging = false;
    delete el.dataset.dragging;
    el.dataset.released = "";
    releasedTimer = window.setTimeout(() => delete el.dataset.released, RELEASE_HOLD_MS);
    const f = fracAt(e.clientX);
    reflect(f);
    opts.onCommit?.(f);
  };
  el.addEventListener("pointerup", end);
  el.addEventListener("pointercancel", end);

  return {
    setValue(frac) {
      if (!dragging) reflect(frac);
    },
    get dragging() {
      return dragging;
    },
  };
}
