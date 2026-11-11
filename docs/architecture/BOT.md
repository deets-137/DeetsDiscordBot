---
status: shipped
desk_test: open
sources: [crates/launcher/src/main.rs, crates/host/src/main.rs, crates/logic/src/main.rs, crates/logic/src/commands.rs, crates/logic/src/relay.rs, crates/ipc/src/lib.rs, crates/rest/src/lib.rs, crates/relay-client/src/lib.rs, Cargo.toml]
updated: 2026-11-11
---
# The bot: three processes

Lil Deets is Rust since 2026-11-10 (his word: "I'll switch to the rust bot entirely now").
The Python bot it replaced is a record in `docs/archive/bot/` (§9).

## 1. Run it
`target\release\lildeets.exe`, from anywhere (the processes find the repo root from where
they were built). Build: `cargo build --workspace --release` with `CMAKE` set (ops/SETUP.md
§3). Ctrl+C stops it; the host leaves every call first.

| Process | Exe | Holds | Changes |
|---|---|---|---|
| launcher | `lildeets.exe` | nothing | almost never; a change needs a stop and a start |
| host | `lildeets-host.exe`, run as `lildeets-host.run.exe` | the gateway, every voice call, the captures, presence | rarely; goes live at a song boundary |
| logic | `lildeets-logic.exe`, run as `lildeets-logic.run-<n>.exe` | every interaction, the cards, the watchers | often; swaps in with no gap |

How a rebuild goes live: architecture/LIVE-RELOAD.md.

## 2. The launcher (`crates/launcher`)
Copies `lildeets-host.exe` to `lildeets-host.run.exe` and runs the copy (Windows locks a
running exe; the copy leaves the original free for cargo). Starts it again whenever it exits
with **75**; any other exit ends the launcher with the host's code. Sets `LILDEETS_LAUNCHED`
so the host knows someone will start it again.

## 3. The host (`crates/host`)
- **The gateway:** twilight-gateway 0.18, one shard, intents **Guilds** and **Guild Voice
  States**. It reads READY, RESUMED, GUILD_CREATE, VOICE_STATE_UPDATE, VOICE_SERVER_UPDATE and
  INTERACTION_CREATE; an interaction goes to the logic as JSON.
- **Voice:** songbird's `driver` with our glue (op 4 self-deafened → our VOICE_STATE_UPDATE
  and VOICE_SERVER_UPDATE → `Driver::connect`); DAVE by songbird (architecture/VOICE-PIPELINE.md).
  The capture is `deets_tap::TapStream`, opened on the blocking pool. Leaves when no person
  is left in its channel.
- **Presence** ("Listening to"), sent as the logic asks.
- **The logic's supervisor:** runs a copy of `lildeets-logic.exe`; when the original changes
  and stays the same for 2 s, starts the new copy, waits for its `Hello`, stops the old one.
  Starts a logic again 2 s after one disappears.
- **Network drops:** a lost call is rejoined, a closed gateway is reconnected fast, a voice
  server move is followed (ops/DEBUGGING.md §6).
- **The song-boundary restart** of itself (LIVE-RELOAD.md §4).
- Its threads are listed in its doc comment (rust-port-plan.md rule 10).

## 4. The logic (`crates/logic`)
- Every `/vc`, `/dm` and `/links` command, every button and autocomplete
  (features/VC.md, DM.md, LINKS.md). Interactions are read from JSON by hand, so a field
  Discord adds can never make one unreadable.
- Discord's REST API through `lildeets-rest` (hand-rolled, with Discord's rate limits);
  DeetsMusic through `deetsmusic-bridge` (architecture/BRIDGE.md); the cards from
  `lildeets-cards` (features/CARDS.md).
- **The watchers:** the status watcher (features/STATUS.md) and the np card watcher
  (CARDS.md §4).
- **A refused reply is rescinded:** a public "thinking" message whose reply Discord refused is
  deleted, and the asker gets a private note.
- **The command list:** `tests/parity/expected/commands.json`, synced when it differs from
  the last sync (a hash in `.lildeets-commands.sync`). The host deletes that record at each
  start, so each host start syncs once and a logic swap does not.

## 5. Host ⇄ logic (`crates/ipc`)
TCP on `127.0.0.1`, a port the OS picks (several Deets apps run on this PC). Each frame is a
4-byte little-endian length and JSON, at most 4 MiB. The logic opens with `Hello` (a token
from the OS random source, passed in `LILDEETS_IPC_TOKEN`, compared in constant time, and the
`PROTOCOL` version). Then:
- host → logic: `Ready`, `Dispatch` (an interaction), `VoiceSnapshot` (each call: channel,
  app, gain, DAVE, people, `rejoining`), `MemberVoice`, `Done` (a voice op's result);
- logic → host: `WhereIs`, `Play` (an app or the tone), `Stop`, `Leave`, `SetGain`,
  `Presence`, `Snapshot`.
- The Activity's relay (2026-11-11, `PROTOCOL` 2; built, not live until a release build):
  host → logic `RelayCheck` (is this person in this Activity instance?), logic → host
  `RelayChecked` (the instance's voice channel, or a line for the person).

### 5.1 The host's connection to the relay (`crates/relay-client`)
One WebSocket out to the Activity's relay Worker (features/ACTIVITY.md §5, §13.8), held only
while the bot is in a call. The host names its voice channels on every change (`tell_relay`,
once a second at most). It is off unless `.env` has both relay keys (§7). It reconnects by
itself and logs `relay: armed`, `connected`, `the connection dropped`, `disconnected`.

## 6. One command list per application
Discord keeps **one global command list per application**; a sync replaces it. So:
- **Only the logic syncs.** The journal's `scan` logs in with the same token to read the
  channel, then logs out; it never syncs. Keep it so.
- **The sync keeps the Activity's Launch command** (2026-11-11). Discord makes that command
  when Activities are turned on. The sync reads Discord's list first and carries it
  (`sync_commands`; features/ACTIVITY.md §13.8).
- **Never run two bots on this token.** The archived Python bot syncs its own list.
- The harness bot (`Vibe Coding Era/harness/discord_bot.py`) shared the token and was retired
  on 2026-11-10 (HANDOFF.md › Retired).
- Global commands can take a while to show in a Discord client: **Ctrl+R** reloads it.

## 7. Settings and the token
- `.env` at the repo root (or the environment): `DISCORD_TOKEN` (the application "Lil
  Deets"; the journal's `scan` uses it too), `VOICE_USER_IDS` (who besides the application's
  owner may start `/vc`). Reset the token in the developer portal → update `.env`.
  `LILDEETS_RELAY_URL` and `LILDEETS_RELAY_SECRET`: the Activity's relay (ACTIVITY.md §12.2);
  with either missing, the bot runs with no Activity.
- `config.py`: `ALLOWED_CHANNEL_IDS` limits `/links` (empty: any channel).
- Files the bot writes at the root, all gitignored: `.lildeets-resume.json`,
  `.lildeets-commands.sync`, `exports/` (`/links`).

## 8. Logging
`tracing` to the console. The default filter is `info` plus debug for `lildeets_host` and
`lildeets_logic`, `warn` for songbird; `RUST_LOG` overrides it. Anything that acts on its own
logs when it arms, fires and stops (CLAUDE.md › Working style). Started from the "Lil Deets"
shortcut, the same lines also go to `logs\lildeets-<date>.log` (ops/SETUP.md §10.1).

## 9. The Python bot (archived 2026-11-10)
`docs/archive/bot/` (discord.py; `requirements-voice.txt` beside it). It is the
specification the Rust port was built against, and the parity kit renders its expected
output from it (`scripts/parity/`, which imports it from the archive). It is not a fallback
any more: running it needs `PYTHONPATH=docs/archive` and `pip install -r
docs/archive/bot/requirements-voice.txt`, and it was not run after the move.

## 10. Servers (2026-11-10)
The bot is in four servers (friends' servers: names stay out of this public repo). It has
Connect, Speak and Set Voice Channel Status in all four.
