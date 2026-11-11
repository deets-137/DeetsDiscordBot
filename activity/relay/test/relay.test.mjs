// A local test of the relay: a fake Discord, a fake bot and fake pages against `cf dev`.
// No real Discord call, no real secret (the values are `.dev.vars.example`'s).
//
//   1. copy .dev.vars.example to .dev.vars
//   2. npm run dev          (note the address it prints)
//   3. RELAY=http://127.0.0.1:<port> npm test
//
// The fake Discord listens on port 5399 (cloudflare.config.ts FAKE_DISCORD). Node 22 or newer.

import http from "node:http";
import assert from "node:assert/strict";

const RELAY = process.env.RELAY ?? "http://127.0.0.1:5373";
const FAKE_PORT = 5399; // cloudflare.config.ts FAKE_DISCORD
const CLIENT_ID = "100000000000000001";
const CLIENT_SECRET = "test-client-secret";
const HOST_SECRET = "test-host-secret";
const ORIGIN = "http://127.0.0.1:5373";

// ── the fake Discord: a code "code-<id>-<name>" is that person ─────────────────
const fake = http.createServer(async (req, res) => {
  const body = await new Promise((done) => {
    let s = "";
    req.on("data", (c) => (s += c));
    req.on("end", () => done(s));
  });
  const reply = (status, obj) => {
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(obj));
  };
  if (req.method === "POST" && req.url === "/api/v10/oauth2/token") {
    const p = new URLSearchParams(body);
    const m = /^code-(\d+)-(\w+)$/.exec(p.get("code") ?? "");
    if (p.get("client_id") !== CLIENT_ID || p.get("client_secret") !== CLIENT_SECRET || p.get("grant_type") !== "authorization_code" || !m) {
      return reply(400, { error: "invalid_grant" });
    }
    return reply(200, { access_token: `tok-${m[1]}-${m[2]}`, token_type: "Bearer" });
  }
  if (req.method === "GET" && req.url === "/api/v10/users/@me") {
    const m = /^Bearer tok-(\d+)-(\w+)$/.exec(req.headers.authorization ?? "");
    if (!m) return reply(401, { message: "401: Unauthorized" });
    return reply(200, { id: m[1], username: m[2].toLowerCase(), global_name: m[2] });
  }
  reply(404, { message: "404: Not Found" });
});
await new Promise((ok) => fake.listen(FAKE_PORT, "127.0.0.1", ok));

// ── helpers ────────────────────────────────────────────────────────────────────
const wsURL = (path) => RELAY.replace(/^http/, "ws") + path;

/** A socket with a queue: `next()` gives the next JSON frame, or the close code. */
function socket(path, headers) {
  const ws = new WebSocket(wsURL(path), { headers });
  const got = [];
  const waiters = [];
  const push = (v) => (waiters.length ? waiters.shift()(v) : got.push(v));
  ws.addEventListener("message", (e) => {
    if (e.data !== "pong") push(JSON.parse(e.data));
  });
  ws.addEventListener("close", (e) => push({ closed: e.code }));
  ws.addEventListener("error", () => push({ closed: "error" }));
  return {
    ws,
    opened: new Promise((ok) => ws.addEventListener("open", ok, { once: true })),
    send: (m) => ws.send(JSON.stringify(m)),
    next: (ms = 3000) =>
      got.length
        ? Promise.resolve(got.shift())
        : new Promise((ok, no) => {
            const waiter = (v) => (clearTimeout(t), ok(v));
            // A waiter that timed out leaves the list, or it would swallow the next frame.
            const t = setTimeout(() => {
              waiters.splice(waiters.indexOf(waiter), 1);
              no(new Error(`no frame on ${path} in ${ms} ms`));
            }, ms);
            waiters.push(waiter);
          }),
    quiet: async (ms = 400) => {
      await new Promise((ok) => setTimeout(ok, ms));
      assert.deepEqual(got, [], `unexpected frames on ${path}`);
    },
  };
}

const api = (path, init = {}) => fetch(RELAY + path, { ...init, headers: { origin: ORIGIN, ...(init.headers ?? {}) } });
const signIn = async (id, name) => {
  const res = await api("/api/token", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code: `code-${id}-${name}` }) });
  assert.equal(res.status, 200);
  return res.json();
};
const guest = async (ticket, instance) => {
  const g = socket("/api/ws", { origin: ORIGIN });
  await g.opened;
  g.send({ t: "hello", v: 2, ticket, instance });
  return g;
};

let passed = 0;
const step = async (name, fn) => {
  await fn();
  passed++;
  console.log(`ok   ${name}`);
};

// ── the test ───────────────────────────────────────────────────────────────────
try {
  await step("a request with no Activity origin is refused", async () => {
    const post = { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code: "x" }) };
    assert.equal((await fetch(RELAY + "/api/token", post)).status, 403);
    assert.equal((await fetch(RELAY + "/api/token", { ...post, headers: { ...post.headers, origin: "https://evil.example" } })).status, 403);
  });

  await step("the old config route is gone (the page reads the app id from its address)", async () => {
    assert.equal((await api("/api/config")).status, 404);
  });

  await step("a bad sign-in code is refused", async () => {
    const res = await api("/api/token", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code: "nope" }) });
    assert.equal(res.status, 401);
  });

  const alice = await signIn("200000000000000001", "Alice");
  await step("a good code gives the person Discord named, and a ticket", async () => {
    assert.deepEqual(alice.user, { id: "200000000000000001", name: "Alice" });
    assert.equal(alice.access_token, "tok-200000000000000001-Alice");
    assert.match(alice.ticket, /^[\w-]+\.[\w-]+$/);
  });

  await step("the host's socket needs the host secret", async () => {
    const bad = socket("/host/ws", { authorization: "Bearer wrong" });
    assert.equal((await bad.next()).closed !== undefined, true);
    const none = socket("/host/ws", {});
    assert.equal((await none.next()).closed !== undefined, true);
  });

  await step("a forged ticket is refused", async () => {
    const [body] = alice.ticket.split(".");
    const forged = Buffer.from(JSON.stringify({ u: "999", n: "Mallory", exp: Date.now() + 60_000 })).toString("base64url");
    for (const t of [`${forged}.${alice.ticket.split(".")[1]}`, `${body}.AAAA`, "junk"]) {
      const g = await guest(t, "i-1");
      assert.equal((await g.next()).t, "refused");
      assert.equal((await g.next()).closed, 4003);
    }
  });

  const a = await guest(alice.ticket, "i-100-gc-1-10");
  await step("with no host, a person gets one line: the host is not here", async () => {
    assert.deepEqual(await a.next(), { t: "welcome", user: alice.user, host: false });
    await a.quiet();
  });

  const host = socket("/host/ws", { authorization: `Bearer ${HOST_SECRET}` });
  await host.opened;
  await step("a new host is asked to check the person who waits", async () => {
    const check = await host.next();
    assert.equal(check.t, "check");
    assert.equal(check.user, alice.user.id);
    assert.equal(check.instance, "i-100-gc-1-10");
    host.send({ t: "checked", rid: check.rid, ok: true, channel: "10" });
    assert.deepEqual(await host.next(), { t: "joined", rid: check.rid, user: alice.user.id, name: "Alice", channel: "10" });
    await a.quiet(); // the bot is in no channel yet: nothing changed for Alice
  });

  await step("a page's data is dropped while the bot is not in its call", async () => {
    a.send({ t: "data", data: { q: 1, search: "x" } });
    await host.quiet();
  });

  await step("the bot comes into the person's channel: the page is told", async () => {
    host.send({ t: "channels", channels: ["10"] });
    assert.deepEqual(await a.next(), { t: "host", here: true });
    host.send({ t: "channels", channels: ["10"] }); // the same again: no frame
    await a.quiet();
  });

  const bob = await signIn("200000000000000002", "Bob");
  await step("a person in another channel is welcomed with the host away", async () => {
    const b = await guest(bob.ticket, "i-200-gc-1-20");
    const check = await host.next();
    assert.equal(check.user, bob.user.id);
    host.send({ t: "checked", rid: check.rid, ok: true, channel: "20" });
    assert.equal((await host.next()).t, "joined");
    assert.deepEqual(await b.next(), { t: "welcome", user: bob.user, host: false });
    b.ws.close();
    assert.deepEqual(await host.next(), { t: "left", rid: check.rid });
  });

  await step("a person Discord does not confirm is refused", async () => {
    const c = await guest(bob.ticket, "i-300-gc-1-10");
    const check = await host.next();
    host.send({ t: "checked", rid: check.rid, ok: false, reason: "You are not in this Activity." });
    assert.deepEqual(await c.next(), { t: "refused", reason: "You are not in this Activity." });
    assert.equal((await c.next()).closed, 4003);
  });

  await step("a second frame from a page closes its socket", async () => {
    a.send({ t: "hello", v: 2, ticket: alice.ticket, instance: "i-100-gc-1-10" });
    assert.equal((await a.next()).closed, 4002);
    assert.equal((await host.next()).t, "left");
  });

  const a2 = await guest(alice.ticket, "i-100-gc-1-10");
  let a2rid;
  {
    const check = await host.next();
    a2rid = check.rid;
    host.send({ t: "checked", rid: check.rid, ok: true, channel: "10" });
    assert.equal((await host.next()).t, "joined");
    assert.deepEqual(await a2.next(), { t: "welcome", user: alice.user, host: true });
  }

  await step("a page's data reaches the bot; the bot's reply reaches that page only", async () => {
    a2.send({ t: "data", data: { q: 1, search: "kiss" } });
    assert.deepEqual(await host.next(), { t: "data", rid: a2rid, data: { q: 1, search: "kiss" } });
    host.send({ t: "to", rid: a2rid, data: { kind: "found", q: 1, tracks: [] } });
    assert.deepEqual(await a2.next(), { t: "data", data: { kind: "found", q: 1, tracks: [] } });
  });

  await step("the state goes to each page in the call, and a newcomer gets the last one", async () => {
    host.send({ t: "state", data: { kind: "state", n: 1 } });
    assert.deepEqual(await a2.next(), { t: "data", data: { kind: "state", n: 1 } });
    const a3 = await guest(alice.ticket, "i-100-gc-1-10");
    const check = await host.next();
    host.send({ t: "checked", rid: check.rid, ok: true, channel: "10" });
    assert.equal((await host.next()).t, "joined");
    assert.deepEqual(await a3.next(), { t: "welcome", user: alice.user, host: true });
    assert.deepEqual(await a3.next(), { t: "data", data: { kind: "state", n: 1 } });
    a3.ws.close();
    assert.deepEqual(await host.next(), { t: "left", rid: check.rid });
  });

  await step("a page that floods is cut off for the rest of its window", async () => {
    for (let i = 0; i < 35; i++) a2.send({ t: "data", data: { q: 100 + i } });
    let got = 0;
    for (;;) {
      try {
        const f = await host.next(600);
        if (f.t === "data") got++;
      } catch {
        break;
      }
    }
    // The window counts the frame of the step before too (one), so 29 of these pass.
    assert.equal(got, 29);
  });

  await step("a second host replaces the first, and checks everyone again", async () => {
    const host2 = socket("/host/ws", { authorization: `Bearer ${HOST_SECRET}` });
    await host2.opened;
    assert.equal((await host.next()).closed, 4000);
    const check = await host2.next();
    assert.equal(check.user, alice.user.id);
    host2.send({ t: "checked", rid: check.rid, ok: true, channel: "10" });
    assert.equal((await host2.next()).t, "joined");
    // The new host has named no channel yet: for Alice the bot is away until it does.
    assert.deepEqual(await a2.next(), { t: "host", here: false });
    host2.send({ t: "channels", channels: ["10"] });
    assert.deepEqual(await a2.next(), { t: "host", here: true });

    host2.ws.close();
    assert.deepEqual(await a2.next(), { t: "host", here: false });
  });

  console.log(`\n${passed} steps passed.`);
  process.exitCode = 0;
} catch (e) {
  console.error(`\nFAILED after ${passed} steps:`, e);
  process.exitCode = 1;
} finally {
  fake.close();
  setTimeout(() => process.exit(), 200);
}
