// The Search card: songs from the host's DeetsMusic search. Markup as DeetsMusic's
// src/search-card.ts (the field, then one "Songs" section). Left out: the category filter,
// the recents, and artists, albums and playlists (the bot's bridge call asks for songs only).
//
// A tap on a song adds it to the end of the queue. Its menu (right-click, or a long press)
// has Play Next, Add to Queue and, for the host, Play Now.

import type { Track } from "../deetsmusic-protocol";
import { artworkURL } from "../deetsmusic-protocol";
import type { CardDef } from "./cards";
import { esc, explicitBadge, on } from "./dom";
import { rowMenu, openMenu } from "./menu";
import { toast } from "./toast";

const DEBOUNCE_MS = 350;
const HOST_ONLY_ADD = "The host has set “Guests may add songs” to host only in DeetsMusic";

const TEMPLATE = `
  <div class="search__bar">
    <div class="search__field">
      <svg class="search__icon" viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.5"/><path d="M11 11l3 3"/></svg>
      <input class="search__input" type="search" placeholder="Search Apple Music" spellcheck="false" enterkeyhint="search" />
      <span class="search__busy" aria-hidden="true"></span>
      <button class="search__clear" type="button" aria-label="Clear search" title="Clears the search" hidden>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
      </button>
    </div>
  </div>
  <div class="search__panes">
    <div class="spane" data-pos="center">
      <div class="spane__scroll search__root" tabindex="0"></div>
    </div>
  </div>`;

const songHTML = (t: Track): string => {
  const cover = artworkURL(t, 96);
  const art = cover
    ? `<img class="search__song-art" src="${esc(cover)}" alt="" loading="lazy" decoding="async" data-art />`
    : `<div class="search__song-art search__song-art--empty" aria-hidden="true">♪</div>`;
  return `<div class="search__song" data-k="song:${esc(t.catalogId)}" data-song="${esc(t.catalogId)}" data-cid="${esc(t.catalogId)}" role="button" tabindex="0">
      ${art}
      <div class="search__song-text"><span class="search__song-title">${esc(t.title)}${explicitBadge(t.contentRating)}</span><span class="search__song-artist">${esc(t.artistName)}</span></div>
    </div>`;
};

export const searchCard: CardDef = {
  id: "search",
  title: "Search",
  mount(panel, ctx) {
    const { store } = ctx;
    const body = document.createElement("div");
    body.className = "panel__body search";
    body.innerHTML = TEMPLATE;
    panel.appendChild(body);

    const input = body.querySelector<HTMLInputElement>(".search__input")!;
    const clear = body.querySelector<HTMLButtonElement>(".search__clear")!;
    const bar = body.querySelector<HTMLElement>(".search__bar")!;
    const busy = (yes: boolean): void => void bar.classList.toggle("search__bar--busy", yes);
    const rootEl = body.querySelector<HTMLElement>(".search__root")!;

    let timer = 0;
    let serial = 0;
    let results: Track[] = [];

    const prompt = (text: string): void => {
      rootEl.innerHTML = `<p class="search__prompt">${esc(text)}</p>`;
    };
    prompt("Search the host's Apple Music. A tap on a song adds it to the queue.");

    async function run(term: string): Promise<void> {
      const mine = ++serial;
      if (!term) {
        results = [];
        busy(false);
        prompt("Search the host's Apple Music. A tap on a song adds it to the queue.");
        return;
      }
      busy(true);
      try {
        const found = await store.search(term);
        if (mine !== serial) return; // a newer search took over
        results = found;
        rootEl.innerHTML = found.length
          ? `<section class="search__sec" data-key="sec:songs"><h3 class="search__sec-title">Songs</h3><div class="search__scroller search__scroller--songs">${found.map(songHTML).join("")}</div></section>`
          : `<p class="search__prompt">No results for “${esc(term)}”.</p>`;
      } catch {
        if (mine === serial) prompt("The host's search did not answer. Try again.");
      } finally {
        if (mine === serial) busy(false);
      }
    }

    const titleOf = (id: string): string => results.find((t) => t.catalogId === id)?.title ?? "The song";

    async function add(catalogId: string, mode: "next" | "later"): Promise<void> {
      if (await store.act({ kind: "enqueue", catalogId, mode })) {
        toast(mode === "next" ? `${titleOf(catalogId)} plays next` : `${titleOf(catalogId)} is in the queue`, "info");
      }
    }

    const undo = [
      on(input, "input", () => {
        clear.hidden = !input.value;
        window.clearTimeout(timer);
        timer = window.setTimeout(() => void run(input.value.trim()), DEBOUNCE_MS);
      }),
      on(input, "keydown", (e) => {
        if (e.key !== "Enter") return;
        window.clearTimeout(timer);
        void run(input.value.trim());
        input.blur(); // a phone's keyboard goes away, so the results show
      }),
      on(clear, "click", () => {
        input.value = "";
        clear.hidden = true;
        window.clearTimeout(timer);
        void run("");
        input.focus();
      }),
      on(rootEl, "click", (e) => {
        const song = (e.target as HTMLElement).closest<HTMLElement>(".search__song");
        if (song?.dataset.song) void add(song.dataset.song, "later");
      }),
      on(rootEl, "keydown", (e) => {
        if (e.key !== "Enter") return;
        const song = (e.target as HTMLElement).closest<HTMLElement>(".search__song");
        if (song?.dataset.song) void add(song.dataset.song, "later");
      }),
      rowMenu(rootEl, ".search__song", (row, x, y) => {
        const catalogId = row.dataset.song ?? "";
        const off = store.can("enqueue") ? undefined : HOST_ONLY_ADD;
        openMenu(x, y, [
          { label: "Play Next", disabled: off, run: () => void add(catalogId, "next") },
          { label: "Add to Queue", disabled: off, run: () => void add(catalogId, "later") },
          ...(store.state()?.you.host ? [{ label: "Play Now", run: () => void store.act({ kind: "playNow", catalogId }) }] : []),
        ]);
      }),
      () => window.clearTimeout(timer),
    ];
    return () => {
      undo.forEach((u) => u());
      body.remove();
    };
  },
};
