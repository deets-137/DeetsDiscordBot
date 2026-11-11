import { cloudflare } from "@cloudflare/vite-plugin";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [cloudflare()],
  server: {
    // Several Deets apps run on this PC at once: never a strict port.
    port: 5373,
    strictPort: false,
    host: "127.0.0.1",
    // The page's code is in ../discord-shell.
    fs: { allow: [".."] },
  },
});
