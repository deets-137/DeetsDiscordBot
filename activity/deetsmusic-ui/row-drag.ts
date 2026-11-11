// Row drag: reorder a row inside its own list. A port of the reorder half of DeetsMusic's
// src/row-drag.ts; the look draws the ghost, the dimmed source row and the insertion line.
//
// Whole-row press-and-drag: a click still acts; hold and move past DRAG_THRESHOLD px to drag.
// A translucent copy of the row (the ghost) follows the pointer on <body>; the pressed row
// stays in place and dims; an insertion line shows where the row lands. Rows are uniform
// height and flush, so the insertion index is row math on the list's scroll position. The
// card holds its re-renders while a drag runs (`active()`). Escape cancels.
//
// Left out of the app's module: the carry from card to card (the protocol has no "add at
// this place"), the hold, measure and sideways modes, and the frame telemetry.
//
// A finger: hold, then move (the owner's call A, 2026-11-11; the app's `hold` mode is the
// precedent). A press held still for HOLD_MS lifts the row. A move after that drags it. A
// release with no move is the long press: `onHoldRelease` (the card opens the row's menu). A
// move before the hold ends is a scroll, and the press is dropped whole.

const DRAG_THRESHOLD = 6;
const HOLD_MS = 450; // the long press of menu.ts
const HOLD_SLOP = 10; // a finger is never quite still

export interface DragRow {
  row: HTMLElement;
  /** The row's position in the list (its data-idx). */
  index: number;
  /** The element that holds the rows. */
  list: HTMLElement;
  /** How many rows the list has. */
  count: number;
}

export interface RowDragOptions {
  /** Where pointerdown is heard (delegated, so it survives re-renders). */
  root: HTMLElement;
  /** The draggable row under a press, or null when a drag is not allowed there now. */
  rowAt: (target: HTMLElement) => DragRow | null;
  /** The drag is over and the row styles are cleared. `to` is splice semantics (remove at
   *  `from`, insert at `to`); null for a cancel or a drop in place. */
  onEnd: (from: number, to: number | null) => void;
  /** A finger held a row still and let go with no move: the long press. */
  onHoldRelease?: (row: HTMLElement, x: number, y: number) => void;
}

export interface RowDrag {
  /** A drag is in progress (past the threshold). */
  active(): boolean;
  destroy(): void;
}

// The click that trails a drag's pointerup lands on whatever sits under the pointer. One
// capture listener swallows it. The next press clears a flag no click consumed.
let swallowClick = false;
window.addEventListener("pointerdown", () => (swallowClick = false), true);
window.addEventListener(
  "click",
  (e) => {
    if (!swallowClick) return;
    swallowClick = false;
    e.stopPropagation();
    e.preventDefault();
  },
  true,
);

/** The insertion slot for pointer `y` among `list`'s uniform, flush `[data-idx]` rows: `ins`
 *  in 0..count and the line's top in the list's content coordinates. */
function slotAt(list: HTMLElement, y: number, count: number): { ins: number; top: number } | null {
  const row = list.querySelector<HTMLElement>("[data-idx]");
  if (!row) return null;
  const lr = list.getBoundingClientRect();
  const rowH = row.offsetHeight || 1;
  const rowTop = row.getBoundingClientRect().top - lr.top - list.clientTop + list.scrollTop;
  const firstTop = rowTop - Number(row.dataset.idx) * rowH;
  const contentY = y - lr.top - list.clientTop + list.scrollTop;
  const ins = Math.max(0, Math.min(count, Math.round((contentY - firstTop) / rowH)));
  return { ins, top: firstTop + ins * rowH };
}

/** The element that really scrolls for this list: the list is a plain block inside the card's
 *  scroller, so the edge auto-scroll walks up to the box that moves. */
function scrollerOf(list: HTMLElement): HTMLElement {
  for (let el: HTMLElement | null = list; el && el !== document.body; el = el.parentElement) {
    if (el.scrollHeight > el.clientHeight + 1) return el;
  }
  return list;
}

const inside = (el: HTMLElement, x: number, y: number): boolean => {
  const r = el.getBoundingClientRect();
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
};

/** The ghost: a copy of the row on <body>. Card styles are scoped by ancestors (`.qcard .qrow`),
 *  so the copy sits inside shells that repeat the row's ancestor chain with `display: contents`:
 *  the selectors and inherited tokens still match, and the shells draw no box. */
function makeGhost(row: HTMLElement): { root: HTMLElement; ghost: HTMLElement } {
  const chain: HTMLElement[] = [];
  for (let a = row.parentElement; a && a !== document.body; a = a.parentElement) chain.unshift(a);
  let root: HTMLElement | null = null;
  let parent: HTMLElement | null = null;
  for (const a of chain) {
    const shell = document.createElement(a.tagName);
    for (const at of Array.from(a.attributes)) if (at.name === "class" || at.name.startsWith("data-")) shell.setAttribute(at.name, at.value);
    shell.classList.remove("is-reordering");
    shell.classList.add("drag-shell"); // the look switches a shell's ::before / ::after off
    shell.style.setProperty("display", "contents", "important");
    shell.setAttribute("aria-hidden", "true");
    if (parent) parent.appendChild(shell);
    else root = shell;
    parent = shell;
  }
  const r = row.getBoundingClientRect();
  const ghost = row.cloneNode(true) as HTMLElement;
  ghost.classList.remove("is-drag-source");
  ghost.classList.add("drag-ghost");
  ghost.removeAttribute("tabindex");
  ghost.setAttribute("aria-hidden", "true");
  ghost.style.position = "fixed";
  ghost.style.pointerEvents = "none";
  ghost.style.left = `${r.left}px`;
  ghost.style.top = `${r.top}px`;
  ghost.style.width = `${r.width}px`;
  ghost.style.height = `${r.height}px`;
  if (parent) parent.appendChild(ghost);
  else root = ghost;
  document.body.appendChild(root!);
  return { root: root!, ghost };
}

export function rowDrag(opts: RowDragOptions): RowDrag {
  type Drag = {
    src: DragRow;
    startX: number;
    startY: number;
    lastX: number;
    lastY: number;
    ghostRoot: HTMLElement;
    ghost: HTMLElement;
    line: HTMLElement;
    lineOn: boolean;
    reordering: boolean; // the pointer is inside the list
    to: number;
    raf: number;
  };
  let drag: Drag | null = null;
  let pending: (DragRow & { startX: number; startY: number; touch: boolean; x: number; y: number }) | null = null;
  // A finger's hold: the timer, and whether it ran its time (the row is lifted).
  let holdTimer = 0;
  let armed = false;
  const holdOff = (): void => {
    window.clearTimeout(holdTimer);
    holdTimer = 0;
    armed = false;
    pending?.row.classList.remove("is-context");
  };
  // Once the row is lifted the finger's moves are the drag, not the list's scroll. The browser
  // asks through `touchmove`; a pointer event cannot refuse a scroll.
  const onTouchMove = (e: TouchEvent): void => {
    if ((armed || drag) && e.cancelable) e.preventDefault();
  };
  // A phone's own long press also sends `contextmenu`: the hold owns the press.
  const onContext = (e: Event): void => {
    if (!pending?.touch && !drag) return;
    e.preventDefault();
    e.stopPropagation();
  };
  // Auto-scroll near the list's edge: the look's --drag-scroll-edge (px) and
  // --drag-scroll-speed (px per second). Read once per drag.
  let edge = 28;
  let speed = 540;
  let tick = 0;
  let carry = 0;

  const placeLine = (d: Drag, top: number | null): void => {
    if (top === null) {
      d.line.remove();
      d.lineOn = false;
      return;
    }
    if (!d.lineOn) {
      d.src.list.classList.add("is-reordering"); // positioned: the line's reference
      d.src.list.appendChild(d.line);
      d.lineOn = true;
    }
    d.line.style.top = `${top}px`;
  };

  const update = (): void => {
    const d = drag;
    if (!d) return;
    d.ghost.style.setProperty("--ghost-x", `${d.lastX - d.startX}px`);
    d.ghost.style.setProperty("--ghost-y", `${d.lastY - d.startY}px`);
    const { src } = d;
    d.reordering = src.list.isConnected && inside(scrollerOf(src.list), d.lastX, d.lastY);
    const s = d.reordering ? slotAt(src.list, d.lastY, src.count) : null;
    if (s) {
      d.to = s.ins <= src.index ? s.ins : s.ins - 1;
      placeLine(d, s.top);
    } else {
      d.to = src.index;
      placeLine(d, null);
    }
    // Outside the list there is nowhere to drop: the ghost and the pointer say so.
    d.ghost.classList.toggle("is-nodrop", !d.reordering);
    document.documentElement.classList.toggle("is-drag-nodrop", !d.reordering);
  };

  const autoScroll = (now: number): void => {
    const d = drag;
    if (!d) return;
    // A long frame scrolls at most 50 ms worth, not a jump.
    carry += (speed * Math.min(50, tick ? now - tick : 0)) / 1000;
    tick = now;
    const step = Math.trunc(carry);
    carry -= step;
    const el = scrollerOf(d.src.list);
    if (d.reordering && el.isConnected && step) {
      const r = el.getBoundingClientRect();
      const dy = d.lastY < r.top + edge ? -step : d.lastY > r.bottom - edge ? step : 0;
      if (dy) {
        el.scrollTop += dy;
        update();
      }
    }
    d.raf = requestAnimationFrame(autoScroll);
  };

  const begin = (): void => {
    if (!pending) return;
    const p = pending;
    holdOff();
    pending = null;
    const { root, ghost } = makeGhost(p.row);
    p.row.classList.add("is-drag-source");
    document.documentElement.classList.add("is-row-dragging");
    window.getSelection()?.removeAllRanges();
    const line = document.createElement("div");
    line.className = "drop-line";
    drag = { src: p, startX: p.startX, startY: p.startY, lastX: p.startX, lastY: p.startY, ghostRoot: root, ghost, line, lineOn: false, reordering: false, to: p.index, raf: 0 };
    const cs = getComputedStyle(document.documentElement);
    edge = parseFloat(cs.getPropertyValue("--drag-scroll-edge")) || 28;
    speed = parseFloat(cs.getPropertyValue("--drag-scroll-speed")) || 540;
    tick = 0;
    carry = 0;
    update();
    drag.raf = requestAnimationFrame(autoScroll);
  };

  const onMove = (e: PointerEvent): void => {
    if (drag) {
      drag.lastX = e.clientX;
      drag.lastY = e.clientY;
      update();
      e.preventDefault(); // no text selection while dragging
    } else if (pending) {
      pending.x = e.clientX;
      pending.y = e.clientY;
      const moved = Math.hypot(e.clientX - pending.startX, e.clientY - pending.startY);
      if (pending.touch && !armed) {
        // A move before the hold ends is a scroll or a miss: drop the press whole.
        if (moved > HOLD_SLOP) {
          holdOff();
          pending = null;
          unlisten();
        }
      } else if (moved > (pending.touch ? HOLD_SLOP : DRAG_THRESHOLD)) {
        begin();
      }
    }
  };
  const onUp = (): void => {
    // A finger that held the row still and let go: the long press.
    const p = pending;
    if (p?.touch && armed && !drag) {
      swallowClick = true;
      end(false);
      opts.onHoldRelease?.(p.row, p.x, p.y);
      return;
    }
    end(true);
  };
  const onCancel = (): void => end(false);
  const onKey = (e: KeyboardEvent): void => {
    if (e.key !== "Escape" || !drag) return;
    e.preventDefault();
    e.stopPropagation(); // the Escape ends the drag, not a card's search or menu
    end(false);
  };

  const listen = (): void => {
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
    document.addEventListener("pointercancel", onCancel);
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("touchmove", onTouchMove, { passive: false });
    document.addEventListener("contextmenu", onContext, true);
  };
  const unlisten = (): void => {
    document.removeEventListener("touchmove", onTouchMove);
    document.removeEventListener("contextmenu", onContext, true);
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerup", onUp);
    document.removeEventListener("pointercancel", onCancel);
    document.removeEventListener("keydown", onKey, true);
  };

  function end(commit: boolean): void {
    unlisten();
    holdOff();
    pending = null;
    if (!drag) return; // never crossed the threshold: it was a click
    const d = drag;
    drag = null;
    cancelAnimationFrame(d.raf);
    d.src.row.classList.remove("is-drag-source");
    d.ghostRoot.remove();
    placeLine(d, null);
    d.src.list.classList.remove("is-reordering");
    document.documentElement.classList.remove("is-row-dragging", "is-drag-nodrop");
    swallowClick = true;
    const to = commit && d.reordering && d.to !== d.src.index ? d.to : null;
    opts.onEnd(d.src.index, to);
  }

  const onDown = (e: PointerEvent): void => {
    if (e.button !== 0 || drag || pending) return; // the main button only, one drag at a time
    const hit = opts.rowAt(e.target as HTMLElement);
    if (!hit) return;
    const touch = e.pointerType === "touch";
    pending = { ...hit, startX: e.clientX, startY: e.clientY, x: e.clientX, y: e.clientY, touch };
    listen();
    if (touch) {
      holdTimer = window.setTimeout(() => {
        holdTimer = 0;
        if (!pending) return;
        armed = true;
        pending.row.classList.add("is-context"); // the look's outline: the row is lifted
        navigator.vibrate?.(10);
      }, HOLD_MS);
    }
  };
  opts.root.addEventListener("pointerdown", onDown);

  return {
    active: () => drag != null,
    destroy() {
      if (drag) end(false); // destroyed mid-drag: the ghost and the listeners must not outlive the card
      holdOff();
      pending = null;
      unlisten();
      opts.root.removeEventListener("pointerdown", onDown);
    },
  };
}
