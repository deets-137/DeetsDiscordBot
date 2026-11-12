// Album colour: the Now Playing card's aurora and text take the cover's colours. A port of
// DeetsMusic's src/album-color.ts, with one difference in where the palette comes from.
//
// The app asks its Rust side for Apple's palette of the album. A page in a call has no such
// side, and the bridge sends no palette (BRIDGE.md §3). So the page reads the cover's own
// pixels, with the app's rule for a picture that has no Apple palette (`paletteFromPixels`,
// album-slots.ts): 0 API calls, one 48 px image per album from Apple's image server. The
// colours can differ a little from the app's for the same album.
//
// The palette lands as inline roles on the card (`--album-bg/-c1/-c2`, the `np--album` class),
// and the look's registered transitions crossfade them. A skin that opts in with
// `--np-album-text: 1` (Glass) also gets the title, the subtext and the accent in the album's
// colours, each pushed through the app's WCAG guard against the worst backdrop the aurora can
// put under it: 4.5:1 for text, 3:1 for accents.

import { artworkURL, type Track } from "../deetsmusic-protocol";
import { auroraSlots, fromOKLCH, lin, paletteFromPixels, parseColor, toHex, toOKLCH, type AlbumPalette, type RGB } from "./album-slots";
import type { Undo } from "./dom";
import type { Store } from "./store";

const PROPS = ["--album-bg", "--album-c1", "--album-c2"] as const;
const TEXT_PROPS = ["--np-title", "--np-subtext", "--np-accent"] as const;
const SAMPLE_PX = 48;
const CACHE_MAX = 64;

// ── WCAG 2 contrast ────────────────────────────────────────────────────────────
const alphaOf = (s: string): number => {
  const m = /rgba?\([^)]*[,/]\s*([\d.]+)\s*\)$/i.exec(s.trim());
  return m ? Number(m[1]) : 1;
};
const luminance = ([r, g, b]: RGB): number => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
const contrast = (a: RGB, b: RGB): number => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const mix = (a: RGB, b: RGB, t: number): RGB => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/** Keep hue and chroma, move lightness away from the backdrop until every backdrop clears
 *  `target`. If neither direction can reach it, return whichever extreme contrasts best. */
function guard(color: RGB, backdrops: RGB[], target: number): RGB {
  const worst = (c: RGB): number => Math.min(...backdrops.map((b) => contrast(c, b)));
  if (worst(color) >= target) return color;
  const [, C, h] = toOKLCH(color);
  const darkBackdrop = backdrops.reduce((s, b) => s + luminance(b), 0) / backdrops.length < 0.18;
  const tryDir = (toWhite: boolean): { c: RGB; ratio: number } => {
    let lo = toOKLCH(color)[0];
    let hi = toWhite ? 1 : 0;
    let best: RGB = fromOKLCH([hi, C, h]);
    if (worst(best) < target) return { c: best, ratio: worst(best) };
    for (let i = 0; i < 18; i++) {
      const mid = (lo + hi) / 2;
      const c = fromOKLCH([mid, C, h]);
      if (worst(c) >= target) {
        hi = mid;
        best = c;
      } else lo = mid;
    }
    return { c: best, ratio: worst(best) };
  };
  const first = tryDir(darkBackdrop);
  if (first.ratio >= target) return first.c;
  const second = tryDir(!darkBackdrop);
  return second.ratio > first.ratio ? second.c : first.c;
}

// ── the cover's palette ────────────────────────────────────────────────────────
// One read per cover, however many cards ask; the answers are kept for the visit.
const known = new Map<string, AlbumPalette | null>();
const inFlight = new Map<string, Promise<AlbumPalette | null>>();

/** The cover's palette (one read per album, kept for the visit). Ocean's glow and neon read it too. */
export function readPalette(track: Track): Promise<AlbumPalette | null> {
  const key = track.artworkTemplate ?? "";
  const url = artworkURL(track, SAMPLE_PX);
  if (!key || !url) return Promise.resolve(null);
  if (known.has(key)) return Promise.resolve(known.get(key) ?? null);
  let p = inFlight.get(key);
  if (!p) {
    p = new Promise<AlbumPalette | null>((done) => {
      const img = new Image();
      img.crossOrigin = "anonymous"; // the canvas may be read only when the server allows it
      img.decoding = "async";
      img.onload = () => {
        try {
          const canvas = document.createElement("canvas");
          canvas.width = canvas.height = SAMPLE_PX;
          const g = canvas.getContext("2d", { willReadFrequently: true });
          if (!g) return done(null);
          g.drawImage(img, 0, 0, SAMPLE_PX, SAMPLE_PX);
          done(paletteFromPixels(g.getImageData(0, 0, SAMPLE_PX, SAMPLE_PX).data));
        } catch {
          done(null); // a cover the canvas may not read: the theme's colours stay
        }
      };
      img.onerror = () => done(null);
      img.src = url;
    }).then((palette) => {
      inFlight.delete(key);
      if (known.size >= CACHE_MAX) known.delete(known.keys().next().value as string);
      known.set(key, palette);
      return palette;
    });
    inFlight.set(key, p);
  }
  return p;
}

/** Watch the host's song and tint `card` (the `.np` element) with its cover's palette. */
export function watchAlbumColor(card: HTMLElement, store: Store): Undo {
  let liveKey: string | null = null;
  let livePalette: AlbumPalette | null = null;

  // A probe inside the card resolves any role, the inline album roles and color-mix() tokens
  // too, to a computed rgb(): the only way a script can read them.
  const probe = document.createElement("span");
  probe.setAttribute("aria-hidden", "true");
  probe.style.cssText = "position:absolute;width:0;height:0;overflow:hidden;pointer-events:none";
  card.appendChild(probe);
  const resolve = (expr: string): string => {
    probe.style.color = expr;
    return getComputedStyle(probe).color;
  };
  const token = (name: string): string => getComputedStyle(card).getPropertyValue(name).trim();
  const clearText = (): void => TEXT_PROPS.forEach((p) => card.style.removeProperty(p));

  /** Work out the guarded text roles for the live palette again (or clear them). */
  const applyText = (): void => {
    if (token("--np-album-text") !== "1") return clearText();
    const canvas = parseColor(resolve("var(--canvas)")) ?? [0, 0, 0];
    const panelStr = resolve("var(--panel)");
    const base = mix(canvas, parseColor(panelStr) ?? canvas, alphaOf(panelStr));
    const strength = parseFloat(token("--album-aurora-strength")) / 100 || 0.36;
    const stops = ["var(--album-bg)", "var(--album-c1)", "var(--album-c2)"].map((v) => parseColor(resolve(v))).filter((c): c is RGB => !!c);
    const backdrops = [base, ...stops.map((s) => mix(base, s, strength))];

    const title = parseColor(livePalette?.c1 ?? "") ?? parseColor(resolve("var(--title)"));
    const sub = parseColor(livePalette?.c2 ?? "") ?? parseColor(resolve("var(--subtext)"));
    if (!title || !sub) return clearText();
    card.style.setProperty("--np-title", toHex(guard(title, backdrops, 4.5)));
    card.style.setProperty("--np-subtext", toHex(guard(sub, backdrops, 4.5)));
    card.style.setProperty("--np-accent", toHex(guard(title, backdrops, 3)));
  };

  const clear = (): void => {
    PROPS.forEach((p) => card.style.removeProperty(p));
    card.classList.remove("np--album");
    livePalette = null;
    applyText();
  };

  const apply = (p: AlbumPalette): void => {
    const slots = auroraSlots(p);
    PROPS.forEach((prop, i) => {
      const v = slots[i];
      if (v) card.style.setProperty(prop, v);
      else card.style.removeProperty(prop);
    });
    card.classList.add("np--album");
    livePalette = p;
    applyText();
  };

  const unwatch = store.watch((s) => {
    const track = s.now.track;
    const key = track?.artworkTemplate ?? null;
    if (key === liveKey) return; // the same album, or still nothing: no work
    liveKey = key;
    clear(); // the theme's colours show until a palette arrives
    if (!track || !key) return;
    void readPalette(track).then((p) => {
      if (liveKey === key && p) apply(p); // not when the song changed during the read
    });
  });

  // A theme or skin change moves the backdrop, and whether the skin opts in at all: guard the
  // same palette again, one frame later, when the new tokens are computed.
  const mo = new MutationObserver(() => requestAnimationFrame(applyText));
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-skin"] });

  return () => {
    unwatch();
    mo.disconnect();
    clear();
    probe.remove();
  };
}
