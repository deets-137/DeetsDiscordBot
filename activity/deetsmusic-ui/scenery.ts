// Skin scenery behind the cards: the layers a skin moves with CSS alone. The look hides each
// layer unless the skin asks for it, so this file names no skin.
//
//   Glass  the aurora: three blobs that drift (layout.ts puts the layer on the page).
//   Cyber  the storm: two bolts. Their motion is CSS; `initStorm` gives each strike a new
//          place and flip at the loop's seam, while the bolt is dark (DeetsMusic's storm.ts).
//
// `initAmbient` holds every endless loop still while the frame cannot be seen
// (`data-ambient="paused"`, as DeetsMusic's ambient.ts). The app asks its window; a page asks
// the browser. It logs each change, as anything that acts by itself does.
//
// Not here yet (ACTIVITY.md §13.6): Ocean's sea, Glass's wallpaper, Happy's scene and the
// Press record's turn. Each needs a larger port of the app's own driver.

const root = document.documentElement;

export function initStorm(): void {
  document.querySelectorAll<HTMLElement>(".storm__strike").forEach((strike) => {
    const roll = (): void => {
      // The storm's viewBox is 0..100 across, so "8..92px" is 8 % to 92 % of the width.
      strike.style.setProperty("--storm-x", `${Math.round(8 + Math.random() * 84)}px`);
      strike.style.setProperty("--storm-flip", Math.random() < 0.5 ? "-1" : "1");
    };
    roll();
    strike.addEventListener("animationiteration", (e) => {
      if (e.target === strike) roll(); // the inner hold box's own event bubbles up: not that one
    });
  });
}

export function initAmbient(): void {
  const check = (): void => {
    const paused = document.hidden;
    if (paused === "ambient" in root.dataset) return;
    if (paused) root.dataset.ambient = "paused";
    else delete root.dataset.ambient;
    console.info(`[deetsmusic-ui] ambient ${paused ? "paused: the frame is hidden" : "running: the frame is in view"}`);
  };
  document.addEventListener("visibilitychange", check);
  check();
}
