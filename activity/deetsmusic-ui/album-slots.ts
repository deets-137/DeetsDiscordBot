// The album palette's pure colour math: a port of DeetsMusic's src/album-slots.ts (the parts
// the Now Playing card needs). No DOM, no store. Keep it in step with the app's file: the
// same cover must give the same aurora in both.

export interface AlbumPalette {
  bg?: string; // "#rrggbb"
  c1?: string;
  c2?: string;
}

// ── colour math (sRGB ⇄ OKLCH) ─────────────────────────────────────────────────
export type RGB = [number, number, number]; // 0..1

export function parseColor(s: string): RGB | null {
  const m = /^#([0-9a-f]{6})$/i.exec(s.trim());
  if (m) {
    const n = parseInt(m[1], 16);
    return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
  }
  const r = /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/i.exec(s.trim());
  if (r) return [Number(r[1]) / 255, Number(r[2]) / 255, Number(r[3]) / 255];
  return null;
}
export const lin = (c: number): number => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gam = (c: number): number => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const clamp01 = (x: number): number => Math.min(1, Math.max(0, x));

export type LCH = [number, number, number];
export function toOKLCH([r, g, b]: RGB): LCH {
  const [lr, lg, lb] = [lin(r), lin(g), lin(b)];
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(a, bb), Math.atan2(bb, a)];
}
export function fromOKLCH([L, C, h]: LCH): RGB {
  const a = C * Math.cos(h);
  const bb = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * bb) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * bb) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * bb) ** 3;
  return [
    gam(clamp01(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s)),
    gam(clamp01(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s)),
    gam(clamp01(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s)),
  ];
}
export const toHex = ([r, g, b]: RGB): string =>
  "#" + [r, g, b].map((c) => Math.round(clamp01(c) * 255).toString(16).padStart(2, "0")).join("");

// ── the aurora's stops ─────────────────────────────────────────────────────────

/** Below this OKLCH chroma a colour reads as grey on the aurora. */
const GREY_CHROMA = 0.04;

/** The palette's colours, most colourful first (OKLCH chroma). */
export function rankByColor(p: AlbumPalette): { s: string; chroma: number }[] {
  return [p.bg, p.c1, p.c2]
    .flatMap((s) => {
      const rgb = s ? parseColor(s) : null;
      return rgb ? [{ s: s!, chroma: toOKLCH(rgb)[1] }] : [];
    })
    .sort((a, b) => b.chroma - a.chroma);
}

/** The aurora's three stops, by what shows: `--album-bg` sits under the cover, `--album-c1` is
 *  the rim, `--album-c2` the halo. The most colourful goes on the rim, the next on the halo (the
 *  rim's colour again when the next is grey), the least under the cover. */
export function auroraSlots(p: AlbumPalette): [string | undefined, string | undefined, string | undefined] {
  const [first, second, third] = rankByColor(p);
  if (!first) return [undefined, undefined, undefined];
  const halo = second && (second.chroma >= GREY_CHROMA || first.chroma < GREY_CHROMA) ? second.s : first.s;
  return [third?.s ?? second?.s, first.s, halo];
}

/** The album's one colour: the most colourful of the three. */
export function albumColor(p: AlbumPalette | null | undefined): string | undefined {
  return p ? rankByColor(p)[0]?.s : undefined;
}

// ── the album as neon (DeetsMusic's OCEAN.md §7, ALBUM-COLOR.md § The album light) ──

/** Is this ground dark enough for a lit line? */
export const isDark = (c: RGB): boolean => toOKLCH(c)[0] < 0.55;

/** Album colour → a neon line: the bloom more vivid, the core lifted toward white on a dark
 *  ground; both deeper on a light ground. A grey cover stays grey. */
export function asNeon(c: RGB, darkGround: boolean): { core: RGB; bloom: RGB } {
  const [, C, h] = toOKLCH(c);
  const vivid = Math.min(C * 1.5, 0.2);
  return darkGround
    ? { bloom: fromOKLCH([0.72, vivid, h]), core: fromOKLCH([0.92, vivid * 0.45, h]) }
    : { bloom: fromOKLCH([0.62, vivid, h]), core: fromOKLCH([0.45, vivid, h]) };
}

/** The album light's three roles, as hex. */
export interface Neon {
  core: string;
  bloom: string;
  /** The secondary colour's bloom (Cyber's bolts). */
  second: string;
}

/** A palette as the card edges' neon (an all-grey cover on a dark ground lights white). */
export function albumNeon(p: AlbumPalette, darkGround: boolean): Neon | null {
  const [, rim, halo] = auroraSlots(p);
  const main = rim ? parseColor(rim) : null;
  if (!main) return null;
  const lit = asNeon(main, darkGround);
  const grey = darkGround && toOKLCH(main)[1] < GREY_CHROMA;
  const next = asNeon((halo ? parseColor(halo) : null) ?? main, darkGround);
  return { core: toHex(lit.core), bloom: toHex(grey ? lit.core : lit.bloom), second: toHex(grey ? next.core : next.bloom) };
}

// ── a palette from a picture's own pixels ──────────────────────────────────────

const HUE_BINS = 12;

/** Three colours from RGBA pixels of a small copy of the cover (about 48 × 48). Each pixel
 *  falls in one of twelve hue bins, weighted by its chroma; the three heaviest bins give the
 *  colours. Too dark and too light pixels carry no hue and are skipped. A grey picture gives
 *  its mean colour three times. */
export function paletteFromPixels(px: Uint8ClampedArray): AlbumPalette {
  const bins = Array.from({ length: HUE_BINS }, () => ({ w: 0, r: 0, g: 0, b: 0 }));
  const all = { n: 0, r: 0, g: 0, b: 0 };
  for (let i = 0; i + 3 < px.length; i += 4) {
    if (px[i + 3] < 128) continue;
    const rgb: RGB = [px[i] / 255, px[i + 1] / 255, px[i + 2] / 255];
    all.n++;
    all.r += rgb[0];
    all.g += rgb[1];
    all.b += rgb[2];
    const [L, C, h] = toOKLCH(rgb);
    if (L < 0.2 || L > 0.95 || C < GREY_CHROMA) continue;
    const bin = bins[Math.floor(((h + Math.PI) / (2 * Math.PI)) * HUE_BINS) % HUE_BINS];
    bin.w += C;
    bin.r += rgb[0] * C;
    bin.g += rgb[1] * C;
    bin.b += rgb[2] * C;
  }
  const mean = all.n ? toHex([all.r / all.n, all.g / all.n, all.b / all.n]) : undefined;
  const top = bins
    .filter((b) => b.w > 0)
    .sort((a, b) => b.w - a.w)
    .slice(0, 3)
    .map((b) => toHex([b.r / b.w, b.g / b.w, b.b / b.w]));
  return { bg: top[2] ?? mean, c1: top[0] ?? mean, c2: top[1] ?? top[0] ?? mean };
}
