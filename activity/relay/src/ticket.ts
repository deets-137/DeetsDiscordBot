// A ticket: who a person is, signed by the relay, good for five minutes. `POST /api/token`
// makes one after Discord said who the sign-in code belongs to; the socket's first frame
// brings it back. So the socket never trusts the page's own claim of a user id, and the
// relay asks Discord once per sign-in, not once per connection.
//
// HMAC-SHA256 under a key made from the host secret (a different key from the secret itself:
// the label below goes into the key's derivation).

const TTL_MS = 5 * 60_000;
const LABEL = "lildeets-relay ticket v1";
const enc = new TextEncoder();

const b64url = (bytes: ArrayBuffer | Uint8Array): string => {
  let s = "";
  for (const b of new Uint8Array(bytes as ArrayBuffer)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};
const unb64url = (s: string): Uint8Array => {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
};

async function key(secret: string): Promise<CryptoKey> {
  const base = await crypto.subtle.importKey("raw", enc.encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const derived = await crypto.subtle.sign("HMAC", base, enc.encode(LABEL));
  return crypto.subtle.importKey("raw", derived, { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);
}

export interface Who {
  id: string;
  name: string;
}

export async function makeTicket(secret: string, who: Who, now = Date.now()): Promise<string> {
  const body = b64url(enc.encode(JSON.stringify({ u: who.id, n: who.name, exp: now + TTL_MS })));
  const sig = await crypto.subtle.sign("HMAC", await key(secret), enc.encode(body));
  return `${body}.${b64url(sig)}`;
}

/** The person a ticket names, or null when it is not ours or is too old. */
export async function readTicket(secret: string, ticket: string, now = Date.now()): Promise<Who | null> {
  const [body, sig, extra] = ticket.split(".");
  if (!body || !sig || extra !== undefined) return null;
  try {
    // `verify` compares in constant time.
    if (!(await crypto.subtle.verify("HMAC", await key(secret), unb64url(sig), enc.encode(body)))) return null;
    const p = JSON.parse(new TextDecoder().decode(unb64url(body))) as { u?: unknown; n?: unknown; exp?: unknown };
    if (typeof p.u !== "string" || typeof p.n !== "string" || typeof p.exp !== "number" || p.exp < now) return null;
    return { id: p.u, name: p.n };
  } catch {
    return null;
  }
}

/** Equal strings, in time that depends only on the lengths (both are hashed first). */
export async function sameSecret(a: string, b: string): Promise<boolean> {
  const [x, y] = await Promise.all([crypto.subtle.digest("SHA-256", enc.encode(a)), crypto.subtle.digest("SHA-256", enc.encode(b))]);
  const u = new Uint8Array(x);
  const v = new Uint8Array(y);
  let diff = 0;
  for (let i = 0; i < u.length; i++) diff |= u[i]! ^ v[i]!;
  return diff === 0;
}
