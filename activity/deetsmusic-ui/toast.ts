// Toasts, with DeetsMusic's markup and behaviour (its src/toast.ts; styles/toast.css draws
// them): a timed strip with a bar that drains, held while the pointer rests on it, and an
// optional row of buttons. Used for the host's own line when it refuses an action, for "added
// to the queue", and for that line's Play Now button (ACTIVITY.md §13.5 call 2).
//
// Left out of the app's module: the tiers and one-time notices (no settings here), the sticky
// queue (nothing here is sticky) and the Report button.

import "./look/src/styles/toast.css";

export type ToastKind = "info" | "success" | "warn" | "error";

export interface ToastAction {
  label: string;
  run?: () => void;
}

const CAP = 3;
const HOLD_MS = 3200; // the app's default
const HOLD_ACTION_MS = 6000; // a line with a button: time to read it and to reach the button
const MIN_RESUME_MS = 400;
const REAP_FALLBACK_MS = 600;

let host: HTMLElement | null = null;

function ensureHost(): HTMLElement {
  if (host) return host;
  const h = document.createElement("div");
  h.className = "toast-host";
  h.setAttribute("aria-live", "polite");
  document.body.appendChild(h);
  host = h;
  // Mini and Midi: the stack hangs under the Now Playing card, so the song stays readable.
  // The card's bottom edge moves with the surface and the frame, so it is measured (the app's
  // `--toast-top`). The look's CSS ignores it on Max and on the player.
  const np = document.querySelector<HTMLElement>('[data-slot="np"]');
  const place = (): void => {
    const r = np?.getBoundingClientRect();
    if (r && r.height > 0) h.style.setProperty("--toast-top", `${Math.round(r.bottom)}px`);
    else h.style.removeProperty("--toast-top");
  };
  place();
  if (np) new ResizeObserver(place).observe(np);
  window.addEventListener("resize", place);
  return h;
}

const live = (h: HTMLElement): HTMLElement[] =>
  [...h.children].filter((c): c is HTMLElement => c instanceof HTMLElement && !c.classList.contains("toast--out"));

export function toast(text: string, kind: ToastKind = "warn", actions: ToastAction[] = []): void {
  const h = ensureHost();
  // The same line twice in a row stays one toast.
  const last = live(h).at(-1);
  if (last?.dataset.text === text) return;

  const el = document.createElement("div");
  el.className = `toast toast--${kind}`;
  el.setAttribute("role", kind === "error" ? "alert" : "status");
  el.dataset.text = text;
  const p = document.createElement("p");
  p.className = "toast__text";
  p.textContent = text;
  el.appendChild(p);

  let gone = false;
  let timer: number | undefined;
  const dismiss = (): void => {
    if (gone) return;
    gone = true;
    window.clearTimeout(timer);
    el.classList.add("toast--out");
    const reap = (): void => el.remove();
    el.addEventListener("transitionend", reap, { once: true });
    window.setTimeout(reap, REAP_FALLBACK_MS);
  };

  if (actions.length) {
    const row = document.createElement("div");
    row.className = "toast__actions";
    for (const a of actions) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "toast__btn";
      b.textContent = a.label;
      b.addEventListener("click", () => {
        try {
          a.run?.();
        } finally {
          dismiss();
        }
      });
      row.appendChild(b);
    }
    el.appendChild(row);
  }

  const ms = actions.length ? HOLD_ACTION_MS : HOLD_MS;
  const bar = document.createElement("div");
  bar.className = "toast__bar";
  bar.style.animationDuration = `${ms}ms`;
  el.appendChild(bar);
  let remaining = ms;
  let deadline = Date.now() + ms;
  timer = window.setTimeout(dismiss, ms);
  // Hover holds the clock: the look pauses the bar, this pauses the end.
  el.addEventListener("mouseenter", () => {
    if (gone || timer === undefined) return;
    window.clearTimeout(timer);
    timer = undefined;
    remaining = Math.max(MIN_RESUME_MS, deadline - Date.now());
  });
  el.addEventListener("mouseleave", () => {
    if (gone || timer !== undefined) return;
    deadline = Date.now() + remaining;
    timer = window.setTimeout(dismiss, remaining);
  });

  // Past the cap the oldest goes.
  for (const old of live(h).slice(0, Math.max(0, live(h).length - CAP + 1))) old.remove();
  h.appendChild(el);
}
