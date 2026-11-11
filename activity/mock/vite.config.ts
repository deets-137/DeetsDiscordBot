// The mock page (docs/features/ACTIVITY.md §10 step 2): deetsmusic-ui on fake data, in a
// browser, with no Discord and no bot.
//
//   npm run mock         a dev server; Vite takes the next free port from 5273 up
//   npm run mock:build   dist-mock/

import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  // Several Deets apps run on this PC at once: never a strict port.
  server: { port: 5273, strictPort: false, host: "127.0.0.1" },
  build: { outDir: fileURLToPath(new URL("../dist-mock", import.meta.url)), emptyOutDir: true },
});
