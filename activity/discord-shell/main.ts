// discord-shell: Discord's side of the Activity's page: the SDK, the sign-in, the socket to
// the relay, the layout modes and Rich Presence (docs/features/ACTIVITY.md §4 to §6, §9).
// The cards are deetsmusic-ui, on the relay's transport (relay-transport.ts).
//
// Every name from Discord is drawn with `textContent`.

import { DiscordSDK, Events } from "@discord/embedded-app-sdk";
import { artworkURL, type HostState, type Request } from "../deetsmusic-protocol";
import { mountDeetsMusicUI, type DeetsMusicUI } from "../deetsmusic-ui";
import { WIRE, type GuestHello, type ToGuest, type TokenReply } from "../relay/src/wire";
import { relayTransport, type RelayTransport } from "./relay-transport";

const $ = (id: string): HTMLElement => document.getElementById(id)!;

/** The sign-in card over the page: shown until the cards mount, and again on a failure. */
const gate = (who: string, status = "", detail = ""): void => {
  $("gate").hidden = false;
  $("who").textContent = who;
  $("status").textContent = status;
  $("detail").textContent = detail;
};

// Inside Discord the page is served from `<app id>.discordsays.com`, and every request to the
// relay goes through the proxy's `/.proxy` path.
const inDiscord = location.hostname.endsWith(".discordsays.com");
const base = inDiscord ? "/.proxy" : "";

async function getJSON<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${base}${path}`, init);
  const body = (await res.json().catch(() => ({}))) as T & { error?: string };
  if (!res.ok) throw new Error(body.error ?? `The relay answered ${res.status}.`);
  return body;
}

async function main(): Promise<void> {
  if (!new URLSearchParams(location.search).has("frame_id")) {
    return gate("Open this from Discord", "This page is Lil Deets' Activity. It works inside a Discord call.");
  }

  // Discord serves the page from `<app id>.discordsays.com`, so the address names the app.
  // (It was once a GET to the relay, but a same-origin GET carries no Origin header, and the
  // relay refuses a request with none: ACTIVITY.md §13.9.)
  const client_id = location.hostname.split(".")[0];
  if (!inDiscord || !/^\d{1,20}$/.test(client_id)) throw new Error("This page is not on Discord's Activity address.");
  const sdk = new DiscordSDK(client_id);
  await sdk.ready();

  // Discord asks the person once. `rpc.activities.write` is for the song on their profile.
  const { code } = await sdk.commands.authorize({
    client_id,
    response_type: "code",
    state: "",
    prompt: "none",
    scope: ["identify", "rpc.activities.write"],
  });
  const signed = await getJSON<TokenReply>("/api/token", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code }),
  });
  await sdk.commands.authenticate({ access_token: signed.access_token });
  gate(`Signed in as ${signed.user.name}`, "Asking the relay…");

  const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${base}/api/ws`);
  const send = (data: Request): void => {
    if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ t: "data", data }));
  };
  const transport = relayTransport(send, { name: signed.user.name });
  let ui: DeetsMusicUI | null = null;

  ws.addEventListener("open", () => {
    ws.send(JSON.stringify({ t: "hello", v: WIRE, ticket: signed.ticket, instance: sdk.instanceId } satisfies GuestHello));
  });
  let refused = false;
  ws.addEventListener("message", (e) => {
    let msg: ToGuest;
    try {
      msg = JSON.parse(String(e.data)) as ToGuest;
    } catch {
      return;
    }
    if (msg.t === "welcome") {
      transport.setHere(msg.host);
      if (!ui) {
        ui = mountDeetsMusicUI(transport);
        $("gate").hidden = true;
        watchLayout(sdk, ui);
        watchPresence(sdk, transport);
      }
    } else if (msg.t === "host") transport.setHere(msg.here);
    else if (msg.t === "data") transport.receive(msg.data);
    else if (msg.t === "refused") {
      refused = true;
      gate("Not signed in", msg.reason);
    }
  });
  ws.addEventListener("close", (e) => {
    transport.setHere(false);
    if (!refused) gate("The connection to the relay closed", `Code ${e.code}.`, "Close the Activity and open it again.");
  });
  // A keep-alive the relay answers without waking (room.ts). Discord's proxy drops idle sockets.
  const beat = window.setInterval(() => (ws.readyState === WebSocket.OPEN ? ws.send("ping") : window.clearInterval(beat)), 30_000);
}

/** Picture-in-picture (1) and the grid (2) show the tile card; focused (0) the full UI
 *  (ACTIVITY.md §3). An unknown mode (-1) changes nothing. */
function watchLayout(sdk: DiscordSDK, ui: DeetsMusicUI): void {
  void sdk
    .subscribe(Events.ACTIVITY_LAYOUT_MODE_UPDATE, ({ layout_mode }) => {
      if (layout_mode === 0) ui.setTile(false);
      else if (layout_mode === 1 || layout_mode === 2) ui.setTile(true);
    })
    .catch((e: unknown) => console.warn("[discord-shell] no layout mode events", e));
}

/** The song on this person's profile while the Activity is open (his call P). Set on a change
 *  of song or of play and pause only, never on a timer. */
function watchPresence(sdk: DiscordSDK, transport: RelayTransport): void {
  let last = "";
  transport.subscribe((s: HostState) => {
    const raw = transport.raw();
    const t = s.active && raw ? raw.now.track : null;
    const key = t ? `${t.catalogId}|${t.title}|${raw!.now.playing}` : "none";
    if (key === last) return;
    last = key;
    const activity = t
      ? {
          type: 2, // Listening
          details: t.title.slice(0, 128),
          state: (raw!.now.playing ? t.artistName : `Paused · ${t.artistName}`).slice(0, 128),
          assets: {
            large_image: artworkURL(t, 512) ?? undefined,
            large_text: (t.albumName ?? t.title).slice(0, 128),
          },
          timestamps:
            raw!.now.playing && t.durationMs
              ? { start: Math.round(s.now.at - s.now.positionMs), end: Math.round(s.now.at - s.now.positionMs + t.durationMs) }
              : undefined,
        }
      : { type: 2, details: "In the call with Lil Deets", state: "Nothing is playing" };
    sdk.commands.setActivity({ activity }).catch((e: unknown) => console.warn("[discord-shell] Rich Presence was not set", e));
  });
}

main().catch((e: unknown) => {
  console.error("[discord-shell]", e);
  gate("Sign-in did not work", e instanceof Error ? e.message : String(e), "Close the Activity and open it again.");
});
