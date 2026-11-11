// A toast with DeetsMusic's markup (its styles/toast.css draws it). Used for one thing: the
// host's own line when it refuses an action, shown to this person only.

import "./look/src/styles/toast.css";

let host: HTMLElement | null = null;
const HOLD_MS = 5000;

export function toast(text: string, kind: "info" | "warn" | "error" = "warn"): void {
  if (!host) {
    host = document.createElement("div");
    host.className = "toast-host";
    host.setAttribute("aria-live", "polite");
    document.body.appendChild(host);
  }
  // The same line twice in a row stays one toast.
  const last = host.lastElementChild as HTMLElement | null;
  if (last?.dataset.text === text) return;

  const el = document.createElement("div");
  el.className = `toast toast--${kind}`;
  el.setAttribute("role", kind === "error" ? "alert" : "status");
  el.dataset.text = text;
  const p = document.createElement("p");
  p.className = "toast__text";
  p.textContent = text;
  el.appendChild(p);
  host.appendChild(el);
  window.setTimeout(() => el.remove(), HOLD_MS);
}
