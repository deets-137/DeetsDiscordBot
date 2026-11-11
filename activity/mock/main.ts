// The mock page: deetsmusic-ui on a fake host, plus a small desk panel (the ⚙ at the bottom
// left) to change what a real host would decide: the theme, the skin, host or guest, each
// Rooms rule, the stream on or off, and the cover-only view.

import { mountDeetsMusicUI } from "../deetsmusic-ui";
import type { GuestRules } from "../deetsmusic-protocol";
import { mockHost } from "./transport";

const host = mockHost();
const ui = mountDeetsMusicUI(host);

const THEMES = ["lilac", "green", "sepia", "moonlight", "black-yellow", "black-red"];
const SKINS = ["press", "vanilla", "glass", "ocean", "cyber", "metal", "happy"];
const RULES: [keyof GuestRules, string][] = [
  ["playPause", "Guests may play and pause"],
  ["skip", "Guests may skip"],
  ["add", "Guests may add songs"],
  ["changeQueue", "Guests may change Up Next"],
  ["seek", "Guests may seek"],
];

const desk = document.createElement("div");
desk.style.cssText = "position:fixed;left:8px;bottom:8px;z-index:100;font:12px system-ui;color:#111";
const options = (list: string[], cur: string): string => list.map((v) => `<option${v === cur ? " selected" : ""}>${v}</option>`).join("");
const s = host.state();
desk.innerHTML = `
  <button id="desk-toggle" type="button" title="The mock desk" style="width:28px;height:28px;border-radius:14px;border:1px solid #888;background:#fff;cursor:pointer">⚙</button>
  <div id="desk-panel" hidden style="position:absolute;left:0;bottom:34px;width:230px;padding:8px;background:#fff;border:1px solid #888;border-radius:8px;display:grid;gap:6px">
    <label>Theme <select id="desk-theme">${options(THEMES, s.theme)}</select></label>
    <label>Skin <select id="desk-skin">${options(SKINS, s.skin)}</select></label>
    <label><input type="checkbox" id="desk-host"> I am the host</label>
    <label><input type="checkbox" id="desk-active" checked> The host is playing here</label>
    <label><input type="checkbox" id="desk-cover"> Cover only (picture-in-picture, grid)</label>
    ${RULES.map(([k, label]) => `<label><input type="checkbox" data-rule="${k}" checked> ${label}</label>`).join("")}
    <span id="desk-size"></span>
  </div>`;
document.body.appendChild(desk);

const $ = <T extends HTMLElement>(id: string): T => desk.querySelector<T>(`#${id}`)!;
const panel = $("desk-panel");
panel.style.display = "none";
$("desk-toggle").addEventListener("click", () => {
  panel.style.display = panel.style.display === "none" ? "grid" : "none";
});
$<HTMLSelectElement>("desk-theme").addEventListener("change", (e) => host.set({ theme: (e.target as HTMLSelectElement).value }));
$<HTMLSelectElement>("desk-skin").addEventListener("change", (e) => host.set({ skin: (e.target as HTMLSelectElement).value }));
$<HTMLInputElement>("desk-host").addEventListener("change", (e) => host.set({ host: (e.target as HTMLInputElement).checked }));
$<HTMLInputElement>("desk-active").addEventListener("change", (e) => host.set({ active: (e.target as HTMLInputElement).checked }));
$<HTMLInputElement>("desk-cover").addEventListener("change", (e) => ui.setCover((e.target as HTMLInputElement).checked));
desk.querySelectorAll<HTMLInputElement>("[data-rule]").forEach((box) =>
  box.addEventListener("change", () => host.set({ rules: { [box.dataset.rule as keyof GuestRules]: box.checked ? "everyone" : "host" } })),
);

const size = $("desk-size");
const showSize = (): void => {
  const d = document.documentElement.dataset;
  size.textContent = `${window.innerWidth} × ${window.innerHeight} · ${d.surface}${d.surface === "mini" ? ` (${"cover" in d ? "cover" : d.mini})` : ""}`;
};
window.addEventListener("resize", showSize);
new MutationObserver(showSize).observe(document.documentElement, { attributes: true });
showSize();

// `?cover` opens on the cover alone (mock/frames.html uses it for the tile).
if (new URLSearchParams(location.search).has("cover")) {
  $<HTMLInputElement>("desk-cover").checked = true;
  ui.setCover(true);
  desk.hidden = true;
}

// For a check from the browser's console.
Object.assign(window, { mock: host, ui });
