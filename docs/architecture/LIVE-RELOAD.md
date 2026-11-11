---
status: shipped
desk_test: passed 2026-11-10 (restart with no call); open (restart while a call hears DeetsMusic)
sources: [crates/launcher/src/main.rs, crates/host/src/main.rs, crates/logic/src/main.rs]
updated: 2026-11-10
---
# Live reload: code changes go live while the music plays

## 1. The ask (the owner, 2026-11-10)
"Any way to hotswap the code / runtime to keep music playing even as slash commands and cards
get updated? Or to set it up such that it occurs when the song pauses / moves to the next song
so it feels seamless?" Chose **automatic**.

## 2. The processes
Rust can't reload code in place safely, so the bot is split where a restart would hurt
(architecture/BOT.md §1):
- **`lildeets.exe`, the launcher**, runs the host as a copy and starts it again whenever it
  exits with **75**. Any other exit (a crash, Ctrl+C) ends the launcher too, so a broken
  build never loops.
- **The host** holds the gateway and every call. It runs as `lildeets-host.run.exe`.
- **The logic** holds every command and card. It runs as `lildeets-logic.run-<n>.exe`.

The copies leave `lildeets-host.exe` and `lildeets-logic.exe` free, so `cargo build
--workspace --release` can write them while the bot plays. A build **is** a live change.

## 3. What happens on a build
The host looks at both originals every 2 s. A change is acted on once the file has stayed
the same for one more look.

| Changed | What happens | Gap |
|---|---|---|
| `lildeets-logic.exe` | **Swap:** the host starts the new copy beside the old, waits for its `Hello`, then stops the old one. The call never notices. | none (≈ 0.5 s for commands) |
| `lildeets-host.exe` | **Restart at a song boundary** (§4). | ≈ 2–3 s, between songs |
| `lildeets.exe` | Nothing: a running launcher is never replaced. Stop and start it by hand. | — |

Cards posted before a swap keep working: their buttons carry fixed ids
(`dm:np:*`, `dm:queue:<page>:<prev|next>`, `dm:pick:*`) that any logic understands.

## 4. The song-boundary restart (`crates/host`)
1. Playing DeetsMusic into a call: wait until the song changes or someone pauses (checked
   each second through the bridge). Not playing DeetsMusic: go now.
2. If DeetsMusic plays, pause it through the bridge.
3. Write `.lildeets-resume.json` (beside, flushed, renamed over): each call, its app and gain,
   any call still being rejoined after a drop, and whether we paused DeetsMusic. Leave every
   call; exit 75.
4. The launcher copies the new host and starts it. On its first GUILD_CREATE it reads the
   hand-off (ignored if older than 3 min) and rejoins each call through the drop path
   (ops/DEBUGGING.md §6): the "anyone still there?" check, retries, then the app. Once a call
   hears DeetsMusic again, it waits 2 s (`DAVE_SETTLE`: songbird gives no DAVE-ready signal)
   and presses play, if it was the one that paused.

Each host start syncs the command list once (BOT.md §4).

## 5. Rules for the code
- **Never keep voice state in the logic.** It is swapped while the music plays. Calls,
  captures, gain and presence live in the host; the logic asks for them over IPC.
- **Keep the host small and rare to change.** Every host change costs a gap between songs.
- **A change that spans several builds:** create `.lildeets-hold` at the repo root first;
  while it exists, neither a swap nor a restart happens; delete it to let them go.
- The resume, sync and hold files are gitignored.

## 6. Seen live (2026-11-10, night)
- Logic swaps at 04:14, 04:20, 04:37, 04:55 and 05:01, the first two while DeetsMusic played
  into a call in Moshi's: no gap.
- Host restarts at 04:55, 05:01 and 05:09, with no call: back with the commands synced in
  2–3 s.
- His desk test of the commands passed the same night ("tested and good to go").

## 7. Desk test (open)
With DeetsMusic playing into a call, change a log line in `crates/host` and build. Expect:
`restart: … waiting for the song to end`; at the song change DeetsMusic pauses; `hand-off
written`; `[launcher] … starting the new one`; `voice: rejoining the lost call`; `voice: back
in the call`; DeetsMusic plays ~2 s after the stream starts. Listen to the first seconds of
the song.

## 8. The Python bot's version (archived)
`docs/archive/bot/live.py` watched its own `.py` files: a change to `dm.py` or
`dm_bridge.py` was reloaded in place (no gap); anything else restarted at a song boundary
the same way as §4, with `python -m bot` as the launcher. Its desk test was never run; the
Rust bot replaced it the same night.
