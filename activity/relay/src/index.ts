// The relay Worker's routes (docs/features/ACTIVITY.md §5, §6).
//
//   GET  /api/config   the application id, for the page's Discord SDK
//   POST /api/token    {code} → Discord says who it is → {access_token, user, ticket}
//   GET  /api/ws       a page's socket to the room (first frame: the ticket)
//   GET  /host/ws      the bot's socket to the room (Authorization: Bearer <host secret>)
//   anything else      the page (static assets)
//
// A page reaches the Worker through Discord's proxy (`<app id>.discordsays.com`), so the
// Worker never sees a person's IP address. The proxy's origin is checked on /api/*.

import { env } from "cloudflare:workers";
import { Room } from "./room.ts";
import { makeTicket, sameSecret } from "./ticket.ts";
import type { TokenReply } from "./wire.ts";

export { Room };

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

/** The one room: every call the bot is in shares it (the bot holds one socket). */
const room = (): DurableObjectStub => env.ROOM.get(env.ROOM.idFromName("room"));

/** A page's request must come from this app's frame in Discord, or from a local dev page. */
function fromOurPage(req: Request): boolean {
  const origin = req.headers.get("origin");
  if (!origin) return false;
  if (origin === `https://${env.DISCORD_CLIENT_ID}.discordsays.com`) return true;
  // `cf dev`: the fake Discord is the sign that this is not production.
  const local = /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/;
  return local.test(origin) && local.test(new URL(env.DISCORD_API).origin);
}

async function token(req: Request): Promise<Response> {
  let code: unknown;
  try {
    code = ((await req.json()) as { code?: unknown }).code;
  } catch {
    return json({ error: "The request is not JSON." }, 400);
  }
  if (typeof code !== "string" || !code || code.length > 512) return json({ error: "No sign-in code." }, 400);

  // The code → a token. Only the holder of the client secret can do this.
  const traded = await fetch(`${env.DISCORD_API}/oauth2/token`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      client_id: env.DISCORD_CLIENT_ID,
      client_secret: env.DISCORD_CLIENT_SECRET,
      grant_type: "authorization_code",
      code,
    }),
  });
  if (!traded.ok) {
    console.warn(`[relay] Discord refused a sign-in code: ${traded.status}`);
    return json({ error: "Discord did not accept the sign-in." }, 401);
  }
  const access_token = ((await traded.json()) as { access_token?: unknown }).access_token;
  if (typeof access_token !== "string") return json({ error: "Discord did not accept the sign-in." }, 401);

  // The token → the person. Discord says who; the page's own claim is never used.
  const me = await fetch(`${env.DISCORD_API}/users/@me`, { headers: { authorization: `Bearer ${access_token}` } });
  if (!me.ok) return json({ error: "Discord did not say who signed in." }, 401);
  const u = (await me.json()) as { id?: unknown; username?: unknown; global_name?: unknown };
  if (typeof u.id !== "string" || !/^\d{1,20}$/.test(u.id)) return json({ error: "Discord did not say who signed in." }, 401);
  const name = (typeof u.global_name === "string" && u.global_name) || (typeof u.username === "string" && u.username) || "Someone";
  const user = { id: u.id, name: name.slice(0, 64) };

  const reply: TokenReply = { access_token, user, ticket: await makeTicket(env.HOST_SECRET, user) };
  return json(reply);
}

export default {
  async fetch(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const upgrade = req.headers.get("upgrade")?.toLowerCase() === "websocket";

    if (url.pathname === "/host/ws") {
      if (!upgrade) return json({ error: "A WebSocket is expected." }, 426);
      const auth = req.headers.get("authorization") ?? "";
      const given = auth.startsWith("Bearer ") ? auth.slice(7) : "";
      if (!given || !(await sameSecret(given, env.HOST_SECRET))) return json({ error: "Not the host." }, 401);
      return room().fetch(new Request("https://room/host", req));
    }

    if (url.pathname.startsWith("/api/")) {
      if (!fromOurPage(req)) {
        // Logged, so a proxy that changes the Origin header shows in the Worker's log at once.
        console.warn(`[relay] refused ${url.pathname}: origin ${req.headers.get("origin") ?? "(none)"}`);
        return json({ error: "Not from the Activity." }, 403);
      }
      if (url.pathname === "/api/config" && req.method === "GET") return json({ client_id: env.DISCORD_CLIENT_ID });
      if (url.pathname === "/api/token" && req.method === "POST") return token(req);
      if (url.pathname === "/api/ws") {
        if (!upgrade) return json({ error: "A WebSocket is expected." }, 426);
        return room().fetch(new Request("https://room/guest", req));
      }
      return json({ error: "No such route." }, 404);
    }

    return json({ error: "No such route." }, 404); // assets answer everything else first
  },
} satisfies ExportedHandler;
