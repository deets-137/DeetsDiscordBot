// The look: DeetsMusic's theme and skin on <html>, as its own theme.ts and skin.ts paint them.
// The host's choice arrives in the state (ACTIVITY.md §7); nobody picks a look here.

const THEMES = ["lilac", "green", "sepia", "moonlight", "black-yellow", "black-red"];
const SKINS = ["press", "vanilla", "glass", "ocean", "cyber", "metal", "happy"];
// DeetsMusic's skin.ts: each `--hook-<name>` token a skin sets is copied onto <html> as
// `data-hook-<name>`, and the look's rules ask for the hook, never for a skin by name.
const HOOKS = ["scrub", "material", "sea", "record", "wallpaper", "scene", "light"];

const root = document.documentElement;

function paintHooks(): void {
  const cs = getComputedStyle(root);
  for (const h of HOOKS) {
    const v = cs.getPropertyValue(`--hook-${h}`).trim();
    const key = `hook${h[0].toUpperCase()}${h.slice(1)}`;
    const want = v && v !== "none" ? v : undefined;
    if (root.dataset[key] === want) continue;
    if (want === undefined) delete root.dataset[key];
    else root.dataset[key] = want;
  }
}

/** Paint the host's theme and skin. An id this copy of the look does not have keeps the last one. */
export function paintLook(theme: string, skin: string): void {
  if (THEMES.includes(theme)) root.dataset.theme = theme;
  if (SKINS.includes(skin)) root.dataset.skin = skin;
  paintHooks();
}

export function initLook(): void {
  if (!root.dataset.theme) root.dataset.theme = "lilac";
  if (!root.dataset.skin) root.dataset.skin = "press";
  paintHooks();
  // The stylesheet may land after this module in dev (Vite injects it): read once more then.
  window.addEventListener("load", paintHooks, { once: true });
}
