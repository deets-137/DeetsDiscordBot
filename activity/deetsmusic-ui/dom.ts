// Small DOM helpers shared by the cards.

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

/** Escape text for an HTML string. Every name from the host or from Discord passes through it. */
export const esc = (s: string): string => s.replace(/[&<>"']/g, (c) => ESC[c]);

/** `3:07` from milliseconds. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** The time of day a song played, as DeetsMusic's History writes it (`6:23 PM`). */
export function timeOfDay(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** DeetsMusic's explicit mark (library-card.ts `EXPLICIT_SIGIL`): a filled square with an E. */
const EXPLICIT_SIGIL =
  `<svg class="lib-src-badge lib-src-badge--explicit" viewBox="0 0 24 24" role="img" aria-label="Explicit">` +
  `<rect x="3" y="3" width="18" height="18" rx="3"/>` +
  `<path class="lib-src-badge__letter" d="M9 7h6v2h-4v2h3.5v2H11v2h4v2H9z"/></svg>`;
export const explicitBadge = (rating?: string): string => (rating === "explicit" ? EXPLICIT_SIGIL : "");

/** A unit of work that a card undoes when it leaves its slot. */
export type Undo = () => void;

/** Listen, and get back the way to stop. */
export function on<K extends keyof HTMLElementEventMap>(
  el: HTMLElement | Document | Window,
  type: K,
  fn: (e: HTMLElementEventMap[K]) => void,
  opts?: AddEventListenerOptions,
): Undo {
  el.addEventListener(type, fn as EventListener, opts);
  return () => el.removeEventListener(type, fn as EventListener, opts);
}
