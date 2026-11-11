// The Queue card: what plays now, then Up Next. Markup as DeetsMusic's src/qcard.ts.
// A row's menu (right-click, or a long press) moves it to the top or removes it, under the
// host's "Guests may change Up Next" rule. Drag to reorder is not built yet.

import "./look/src/styles/qcard.css";
import type { CardDef } from "./cards";
import type { HostState } from "../deetsmusic-protocol";
import { rowMenu, openMenu } from "./menu";
import { nowHTML, rowHTML } from "./rows";

const HOST_ONLY = "The host has set “Guests may change Up Next” to host only in DeetsMusic";

export const queueCard: CardDef = {
  id: "queue",
  title: "Queue",
  mount(panel, ctx) {
    const { store } = ctx;
    const body = document.createElement("div");
    body.className = "panel__body qcard";
    body.tabIndex = 0;
    panel.appendChild(body);

    let shown = "";
    function paint(s: HostState): void {
      // Rewrite only when the list itself changed: a play or a pause must not reset the scroll.
      const key = `${s.active}|${s.now.track?.catalogId ?? ""}|${s.upNext.map((t) => t.catalogId).join(",")}`;
      if (key === shown) return;
      shown = key;
      const rows = s.upNext.map((t, i) => rowHTML(i, t, { key: `q${i}:${t.catalogId}` })).join("");
      body.innerHTML =
        nowHTML(s.now.track, s.active ? "Not playing" : "The host is not playing here") +
        `<div class="qcard__label">Up Next</div>` +
        (rows ? `<ol class="qcard__list">${rows}</ol>` : `<p class="qcard__empty">Nothing is up next. Add a song from Search.</p>`);
    }

    const undo = [
      store.watch(paint),
      rowMenu(body, ".qrow", (row, x, y) => {
        const index = Number(row.dataset.idx);
        const off = store.can("queueMove") ? undefined : HOST_ONLY;
        openMenu(x, y, [
          { label: "Play Next", disabled: off ?? (index === 0 ? "This song is next already" : undefined), run: () => void store.act({ kind: "queueMove", index, to: 0 }) },
          { label: "Remove from Queue", disabled: off, run: () => void store.act({ kind: "queueRemove", index }) },
        ]);
      }),
    ];
    return () => {
      undo.forEach((u) => u());
      body.remove();
    };
  },
};
