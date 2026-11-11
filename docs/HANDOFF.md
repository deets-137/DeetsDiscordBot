---
status: sop
desk_test: none
sources: []
updated: 2026-11-11
---
# DeetsDiscordBot — Handoff

> Cold start: read this first. This file holds what is still true. The record of each sitting
> is [WORKLOG.md](WORKLOG.md).

## State of play (2026-11-10, night)
- **Lil Deets is the Rust bot** (his word: "I'll switch to the rust bot entirely now"). Run
  `target\release\lildeets.exe`; build `cargo build --workspace --release` with `CMAKE` set
  (ops/SETUP.md). Three processes: launcher, host, logic (architecture/BOT.md).
- **The Python bot is archived** in `docs/archive/bot/` (BOT.md §9). It is not a fallback:
  it was not run after the move. **Never run two bots on this token.**
- **`rust-rewrite` is merged into main** (`42f76b8`). This checkout is on main.
- **A rebuild goes live by itself:** a new `lildeets-logic.exe` swaps in with no gap; a new
  `lildeets-host.exe` restarts the host at the next song boundary (LIVE-RELOAD.md). So a
  `cargo build --release` while he listens is a live change. `.lildeets-hold` holds both.
- **Network drops are handled by the bot** (ops/DEBUGGING.md §6): a lost call is rejoined
  and DeetsMusic paused meanwhile; the gateway reconnects fast. The drops on this PC are
  partly local (daily ephemeral-port exhaustion, Ethernet at 100 Mbps): the PortWatch logger
  (installed 2026-11-10 by a separate session) waits for the next episode to name the
  program.
- **The nightly job runs from this repo since 2026-11-10** (Python, `journal/`); the first
  run passed. **DeetsOTD is archived** (GitHub read-only, main `81bbf2c`). Its local
  `storage.db` is frozen as the backup (ops/NIGHTLY.md §4).
- **His desk test passed** (2026-11-10 night, "tested and good to go on the desk test"):
  `/vc`, `/dm` with its cards and buttons, layout C, the status line, "Listening to",
  `/links`. Seen live by the log: logic swaps with no gap while playing, leave-when-alone, the
  host restart with no call (04:55, 05:01, 05:09; back in 2–3 s).

## Open now
0. **Two desk tests not yet confirmed** (if his "tested" covered them, mark them passed):
   - **The network drops:** ops/DEBUGGING.md §6 (unplug the cable for 10 s, 60 s, 5 min).
   - **The host restart while a call hears DeetsMusic:** LIVE-RELOAD.md §7.
   - Not seen yet: a refused reply rescinded (no reply has failed since it was built).
1. **twilight:** 0.18.0 is two weeks old on 2026-11-15; check for a newer 0.18.x and its
   advisories then (rust-port-plan.md P8).
2. **The installed DeetsSQL** opens the new `storage.db` path only after a rebuild
   (DeetsSQL `61bf0fc`).
3. **The Activity: steps 1 to 5 of 6 are built** (features/ACTIVITY.md §13, 2026-11-10/11):
   the bridge package is `deetsmusic-bridge`, and `activity/` holds the UI on fake data
   (`npm run mock` there). **Merged into main** (`f360a04`, 2026-11-11, his word) from
   branch `disc-activities`. This checkout is on main.
   **Most of step 2b is built and his desk test of the mock passed** (2026-11-11, §13.7, §12.1): the landscape layout,
   the Play Now button on the toast, the tile card, album colour, hints, drag to reorder
   (a finger holds, then moves), Glass's aurora and Cyber's storm.
   **Step 3 is built and committed (`294e0d1`); the relay is deployed; the bot's side is not
   live** (2026-11-11, §13.8): the relay Worker (`activity/relay`), the sign-in page
   (`activity/discord-shell`), the bot's connection (`crates/relay-client`), the check in the
   logic, the command sync that keeps the Launch command. **Setup steps 1 to 3 of §12.2 are
   done** (2026-11-11): the Worker `lildeets-relay` is on Cloudflare with its three secrets,
   and `.env` has `LILDEETS_RELAY_URL` and `LILDEETS_RELAY_SECRET`. The Worker's address is in
   `.env` only (it names the Cloudflare account: keep it out of this public repo). The client
   secret was reset that day; a copy is in `Documents/Deets' Secrets/`. Checked live: the
   page loads, the origin check, a made-up sign-in code is refused by the real Discord, the
   host socket takes the secret in `.env`. **The release build is made** (2026-11-11 12:00;
   the bot was not running, so it goes live at the bot's next start). **The Developer
   Portal steps are done** (his word, 2026-11-11: "steps 1 to 3 complete": the mapping's
   target, the redirect, Enable Activities). The first launch failed ("Not from the
   Activity"); fixed and deployed (§13.9). **§12.2 steps 1, 2, 3, 5 passed**; 4 and 6 to 11
   are open.
   **Steps 4 and 5 are built and live** (2026-11-11 13:53, §13.10; his calls 1A, 2A, 3B, and
   "A, no line"): the real cards in the call, `deetsmusic_remote`, "Open in call" on the
   `/dm np` card, Play Now and `/dm play` for guests under "Guests may skip" and "Guests may add
   songs" (keeping Up Next). IPC `PROTOCOL` 3, relay WIRE 2, the five cover URL mappings in the
   Portal. **Next: his desk test §12.3** (15 steps). Set aside by his word ("Step 3,
   the relay"): the rest of the scenery (§13.7: the Press record, the wallpaper, Ocean, Happy).
   §11 lists what is not confirmed (a Read-only seat launching in a large server is the first;
   §12.2 steps 10 and 11 test it). `activity/` can be worked on from a Mac (ops/SETUP.md
   §9.1); the bot cannot.
4. Later, by his word: in-call extras, image cards, the tray app (features/TRAY.md, now over
   `lildeets.exe`).

## Retired (2026-11-10)
The harness Discord bot (`../Vibe Coding Era/harness/discord_bot.py`) shared this token. Its
10 guild commands (`/reset` … `/emergency`) were cleared from the three servers that had them.
If it ever runs again, they come back. The harness itself was not touched.

## The desk
- DeetsMusic plays to the Dell monitor's speakers at 100% in the Windows mixer, so the PC is
  quiet and the call hears full level (VOICE-PIPELINE.md §1).
- `../DeetsOTD` is on main (`81bbf2c`, archived). Nothing runs from it. Keep the folder until
  he says.
- Rust stable (MSVC), CMake from the VS 2022 Build Tools; Python 3.12 for the journal.

## Gotchas
- **The capture hears the app after its mixer slider.** A quiet stream is the slider first.
- **`deetsmusic.exe` is several processes** (the app, plus its CLI and MCP from Claude
  sessions). The capture picks the top-most and its WebView2 child; `deets-tap --list` shows
  who has sound.
- **DeetsMusic on AirPlay** has no Windows audio session: the call hears silence.
- **Global commands lag in the client.** Ctrl+R.
- **The Rust host runs as `lildeets-host.run.exe`** (a copy the launcher makes), so cargo can
  write `lildeets-host.exe`. Run alone (not through `lildeets.exe`), the host never restarts
  itself. Each host start syncs the command list once (it clears `.lildeets-commands.sync`).
- Notion: `notion_sync` uses an internal integration that must be connected to the DeetsOTD
  page, or it 403s (guide/USAGE.md §4).
