// The layout: DeetsMusic's page skeleton (its index.html: the title bar, the bento, the stage
// column, the slots) and which card sits in which slot on each surface (ACTIVITY.md §3).
//
//   Max   Now Playing over the Queue (the stage column), Search and History beside them
//         (DeetsMusic's "2cols" layout for Max, as its Rooms web port uses). When the stage
//         shows 5 Up Next rows or fewer, the Queue's title offers a swap with History: the
//         Queue takes History's tall place and History goes under Now Playing (his ask,
//         2026-11-11; ACTIVITY.md §14.6).
//   Midi  the Now Playing strip, then two slots. Each slot's title picks its card.
//   Mini  Now Playing and one slot; or the player (Now Playing alone).
//   Wide  (a short, wide frame) the player on the left, one slot on the right.
//   Tile  (picture-in-picture, grid) Now Playing alone, drawn as the tile card.
//
// On Midi and Max, Now Playing opens the full player (full-player.ts) and the cards grow
// (grow.ts), as in the app. The look hides the slots a surface does not use, so this file only
// decides what to mount.

import type { CardCtx, CardDef, CardId } from "./cards";
import type { Undo } from "./dom";
import { closeFullPlayer, fullPlayerBusyOrOpen, initFullPlayer, isFullPlayer, onFullPlayerChange, paintZones, resetSide } from "./full-player";
import { collapseGrow, grownState, growSurfaceChanged, initGrow, isCovered, onGrowChange, refreshGrow, type Slot } from "./grow";
import { historyCard } from "./history-card";
import { sizeDrum } from "./metal";
import { nowPlayingCard } from "./now-playing-card";
import { queueCard, rowsShown } from "./queue-card";
import { searchCard } from "./search-card";
import type { Store } from "./store";
import { onSurface, setMiniView, surfaceState, type SurfaceState } from "./surface";

type Pickable = Exclude<CardId, "now-playing">;

const CARDS: Record<CardId, CardDef> = {
  "now-playing": nowPlayingCard,
  queue: queueCard,
  search: searchCard,
  history: historyCard,
};
const PICKABLE: Pickable[] = ["queue", "search", "history"];
/** The swap is offered when the stage shows this many Up Next rows or fewer. */
const SWAP_AT_ROWS = 5;

const SHELL = `
  <header class="titlebar dm-activity__bar">
    <div class="settings"><span class="app-title"><span class="app-title__full">DeetsMusic</span><span class="app-title__short" aria-hidden="true">DM</span></span></div>
    <div class="drag-region"></div>
    <div class="chrome-right"><span class="dm-activity__who" id="dm-who"></span></div>
  </header>
  <main class="app-body">
    <div class="ocean" aria-hidden="true">
      <div class="ocean__heave ocean__heave--far"><div class="ocean__bob ocean__bob--far"><div class="ocean__train ocean__train--far"><i class="ocean__neon"></i><i class="ocean__neon"></i></div></div></div>
      <div class="ocean__heave ocean__heave--mid"><div class="ocean__bob ocean__bob--mid"><div class="ocean__train ocean__train--mid"><i class="ocean__neon"></i><i class="ocean__neon"></i></div></div></div>
      <div class="ocean__heave ocean__heave--near"><div class="ocean__bob ocean__bob--near"><div class="ocean__train ocean__train--near"><i class="ocean__neon"></i><i class="ocean__neon"></i></div></div></div>
      <div class="ocean__glow"></div>
      <div class="ocean__ripples"></div>
    </div>
    <div class="wallpaper" aria-hidden="true"></div>
    <div class="aurora" aria-hidden="true">
      <div class="aurora__blob aurora__blob--1"></div>
      <div class="aurora__blob aurora__blob--2"></div>
      <div class="aurora__blob aurora__blob--3"></div>
    </div>
    <div class="storm" aria-hidden="true">
      <div class="storm__strike storm__strike--1"><div class="storm__hold">
        <svg class="storm__svg" viewBox="0 0 100 100" preserveAspectRatio="none"><path class="storm__bolt"></path></svg>
      </div></div>
      <div class="storm__strike storm__strike--2"><div class="storm__hold">
        <svg class="storm__svg" viewBox="0 0 100 100" preserveAspectRatio="none"><path class="storm__bolt"></path></svg>
      </div></div>
    </div>
    <div class="scene scene--back" aria-hidden="true">
      <div class="scene__sky scene__sky--from"></div>
      <div class="scene__sky scene__sky--to"></div>
    </div>
    <div class="bento" data-screen="home">
      <div class="stage">
        <section class="panel" data-slot="np" data-span="wide"></section>
        <section class="panel" data-slot="queue"></section>
      </div>
      <section class="panel" data-slot="left"></section>
      <section class="panel" data-slot="right"></section>
      <section class="panel fp-history" aria-label="Beside the full player"></section>
    </div>
    <div class="scene scene--front" aria-hidden="true">
      <div class="scene__sun"><i class="scene__rays"></i><i class="scene__disc"></i><i class="scene__moon"></i></div>
      <div class="scene__grass"></div>
    </div>
    <div class="scene scene--actors" aria-hidden="true">
      <div class="scene__floor"></div>
    </div>
  </main>`;

/** The two pickable slots' cards on Midi, and the one on Mini and Wide. Kept for the visit,
 *  per composition. */
type Place = "midi" | "mini" | "wide";
const picks: Record<Place, { left: Pickable; right: Pickable }> = {
  midi: { left: "queue", right: "search" },
  mini: { left: "queue", right: "search" },
  wide: { left: "queue", right: "search" },
};
/** Max: the Queue and History have changed places. Kept for the visit. */
let maxSwap = false;
/** Which set of picks the frame uses now. Max and the tile have none. */
const placeOf = (s: SurfaceState): Place | null => (s.tile ? null : s.wide ? "wide" : s.surface === "max" ? null : s.surface);
const isMax = (s: SurfaceState): boolean => !s.tile && !s.wide && s.surface === "max";

function want(s: SurfaceState): Partial<Record<Slot, CardId>> {
  if (s.tile) return { np: "now-playing" };
  if (s.wide) return { np: "now-playing", left: picks.wide.left };
  if (s.surface === "mini" && s.mini === "player") return { np: "now-playing" };
  switch (s.surface) {
    case "max":
      return { np: "now-playing", queue: maxSwap ? "history" : "queue", left: "search", right: maxSwap ? "queue" : "history" };
    case "midi":
      return { np: "now-playing", left: picks.midi.left, right: picks.midi.right };
    case "mini":
      return { np: "now-playing", left: picks.mini.left };
  }
}

export function initLayout(root: HTMLElement, store: Store): void {
  root.insertAdjacentHTML("afterbegin", SHELL);
  const panel = (slot: Slot): HTMLElement => root.querySelector<HTMLElement>(`.panel[data-slot="${slot}"]`)!;
  const hosts: Record<Slot, HTMLElement> = { np: panel("np"), queue: panel("queue"), left: panel("left"), right: panel("right") };
  const sideHost = root.querySelector<HTMLElement>(".panel.fp-history")!;
  const bento = root.querySelector<HTMLElement>(".bento")!;
  const mounted: Partial<Record<Slot, { id: CardId; undo: Undo }>> = {};

  const ctx: CardCtx = { store, requestCard };

  function unmount(slot: Slot): void {
    mounted[slot]?.undo();
    delete mounted[slot];
    const p = panel(slot);
    p.innerHTML = "";
    delete p.dataset.mounted;
  }

  /** A card's header: its title, and the way the title picks (or swaps) the card. */
  type Head = "plain" | "pick" | "swap";
  function headFor(slot: Slot, id: CardId, s: SurfaceState): Head {
    if (id === "now-playing") return "plain";
    if (isMax(s) && (slot === "queue" || slot === "right") && (id === "queue" || id === "history")) return "swap";
    return placeOf(s) !== null && (slot === "left" || slot === "right") ? "pick" : "plain";
  }

  function mount(slot: Slot, id: CardId, head: Head): void {
    const p = panel(slot);
    const def = CARDS[id];
    p.dataset.mounted = id;
    let undoHead: Undo = () => {};
    if (id !== "now-playing") {
      const pickable = head === "pick";
      p.innerHTML = `<header class="panel__head"><h2 class="panel__title${pickable ? " is-pickable" : ""}"${pickable ? ` aria-haspopup="true" tabindex="0" title="Picks the card in this place"` : ""}>${def.title}</h2></header>`;
      if (pickable) undoHead = picker(p, PICKABLE, id, (next) => place(slot as "left" | "right", next as Pickable));
      else if (head === "swap") undoHead = picker(p, ["queue", "history"], id, () => swap(), swapOffered);
    }
    const undoCard = def.mount(p, ctx);
    mounted[slot] = { id, undo: () => (undoHead(), undoCard()) };
  }

  /** The title picker, as DeetsMusic's: the title opens a menu of the cards for this place.
   *  `live`: whether the title acts now (the swap is offered only when the Queue is short). */
  function picker(p: HTMLElement, ids: Pickable[], current: CardId, onPick: (id: Pickable) => void, live: () => boolean = () => true): Undo {
    const head = p.querySelector<HTMLElement>(".panel__head")!;
    const title = head.querySelector<HTMLElement>(".panel__title")!;
    const menu = document.createElement("div");
    menu.className = "slot-picker__menu pop";
    menu.setAttribute("role", "menu");
    menu.hidden = true;
    menu.innerHTML = ids
      .map((id) => `<button class="flyout__item" type="button" role="menuitemradio" data-card-id="${id}" aria-checked="${id === current}">${CARDS[id].title}</button>`)
      .join("");
    head.appendChild(menu);

    const close = (): void => void (menu.hidden = true);
    const toggle = (): void => void (menu.hidden = !menu.hidden);
    const onTitle = (e: Event): void => {
      if (!live()) return;
      if (e instanceof KeyboardEvent && e.key !== "Enter" && e.key !== " ") return;
      e.preventDefault();
      toggle();
    };
    const onPickEv = (e: Event): void => {
      const id = (e.target as HTMLElement).closest<HTMLElement>("[data-card-id]")?.dataset.cardId as Pickable | undefined;
      close();
      if (id && id !== current) onPick(id);
    };
    const onAway = (e: Event): void => {
      if (!head.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape" && !menu.hidden) {
        close();
        e.preventDefault();
      }
    };
    title.addEventListener("click", onTitle);
    title.addEventListener("keydown", onTitle);
    menu.addEventListener("click", onPickEv);
    document.addEventListener("pointerdown", onAway, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onAway, true);
      document.removeEventListener("keydown", onKey);
    };
  }

  /** Put `id` in `slot` on the current surface. A card already in the other slot swaps places. */
  function place(slot: "left" | "right", id: Pickable): void {
    const s = surfaceState();
    const where = placeOf(s);
    if (!where) return;
    const set = picks[where];
    const other = slot === "left" ? "right" : "left";
    if (where === "midi" && set[other] === id) set[other] = set[slot];
    set[slot] = id;
    if (grownState()) void collapseGrow("pick", false);
    apply(s);
  }

  // ── Max: the Queue and History change places ────────────────────────────────
  let queueShort = false;
  /** The swap is offered: the stage's Queue shows 5 rows or fewer, or the two have swapped. */
  const swapOffered = (): boolean => maxSwap || queueShort;
  function swap(): void {
    maxSwap = !maxSwap;
    console.info(`[deetsmusic-ui] layout: ${maxSwap ? "the Queue takes History's place" : "the Queue is back under Now Playing"}`);
    if (grownState()) void collapseGrow("swap", false);
    apply(surfaceState());
    resetSide();
  }
  /** Mark the swap titles: a pickable look and a hint only while the swap is offered. */
  function paintSwap(): void {
    for (const slot of ["queue", "right"] as Slot[]) {
      const title = panel(slot).querySelector<HTMLElement>(":scope > .panel__head .panel__title");
      const cur = mounted[slot]?.id;
      if (!title || !isMax(surfaceState()) || (cur !== "queue" && cur !== "history")) continue;
      const on = swapOffered();
      title.classList.toggle("is-pickable", on);
      if (on) {
        title.setAttribute("aria-haspopup", "true");
        title.tabIndex = 0;
        title.dataset.hint = maxSwap ? "Swaps the Queue and History back" : "Swaps the Queue and History: the Queue gets the tall place";
      } else {
        title.removeAttribute("aria-haspopup");
        title.removeAttribute("tabindex");
        delete title.dataset.hint;
      }
    }
  }
  /** Measure what the stage's Queue shows (Max, at rest). */
  function measureQueue(): void {
    const s = surfaceState();
    if (!root.isConnected || !isMax(s) || maxSwap || grownState() || isFullPlayer() || mounted.queue?.id !== "queue") return;
    const host = root.querySelector<HTMLElement>('.panel[data-slot="queue"]');
    if (!host) return;
    const n = rowsShown(host);
    const short = n <= SWAP_AT_ROWS;
    if (short !== queueShort) {
      queueShort = short;
      console.info(`[deetsmusic-ui] layout: the stage's Queue shows ${n} rows; the swap with History is ${short ? "offered" : "not offered"}`);
    }
    paintSwap();
  }
  new ResizeObserver(() => measureQueue()).observe(panel("queue"));

  /** Bring a card on screen: Now Playing's Queue and Search squares. */
  function requestCard(id: CardId): void {
    if (id === "now-playing") return;
    const s = surfaceState();
    const slotOf = (): Slot | undefined => (Object.keys(mounted) as Slot[]).find((k) => mounted[k]?.id === id);
    // Under the full player or a grown card: that ends first (as the app's card request). Only
    // the stage card in Max stays in sight beside the player; any other card would be placed
    // under it, so the player closes (the break-it pass: on Midi, Search did nothing).
    const at = slotOf();
    if (isFullPlayer() && !(isMax(s) && at === "queue")) void closeFullPlayer("card");
    if (at && isCovered(at)) void collapseGrow("card");
    const where = placeOf(s);
    if (where === "wide") {
      picks.wide.left = id;
      apply(s);
    } else if (where === "mini") {
      picks.mini.left = id;
      if (s.mini === "player") setMiniView("cards"); // the surface change mounts it
      else apply(s);
    } else if (where === "midi" && picks.midi.left !== id && picks.midi.right !== id) {
      place("right", id);
    }
    focusCard(id);
  }

  function focusCard(id: CardId): void {
    const slot = (Object.keys(mounted) as Slot[]).find((k) => mounted[k]?.id === id);
    if (!slot) return;
    const p = panel(slot);
    (p.querySelector<HTMLElement>(".search__input") ?? p.querySelector<HTMLElement>(".panel__body"))?.focus();
  }

  const lastHead: Partial<Record<Slot, Head>> = {};
  function apply(s: SurfaceState): void {
    const next = want(s);
    for (const slot of ["np", "queue", "left", "right"] as Slot[]) {
      const id = next[slot];
      const cur = mounted[slot];
      const head = id ? headFor(slot, id, s) : "plain";
      // A slot keeps its card over a surface flip unless the card or its head changes.
      if (cur && (cur.id !== id || lastHead[slot] !== head)) unmount(slot);
      if (id && !mounted[slot]) mount(slot, id, head);
      lastHead[slot] = head;
    }
    refreshGrow();
    paintZones();
    paintSwap();
    requestAnimationFrame(measureQueue);
  }

  // The grow and the full player work on these hosts. A surface change ends both with no
  // motion before the slots move.
  initGrow({
    hosts,
    titleOf: (slot) => CARDS[mounted[slot]?.id ?? "now-playing"].title,
    // Also while the player opens (it waits for the large cover first): a grow started then
    // outlived the player and left two cards on top of each other.
    playerOpen: fullPlayerBusyOrOpen,
  });
  initFullPlayer({
    hosts,
    bento,
    side: sideHost,
    slots: () => (["left", "right"] as Slot[]).filter((sl) => !!mounted[sl]),
    onFlight: sizeDrum, // Metal's side covers fly with the cover at the full size's drum depth
    // Beside the full player (Max): the one of the Queue and History that is not in the stage.
    mountSide(host) {
      const id: CardId = maxSwap ? "queue" : "history";
      host.dataset.mounted = id;
      host.innerHTML = `<header class="panel__head"><h2 class="panel__title">${CARDS[id].title}</h2></header>`;
      const undo = CARDS[id].mount(host, ctx);
      return () => {
        undo();
        host.innerHTML = "";
        delete host.dataset.mounted;
      };
    },
  });
  onFullPlayerChange(() => {
    refreshGrow();
    if (!isFullPlayer()) requestAnimationFrame(measureQueue);
  });
  onGrowChange(() => {
    paintZones();
    if (!grownState()) requestAnimationFrame(measureQueue);
  });

  onSurface((s) => {
    if (isFullPlayer()) void closeFullPlayer("surface", false);
    growSurfaceChanged();
    apply(s);
  });
  apply(surfaceState());

  // Who this person is to the host, at the right of the title bar.
  const who = root.querySelector<HTMLElement>("#dm-who")!;
  store.watch((s) => {
    who.textContent = !s.active ? "" : s.you.host ? "Host" : "Guest";
    who.title = s.you.host ? "You are the host" : "What you can do here follows DeetsMusic › Settings › Rooms";
  });
  // Up Next's length does not change what fits, but the first row does (the probe stands in
  // until there is one): measure again when the list changes.
  store.watch(() => requestAnimationFrame(measureQueue));
}
