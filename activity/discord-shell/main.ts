// discord-shell: Discord's side of the Activity's page: the SDK, the sign-in, the socket to
// the relay (docs/features/ACTIVITY.md §4 to §6). It knows nothing of DeetsMusic.
//
// Build step 3: it signs the person in and says whether the bot is in their call. No cards.
//
// Every name from Discord is drawn with `textContent`.

import { DiscordSDK } from "@discord/embedded-app-sdk";
import { WIRE, type GuestHello, type ToGuest, type TokenReply } from "../relay/src/wire";

const $ = (id: string): HTMLElement => document.getElementById(id)!;
const show = (who: string, status = "", detail = ""): void => {
  $("who").textContent = who;
  $("status").textContent = status;
  $("detail").textContent = detail;
};

// Inside Discord the page is served from `<app id>.discordsays.com`, and every request to the
// relay goes through the proxy's `/.proxy` path. Outside it (a local check), the relay is direct.
const inDiscord = location.hostname.endsWith(".discordsays.com");
const base = inDiscord ? "/.proxy" : "";
const HOST_HERE = "Lil Deets is in this call.";
const HOST_AWAY = "The host is not playing here";

async function getJSON<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${base}${path}`, init);
  const body = (await res.json().catch(() => ({}))) as T & { error?: string };
  if (!res.ok) throw new Error(body.error ?? `The relay answered ${res.status}.`);
  return body;
}

async function main(): Promise<void> {
  if (!new URLSearchParams(location.search).has("frame_id")) {
    return show("Open this from Discord", "This page is Lil Deets' Activity. It works inside a Discord call.");
  }

  const { client_id } = await getJSON<{ client_id: string }>("/api/config");
  const sdk = new DiscordSDK(client_id);
  await sdk.ready();

  // Discord asks the person once. `rpc.activities.write` is for the song on their profile
  // (build step 5); asking now means they agree one time only.
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
  show(`Signed in as ${signed.user.name}`, "Asking the relay…");

  const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${base}/api/ws`);
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
    if (msg.t === "welcome") show(`Signed in as ${msg.user.name}`, msg.host ? HOST_HERE : HOST_AWAY);
    else if (msg.t === "host") $("status").textContent = msg.here ? HOST_HERE : HOST_AWAY;
    else if (msg.t === "refused") {
      refused = true;
      show("Not signed in", msg.reason);
    }
  });
  ws.addEventListener("close", (e) => {
    if (!refused) $("detail").textContent = `The connection to the relay closed (${e.code}). Close the Activity and open it again.`;
  });
  // A keep-alive the relay answers without waking (room.ts). Discord's proxy drops idle sockets.
  const beat = window.setInterval(() => (ws.readyState === WebSocket.OPEN ? ws.send("ping") : window.clearInterval(beat)), 30_000);
}

main().catch((e: unknown) => {
  console.error("[discord-shell]", e);
  show("Sign-in did not work", e instanceof Error ? e.message : String(e), "Close the Activity and open it again.");
});
