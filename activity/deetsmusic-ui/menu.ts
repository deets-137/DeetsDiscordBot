// A small menu that hangs at a point: DeetsMusic's `.pop` surface with `.flyout__item` rows
// (the title picker's markup). A right-click or a long press on a row opens it, so the same
// actions work with a mouse and on a touchscreen.

import { on, type Undo } from "./dom";

export interface MenuItem {
  label: string;
  run: () => void;
  /** Shown greyed, with the reason as its hint. */
  disabled?: string;
}

let open: { el: HTMLElement; undo: Undo[] } | null = null;

export function closeMenu(): void {
  if (!open) return;
  open.undo.forEach((u) => u());
  open.el.remove();
  open = null;
}

export function openMenu(x: number, y: number, items: MenuItem[]): void {
  closeMenu();
  if (!items.length) return;
  const el = document.createElement("div");
  el.className = "slot-picker__menu pop dm-menu";
  el.setAttribute("role", "menu");
  for (const item of items) {
    const b = document.createElement("button");
    b.className = "flyout__item";
    b.type = "button";
    b.setAttribute("role", "menuitem");
    b.textContent = item.label;
    if (item.disabled) {
      b.disabled = true;
      b.title = item.disabled;
    }
    b.addEventListener("click", () => {
      closeMenu();
      item.run();
    });
    el.appendChild(b);
  }
  document.body.appendChild(el);
  // Keep it inside the frame.
  const r = el.getBoundingClientRect();
  el.style.left = `${Math.max(4, Math.min(x, window.innerWidth - r.width - 4))}px`;
  el.style.top = `${Math.max(4, Math.min(y, window.innerHeight - r.height - 4))}px`;

  const undo: Undo[] = [
    on(document, "pointerdown", (e) => {
      if (!el.contains(e.target as Node)) closeMenu();
    }, { capture: true }),
    on(document, "keydown", (e) => {
      if (e.key === "Escape") closeMenu();
    }),
    on(window, "resize" as keyof HTMLElementEventMap, closeMenu),
  ];
  open = { el, undo };
  el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
}

const LONG_PRESS_MS = 450;
const MOVE_SLOP = 10;

/** Call `open(row, x, y)` on a right-click or a long press on an element that matches `selector`. */
export function rowMenu(host: HTMLElement, selector: string, openFor: (row: HTMLElement, x: number, y: number) => void): Undo {
  let timer = 0;
  let start: { x: number; y: number } | null = null;
  let fired = false;
  const cancel = (): void => {
    window.clearTimeout(timer);
    start = null;
  };
  return ((undo: Undo[]) => () => undo.forEach((u) => u()))([
    on(host, "contextmenu", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>(selector);
      if (!row) return;
      e.preventDefault();
      if (!fired) openFor(row, e.clientX, e.clientY);
      fired = false;
    }),
    on(host, "pointerdown", (e) => {
      if (e.pointerType === "mouse") return;
      const row = (e.target as HTMLElement).closest<HTMLElement>(selector);
      if (!row) return;
      start = { x: e.clientX, y: e.clientY };
      fired = false;
      timer = window.setTimeout(() => {
        if (!start) return;
        fired = true;
        row.dataset.pressed = "";
        openFor(row, start.x, start.y);
        start = null;
      }, LONG_PRESS_MS);
    }),
    on(host, "pointermove", (e) => {
      if (start && Math.hypot(e.clientX - start.x, e.clientY - start.y) > MOVE_SLOP) cancel();
    }),
    on(host, "pointerup", cancel),
    on(host, "pointercancel", cancel),
    // The tap that ends a long press must not also act on the row.
    on(host, "click", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>(selector);
      if (row && "pressed" in row.dataset) {
        delete row.dataset.pressed;
        e.stopPropagation();
        e.preventDefault();
      }
    }, { capture: true }),
  ]);
}
