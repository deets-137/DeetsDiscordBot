// deetsmusic-ui: DeetsMusic's Now Playing, Queue, Search and History cards on its Mini, Midi
// and Max surfaces, for a page that is not the app (docs/features/ACTIVITY.md).
//
// It knows DeetsMusic's data and look. It knows nothing of Discord or of the relay: it talks
// to one Transport (deetsmusic-protocol). The page that mounts it owns <html> and <body>.

import "./look/src/styles.css";
import "./ui.css";
import type { Transport } from "../deetsmusic-protocol";
import { initHints } from "./hint";
import { initLayout } from "./layout";
import { initLook, paintLook } from "./look";
import { initAmbient, initStorm } from "./scenery";
import { makeStore } from "./store";
import { initSurface, setTile } from "./surface";

export interface DeetsMusicUI {
  /** Show the tile card alone (Discord's picture-in-picture and grid tiles), or the full UI again. */
  setTile(tile: boolean): void;
}

export function mountDeetsMusicUI(transport: Transport, root: HTMLElement = document.body): DeetsMusicUI {
  const html = document.documentElement;
  // DeetsMusic's Max layout with two cards beside the stage, the stage on the left.
  html.dataset.maxCards = "2cols";
  html.dataset.stageSide = "left";

  initLook();
  initSurface();
  const store = makeStore(transport);
  store.watch((s) => paintLook(s.theme, s.skin));
  initLayout(root, store);
  initStorm(); // after the layout: the storm's layer is in its shell
  initAmbient();
  initHints();
  return { setTile };
}

export type { CardId } from "./cards";
