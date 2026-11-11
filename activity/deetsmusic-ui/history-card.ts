// The History card: what plays now, then what played, newest first. Markup as DeetsMusic's
// src/history-card.ts. A row's menu adds the song to the queue again.

import "./look/src/styles/qcard.css";
import type { CardDef } from "./cards";
import type { HostState } from "../deetsmusic-protocol";
import { timeOfDay } from "./dom";
import { rowMenu, openMenu } from "./menu";
import { nowHTML, rowHTML } from "./rows";

const HOST_ONLY = "The host has set “Guests may add songs” to host only in DeetsMusic";

export const historyCard: CardDef = {
  id: "history",
  title: "History",
  mount(panel, ctx) {
    const { store } = ctx;
    const body = document.createElement("div");
    body.className = "panel__body qcard";
    body.tabIndex = 0;
    panel.appendChild(body);

    let shown = "";
    function paint(s: HostState): void {
      const key = `${s.active}|${s.now.track?.catalogId ?? ""}|${s.history.map((h) => `${h.track.catalogId}@${h.playedAt ?? ""}`).join(",")}`;
      if (key === shown) return;
      shown = key;
      const rows = s.history
        .map((h, i) =>
          rowHTML(i, h.track, {
            key: `${h.playedAt ?? i}:${h.track.catalogId}`,
            meta: h.playedAt ? `<div class="qrow__meta"><span class="qrow__time">${timeOfDay(h.playedAt)}</span></div>` : "",
          }),
        )
        .join("");
      body.innerHTML =
        nowHTML(s.now.track, s.active ? "Not playing" : "The host is not playing here") +
        `<div class="qcard__label">Previously</div>` +
        (rows ? `<ol class="qcard__list">${rows}</ol>` : `<p class="qcard__empty">No songs have played yet.</p>`);
    }

    const undo = [
      store.watch(paint),
      rowMenu(body, ".qrow", (row, x, y) => {
        const catalogId = row.dataset.cid ?? "";
        const off = store.can("enqueue") ? undefined : HOST_ONLY;
        openMenu(x, y, [
          { label: "Play Next", disabled: off, run: () => void store.act({ kind: "enqueue", catalogId, mode: "next" }) },
          { label: "Add to Queue", disabled: off, run: () => void store.act({ kind: "enqueue", catalogId, mode: "later" }) },
        ]);
      }),
    ];
    return () => {
      undo.forEach((u) => u());
      body.remove();
    };
  },
};
