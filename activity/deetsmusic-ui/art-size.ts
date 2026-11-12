// The cover file's size. At rest the Now Playing card asks Apple's image server for 480 px
// (640 on a sharp screen). The full player asks for a larger file while it is open
// (fullscreen-player.md §12.5: the box in device pixels, in 200 px steps, at most 2000 px).

let fullPx = 0;
const subs = new Set<() => void>();

/** The rest size (DeetsMusic's stage size for this screen). */
export const restArtPx = (): number => (window.devicePixelRatio > 1.5 ? 640 : 480);
/** The size the cover asks for now. */
export const artPxNow = (): number => Math.max(restArtPx(), fullPx);

/** The full player's ask (0: back to the rest size). */
export function setFullArtPx(px: number): void {
  if (px === fullPx) return;
  fullPx = px;
  subs.forEach((cb) => cb());
}
export function onArtPx(cb: () => void): () => void {
  subs.add(cb);
  return () => subs.delete(cb);
}

/** The largest file a box needs: its device pixels, up to the next 200 px, at least `base`. */
export function fullArtPx(boxCssPx: number, dpr: number, base: number): number {
  const want = Math.ceil((boxCssPx * (dpr > 0 ? dpr : 1)) / 200) * 200;
  return Math.min(2000, Math.max(base, want));
}
