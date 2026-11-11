// deetsmusic-ui: DeetsMusic's Now Playing, Queue, Search and History cards on its Mini, Midi
// and Max surfaces, for a page that is not the app (docs/features/ACTIVITY.md).
//
// It knows DeetsMusic's data and look. It knows nothing of Discord or of the relay: it talks
// to one Transport (deetsmusic-protocol). The page that mounts it owns <html> and <body>.

import "./look/src/styles.css";
import "./look/src/styles/pin.css"; // the cover-only view (DeetsMusic's pin › Cover)
import "./ui.css";
import type { Transport } from "../deetsmusic-protocol";
import { initLayout } from "./layout";
import { initLook, paintLook } from "./look";
import { makeStore } from "./store";
import { initSurface, setCover } from "./surface";

export interface DeetsMusicUI {
  /** Show the cover alone (Discord's picture-in-picture and grid tiles), or the full UI again. */
  setCover(cover: boolean): void;
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
  return { setCover };
}

export type { CardId } from "./cards";
