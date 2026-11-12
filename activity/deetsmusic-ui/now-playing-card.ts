// The Now Playing card. The markup is DeetsMusic's (src/now-playing-card.ts `TEMPLATE`), so
// the copied look draws it as it draws the app's. Left out, as DeetsMusic's own web port
// leaves out what a browser cannot do: Shuffle and Repeat (the bridge has no route), Favorite,
// Add to Library, AirPlay, the Last.fm counts and the volume row (the bot's volume is the
// host's; a person sets their own level for Lil Deets in Discord).

import { artworkURL, positionNow, type HostState } from "../deetsmusic-protocol";
import { watchAlbumColor } from "./album-color";
import type { CardDef } from "./cards";
import { clock, explicitBadge, on, type Undo } from "./dom";
import { makeSlider } from "./slider";
import { setMiniView, surfaceState } from "./surface";
import { artPxNow, onArtPx } from "./art-size";
import { closeFullPlayer, isFullPlayer, openFullPlayer } from "./full-player";
import { mountVinyl } from "./vinyl";

const ICON_PLAY = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>';
const ICON_PAUSE = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5h4v14H7zM13 5h4v14h-4z" /></svg>';
const ICON_SHRINK = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 10h-6V4M14 10l7-7M4 14h6v6M10 14l-7 7"/></svg>';
const GLYPH_NOTE = '<span class="np__art-glyph" aria-hidden="true">♪</span>';

const TEMPLATE = `
  <div class="np np--idle">
    <div class="np__aurora" aria-hidden="true"></div>
    <div class="np__art" id="np-art">${GLYPH_NOTE}</div>
    <button class="panel__action np__close" type="button" aria-label="Back to the cards" title="Back to the cards">${ICON_SHRINK}</button>
    <div class="np__center">
      <div class="np__meta">
        <div class="np__line">
          <span class="np__title" id="np-title"><span class="np__title-text">Not playing</span></span>
          <span class="np__explicit" id="np-explicit"></span>
        </div>
        <div class="np__line">
          <span class="np__artist" id="np-artist"></span>
        </div>
        <span class="np__album" id="np-album"></span>
      </div>
      <div class="np__scrub scrub">
        <div class="scrub__track"><div class="scrub__fill"></div></div>
        <span class="scrub__handle" aria-hidden="true"></span>
      </div>
      <div class="np__times" aria-hidden="true"><span id="np-elapsed">0:00</span><span id="np-remaining">0:00</span></div>
      <div class="np__bottom">
        <div class="np__left">
          <button class="panel__action np__summon" id="np-summon" type="button" aria-label="Show queue" title="Shows the Queue card: what plays next">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <line x1="3" y1="6" x2="13" y2="6"></line>
              <line x1="3" y1="12" x2="13" y2="12"></line>
              <line x1="3" y1="18" x2="13" y2="18"></line>
              <path d="M17 8.5 22 12 17 15.5z" fill="none"></path>
            </svg>
          </button>
          <button class="panel__action np__search" id="np-search" type="button" aria-label="Show search" title="Shows the Search card: the host's Apple Music search">
            <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"></circle><path d="M15.5 15.5 21 21"></path></svg>
          </button>
        </div>
        <div class="np__controls">
          <button class="np__btn" id="np-prev" type="button" aria-label="Previous" title="Plays the song before this one">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6v12h2V6zM20 6 10 12 20 18z" /></svg>
          </button>
          <button class="np__btn np__btn--play" id="np-playpause" type="button" aria-label="Play" title="Play">${ICON_PLAY}</button>
          <button class="np__btn" id="np-next" type="button" aria-label="Next" title="Plays the next song">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6 14 12 4 18zM16 6v12h2V6z" /></svg>
          </button>
        </div>
        <div class="np__right"></div>
      </div>
    </div>
  </div>`;

/** Why a control is off, in the words of the `/dm` refusals. */
const HOST_ONLY = "The host has set this to host only in DeetsMusic";

export const nowPlayingCard: CardDef = {
  id: "now-playing",
  title: "Now Playing",
  mount(panel, ctx) {
    const { store } = ctx;
    panel.innerHTML = TEMPLATE;
    const q = <T extends HTMLElement>(sel: string): T => panel.querySelector<T>(sel)!;
    const np = q(".np");
    const art = q("#np-art");
    const title = q(".np__title-text");
    const explicit = q("#np-explicit");
    const artist = q("#np-artist");
    const album = q("#np-album");
    const elapsed = q("#np-elapsed");
    const remaining = q("#np-remaining");
    const playBtn = q<HTMLButtonElement>("#np-playpause");
    const prevBtn = q<HTMLButtonElement>("#np-prev");
    const nextBtn = q<HTMLButtonElement>("#np-next");
    const scrubEl = q(".np__scrub");
    const undo: Undo[] = [];

    let scrubbingTo: number | null = null;
    const vinyl = mountVinyl(art, GLYPH_NOTE);
    undo.push(() => vinyl.destroy());

    const duration = (): number => store.state()?.now.track?.durationMs ?? 0;

    const slider = makeSlider(scrubEl, {
      enabled: () => store.can("seek") && duration() > 0,
      onDrag: (f) => {
        scrubbingTo = f * duration();
        vinyl.scrub(scrubbingTo / 1000);
        paintClock();
      },
      onCommit: (f) => {
        const positionMs = Math.round(f * duration());
        scrubbingTo = null;
        vinyl.scrub(positionMs / 1000, true);
        void store.act({ kind: "seek", positionMs });
      },
    });

    function paintClock(): void {
      const s = store.state();
      const d = duration();
      const pos = scrubbingTo ?? (s ? positionNow(s.now) : 0);
      elapsed.textContent = clock(pos);
      remaining.textContent = d ? `-${clock(d - pos)}` : "0:00";
      slider.setValue(d ? pos / d : 0);
    }

    /** A control the rules keep from this person stays on screen, greyed, with the reason. */
    function gate(btn: HTMLButtonElement, ok: boolean, hint: string): void {
      btn.disabled = !ok;
      btn.title = ok ? hint : HOST_ONLY;
    }

    function paint(s: HostState): void {
      const t = s.now.track;
      np.classList.toggle("np--idle", !t);
      title.textContent = t ? t.title : s.active ? "Not playing" : "The host is not playing here";
      explicit.innerHTML = explicitBadge(t?.contentRating);
      artist.textContent = t?.artistName ?? "";
      album.textContent = t?.albumName ?? "";

      // The cover: DeetsMusic's vinyl markup, so Press turns it and the other skins show it flat.
      // The cover: DeetsMusic's record slot (vinyl.ts), so Press turns it, Metal's drum turns
      // between songs, Happy pushes it, and the other skins show it flat. The same song at a
      // new size (the full player) swaps its file once decoded.
      vinyl.show(t?.catalogId ?? "", artworkURL(t, artPxNow()) ?? undefined);
      vinyl.playing(!!t && s.now.playing);
      if (t) vinyl.position(positionNow(s.now) / 1000, (t.durationMs ?? 0) / 1000);

      const playing = s.now.playing;
      playBtn.innerHTML = playing ? ICON_PAUSE : ICON_PLAY;
      playBtn.setAttribute("aria-label", playing ? "Pause" : "Play");
      gate(playBtn, store.can(playing ? "pause" : "play") && !!t, playing ? "Pause" : "Play");
      gate(prevBtn, store.can("previous"), "Plays the song before this one");
      gate(nextBtn, store.can("next"), "Plays the next song");
      scrubEl.toggleAttribute("data-locked", !store.can("seek"));
      scrubEl.title = store.can("seek") ? "" : HOST_ONLY;
      paintClock();
      tick();
    }

    // The clock runs in the page between state messages. One frame loop while a song plays
    // and the page is on screen; nothing while paused or hidden.
    let raf = 0;
    function tick(): void {
      cancelAnimationFrame(raf);
      const s = store.state();
      if (!s?.now.playing || document.hidden) return;
      raf = requestAnimationFrame(() => {
        paintClock();
        tick();
      });
    }
    undo.push(on(document, "visibilitychange" as keyof HTMLElementEventMap, tick));
    undo.push(() => cancelAnimationFrame(raf));

    undo.push(
      on(playBtn, "click", () => void store.act({ kind: store.state()?.now.playing ? "pause" : "play" })),
      on(prevBtn, "click", () => void store.act({ kind: "previous" })),
      on(nextBtn, "click", () => void store.act({ kind: "next" })),
      on(q("#np-summon"), "click", () => ctx.requestCard("queue")),
      on(q("#np-search"), "click", () => ctx.requestCard("search")),
      // A tap on the cover: Mini opens its player view; Midi and Max open the full player (his
      // call 2A, 2026-11-11). The corner square goes back: the cards, or the player closes.
      on(art, "click", () => {
        const sf = surfaceState();
        if (sf.tile || sf.wide) return;
        if (sf.surface === "mini") {
          if (sf.mini === "cards") setMiniView("player");
        } else if (!isFullPlayer()) {
          void openFullPlayer("cover");
        }
      }),
      on(q(".np__close"), "click", () => {
        if (isFullPlayer()) void closeFullPlayer("close");
        else setMiniView("cards");
      }),
      // The full player asks for a larger cover file while it is open.
      onArtPx(() => {
        const s = store.state();
        if (s) paint(s);
      }),
    );

    undo.push(store.watch(paint));
    // After the first paint, so the card has its song: the aurora and the text take the cover's colours.
    undo.push(watchAlbumColor(np, store));
    return () => undo.forEach((u) => u());
  },
};
