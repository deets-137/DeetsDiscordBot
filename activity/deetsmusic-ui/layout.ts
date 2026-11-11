// The layout: DeetsMusic's page skeleton (its index.html: the title bar, the bento, the stage
// column, the slots) and which card sits in which slot on each surface (ACTIVITY.md §3).
//
//   Max   Now Playing over the Queue (the stage column), Search and History beside them
//         (DeetsMusic's "2cols" layout for Max, as its Rooms web port uses).
//   Midi  the Now Playing strip, then two slots. Each slot's title picks its card.
//   Mini  Now Playing and one slot; or the player (Now Playing alone); or the cover alone.
//
// The look hides the slots a surface does not use, so this file only decides what to mount.

import type { CardCtx, CardDef, CardId } from "./cards";
import type { Undo } from "./dom";
import { historyCard } from "./history-card";
import { nowPlayingCard } from "./now-playing-card";
import { queueCard } from "./queue-card";
import { searchCard } from "./search-card";
import type { Store } from "./store";
import { onSurface, setMiniView, surfaceState, type SurfaceName, type SurfaceState } from "./surface";

type Slot = "np" | "queue" | "left" | "right";
type Pickable = Exclude<CardId, "now-playing">;

const CARDS: Record<CardId, CardDef> = {
  "now-playing": nowPlayingCard,
  queue: queueCard,
  search: searchCard,
  history: historyCard,
};
const PICKABLE: Pickable[] = ["queue", "search", "history"];

const SHELL = `
  <header class="titlebar dm-activity__bar">
    <div class="settings"><span class="app-title"><span class="app-title__full">DeetsMusic</span><span class="app-title__short" aria-hidden="true">DM</span></span></div>
    <div class="drag-region"></div>
    <div class="chrome-right"><span class="dm-activity__who" id="dm-who"></span></div>
  </header>
  <main class="app-body">
    <div class="bento" data-screen="home">
      <div class="stage">
        <section class="panel" data-slot="np" data-span="wide"></section>
        <section class="panel" data-slot="queue"></section>
      </div>
      <section class="panel" data-slot="left"></section>
      <section class="panel" data-slot="right"></section>
    </div>
  </main>`;

/** The two pickable slots' cards on Midi, and Mini's one. Kept for the visit, per surface. */
const picks: Record<"midi" | "mini", { left: Pickable; right: Pickable }> = {
  midi: { left: "queue", right: "search" },
  mini: { left: "queue", right: "search" },
};

function want(s: SurfaceState): Partial<Record<Slot, CardId>> {
  if (s.cover || (s.surface === "mini" && s.mini === "player")) return { np: "now-playing" };
  switch (s.surface) {
    case "max":
      return { np: "now-playing", queue: "queue", left: "search", right: "history" };
    case "midi":
      return { np: "now-playing", left: picks.midi.left, right: picks.midi.right };
    case "mini":
      return { np: "now-playing", left: picks.mini.left };
  }
}

export function initLayout(root: HTMLElement, store: Store): void {
  root.insertAdjacentHTML("afterbegin", SHELL);
  const panel = (slot: Slot): HTMLElement => root.querySelector<HTMLElement>(`.panel[data-slot="${slot}"]`)!;
  const mounted: Partial<Record<Slot, { id: CardId; undo: Undo }>> = {};

  const ctx: CardCtx = { store, requestCard };

  function unmount(slot: Slot): void {
    mounted[slot]?.undo();
    delete mounted[slot];
    const p = panel(slot);
    p.innerHTML = "";
    delete p.dataset.mounted;
  }

  function mount(slot: Slot, id: CardId, pickable: boolean): void {
    const p = panel(slot);
    const def = CARDS[id];
    p.dataset.mounted = id;
    let undoHead: Undo = () => {};
    if (id !== "now-playing") {
      p.innerHTML = `<header class="panel__head"><h2 class="panel__title${pickable ? " is-pickable" : ""}"${pickable ? ` aria-haspopup="true" tabindex="0" title="Picks the card in this place"` : ""}>${def.title}</h2></header>`;
      if (pickable) undoHead = picker(p, slot as "left" | "right", id as Pickable);
    }
    const undoCard = def.mount(p, ctx);
    mounted[slot] = { id, undo: () => (undoHead(), undoCard()) };
  }

  /** The title picker, as DeetsMusic's: the title opens a menu of the cards for this place. */
  function picker(p: HTMLElement, slot: "left" | "right", current: Pickable): Undo {
    const head = p.querySelector<HTMLElement>(".panel__head")!;
    const title = head.querySelector<HTMLElement>(".panel__title")!;
    const menu = document.createElement("div");
    menu.className = "slot-picker__menu pop";
    menu.setAttribute("role", "menu");
    menu.hidden = true;
    menu.innerHTML = PICKABLE.map(
      (id) => `<button class="flyout__item" type="button" role="menuitemradio" data-card-id="${id}" aria-checked="${id === current}">${CARDS[id].title}</button>`,
    ).join("");
    head.appendChild(menu);

    const close = (): void => void (menu.hidden = true);
    const toggle = (): void => void (menu.hidden = !menu.hidden);
    const onTitle = (e: Event): void => {
      if (e instanceof KeyboardEvent && e.key !== "Enter" && e.key !== " ") return;
      e.preventDefault();
      toggle();
    };
    const onPick = (e: Event): void => {
      const id = (e.target as HTMLElement).closest<HTMLElement>("[data-card-id]")?.dataset.cardId as Pickable | undefined;
      close();
      if (id && id !== current) place(slot, id);
    };
    const onAway = (e: Event): void => {
      if (!head.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") close();
    };
    title.addEventListener("click", onTitle);
    title.addEventListener("keydown", onTitle);
    menu.addEventListener("click", onPick);
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
    if (s.surface === "max") return;
    const set = picks[s.surface];
    const other = slot === "left" ? "right" : "left";
    if (s.surface === "midi" && set[other] === id) set[other] = set[slot];
    set[slot] = id;
    apply(s);
  }

  /** Bring a card on screen: Now Playing's Queue and Search squares. */
  function requestCard(id: CardId): void {
    if (id === "now-playing") return;
    const s = surfaceState();
    if (s.cover) return;
    if (s.surface === "mini") {
      picks.mini.left = id;
      if (s.mini === "player") setMiniView("cards"); // the surface change mounts it
      else apply(s);
    } else if (s.surface === "midi" && picks.midi.left !== id && picks.midi.right !== id) {
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

  let lastSurface: SurfaceName | null = null;
  function apply(s: SurfaceState): void {
    const next = want(s);
    const pickable = s.surface !== "max";
    for (const slot of ["np", "queue", "left", "right"] as Slot[]) {
      const id = next[slot];
      const cur = mounted[slot];
      // A slot keeps its card over a surface flip unless the card or its head changes.
      const headChanged = !!cur && cur.id !== "now-playing" && lastSurface !== null && (lastSurface === "max") !== (s.surface === "max");
      if (cur && (cur.id !== id || headChanged)) unmount(slot);
      if (id && !mounted[slot]) mount(slot, id, pickable && slot !== "np" && slot !== "queue");
    }
    lastSurface = s.surface;
  }

  onSurface(apply);
  apply(surfaceState());

  // Who this person is to the host, at the right of the title bar.
  const who = root.querySelector<HTMLElement>("#dm-who")!;
  store.watch((s) => {
    who.textContent = !s.active ? "" : s.you.host ? "Host" : "Guest";
    who.title = s.you.host ? "You are the host" : "What you can do here follows DeetsMusic › Settings › Rooms";
  });
}
