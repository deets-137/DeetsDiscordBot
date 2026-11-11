// The Queue card: what plays now, then Up Next. Markup as DeetsMusic's src/qcard.ts.
// Under the host's "Guests may change Up Next" rule a person may drag a row to a new place
// (row-drag.ts; a finger holds the row, then moves), and a row's menu (right-click, or a long
// press let go with no move) moves it to the top or removes it.

import "./look/src/styles/qcard.css";
import type { CardDef } from "./cards";
import type { HostState } from "../deetsmusic-protocol";
import { rowMenu, openMenu } from "./menu";
import { rowDrag } from "./row-drag";
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
    let held: HostState | null = null; // a state that arrived during a drag
    function paint(s: HostState): void {
      // A drag holds the list: a rewrite would take the pressed row and the line away.
      if (drag.active()) {
        held = s;
        return;
      }
      // Rewrite only when the list itself changed: a play or a pause must not reset the scroll.
      const key = `${s.active}|${s.now.track?.catalogId ?? ""}|${s.upNext.map((t) => t.catalogId).join(",")}`;
      if (key === shown) return;
      shown = key;
      const scroll = body.querySelector(".qcard__list")?.scrollTop ?? 0;
      const rows = s.upNext.map((t, i) => rowHTML(i, t, { key: `q${i}:${t.catalogId}` })).join("");
      body.innerHTML =
        nowHTML(s.now.track, s.active ? "Not playing" : "The host is not playing here") +
        `<div class="qcard__label">Up Next</div>` +
        (rows ? `<ol class="qcard__list">${rows}</ol>` : `<p class="qcard__empty">Nothing is up next. Add a song from Search.</p>`);
      const list = body.querySelector(".qcard__list");
      if (list) list.scrollTop = scroll;
    }

    /** Draw the list from the host's state again, whatever was shown. */
    const repaint = (): void => {
      shown = "";
      const s = held ?? store.state();
      held = null;
      if (s) paint(s);
    };

    function menuFor(row: HTMLElement, x: number, y: number): void {
      const index = Number(row.dataset.idx);
      const catalogId = row.dataset.cid; // the song this person saw in the row
      const off = store.can("queueMove") ? undefined : HOST_ONLY;
      openMenu(x, y, [
        { label: "Play Next", disabled: off ?? (index === 0 ? "This song is next already" : undefined), run: () => void store.act({ kind: "queueMove", index, to: 0, catalogId }) },
        { label: "Remove from Queue", disabled: off, run: () => void store.act({ kind: "queueRemove", index, catalogId }) },
      ]);
    }

    const drag = rowDrag({
      root: body,
      // A finger held still on a row and let go: the long press opens the menu (row-drag.ts).
      onHoldRelease: menuFor,
      rowAt(target) {
        if (!store.can("queueMove")) return null;
        const row = target.closest<HTMLElement>(".qrow");
        const list = row?.closest<HTMLElement>(".qcard__list");
        if (!row || !list) return null;
        return { row, index: Number(row.dataset.idx), list, count: list.children.length };
      },
      onEnd(from, to) {
        if (to === null) return repaint(); // a cancel or a drop in place: only what was held
        // Show the row in its new place at once; the host's state confirms it, or puts it back.
        const list = body.querySelector<HTMLElement>(".qcard__list");
        const row = list?.querySelector<HTMLElement>(`.qrow[data-idx="${from}"]`);
        const catalogId = row?.dataset.cid; // the song this person dragged
        if (list && row) {
          row.remove();
          list.insertBefore(row, list.children[to] ?? null);
          [...list.children].forEach((el, i) => ((el as HTMLElement).dataset.idx = String(i)));
          shown = "";
        }
        held = null;
        void store.act({ kind: "queueMove", index: from, to, catalogId }).then((ok) => {
          if (!ok) repaint();
        });
      },
    });

    const undo = [
      store.watch(paint),
      () => drag.destroy(),
      // While a person may move rows, a finger's long press is the drag's hold (it opens the
      // menu itself). A person who may not still gets the menu, with the reason on each line.
      rowMenu(body, ".qrow", menuFor, () => store.can("queueMove")),
    ];
    return () => {
      undo.forEach((u) => u());
      body.remove();
    };
  },
};
