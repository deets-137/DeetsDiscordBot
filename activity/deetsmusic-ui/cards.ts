// What a card is here: the same idea as DeetsMusic's CardDef (src/cards.ts). A card mounts
// into a `.panel` host and gives back the way to leave it, so the layout can move cards
// between slots when the surface changes.

import type { Store } from "./store";
import type { Undo } from "./dom";

export type CardId = "now-playing" | "queue" | "search" | "history";

export interface CardCtx {
  store: Store;
  /** Bring a card on screen (Now Playing's Queue and Search squares, as in the app). */
  requestCard(id: CardId): void;
}

export interface CardDef {
  id: CardId;
  title: string;
  /** Fill `panel` (the layout has already written the head) and return the undo. */
  mount(panel: HTMLElement, ctx: CardCtx): Undo;
}
