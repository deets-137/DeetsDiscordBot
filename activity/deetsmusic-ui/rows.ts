// The row and the "now" hero that Queue and History share: DeetsMusic's markup
// (src/queue-rows.ts `rowHTML`, src/qcard.ts), so the two cards cannot drift apart.

import { artworkURL, type Track } from "../deetsmusic-protocol";
import { esc } from "./dom";

const ART_PX = 96;

/** One `.qrow`. `meta` is the right-hand column (History: the time of day). */
export function rowHTML(idx: number, t: Track, opts: { interactive?: boolean; meta?: string; key?: string } = {}): string {
  const cover = artworkURL(t, ART_PX);
  const art = cover
    ? `<img class="qrow__art" src="${esc(cover)}" alt="" loading="lazy" data-art />`
    : `<div class="qrow__art qrow__art--empty" aria-hidden="true">♪</div>`;
  const role = opts.interactive ? ` role="button" tabindex="0"` : "";
  return `<li class="qrow" data-k="${esc(opts.key ?? t.catalogId)}" data-idx="${idx}"${role} data-cid="${esc(t.catalogId)}">${art}<div class="qrow__text"><span class="qrow__title">${esc(
    t.title,
  )}</span><span class="qrow__artist">${esc(t.artistName)}</span></div>${opts.meta ?? ""}</li>`;
}

/** The hero at the top of both cards: what plays now. */
export function nowHTML(t: Track | null, idleTitle: string): string {
  const cover = artworkURL(t, ART_PX);
  const art = cover
    ? `<img class="qnow__art" src="${esc(cover)}" alt="" data-art />`
    : `<div class="qnow__art qnow__art--empty" aria-hidden="true">♪</div>`;
  return `<div class="qnow${t ? "" : " qnow--idle"}">${art}<div class="qnow__text"><span class="qnow__title">${esc(
    t ? t.title : idleTitle,
  )}</span><span class="qnow__artist">${esc(t?.artistName ?? "")}</span></div></div>`;
}
