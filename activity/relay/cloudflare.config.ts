// The relay Worker (docs/features/ACTIVITY.md §4 to §6): the Activity's page, Discord sign-in,
// and one room that passes messages between the bot and the people in its call.
//
// Secrets are set on the Worker by the owner (`cf workers secrets update`), never in the repo.
// For `cf dev`, copy `.dev.vars.example` to `.dev.vars`.

import { bindings, defineConfig, exports } from "cf/config";
import * as entrypoint from "./src/index.ts" with { type: "cf-worker" };

const NAME = "lildeets-relay";

// `cf dev` (mode "development") talks to the fake Discord of test/relay.test.mjs, never to
// the real one: a local page is not inside Discord, so a real sign-in cannot happen there.
const FAKE_DISCORD = "http://127.0.0.1:5399/api/v10";

export default defineConfig(({ mode }) => ({
  worker: {
    name: NAME,
    compatibilityDate: "2026-09-30",
    entrypoint,
    // The page is the Vite build of index.html. The Worker answers /api/* and /host/* itself.
    assets: { notFoundHandling: "single-page-application", runWorkerFirst: ["/api/*", "/host/*"] },
    env: {
      ROOM: bindings.durableObject({ worker: NAME, exportName: "Room" }),
      /** Discord's API. */
      DISCORD_API: bindings.text(mode === "development" ? FAKE_DISCORD : "https://discord.com/api/v10"),
      /** The application id of Lil Deets. Not a secret, but not in this public repo. */
      DISCORD_CLIENT_ID: bindings.secret(),
      /** The application's OAuth client secret: turns a sign-in code into a person. */
      DISCORD_CLIENT_SECRET: bindings.secret(),
      /** Proves that a connection on /host/ws is the bot. The bot's `.env` holds the same value. */
      HOST_SECRET: bindings.secret(),
    },
    exports: {
      Room: exports.durableObject({ storage: "sqlite" }),
    },
  },
}));
