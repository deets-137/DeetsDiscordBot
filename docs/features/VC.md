---
status: shipped
desk_test: passed 2026-11-10
sources: [bot/voice.py]
updated: 2026-11-10
---
# /vc — play one of this PC's apps into a call

Host only (PERMISSIONS.md §1). Every reply is visible only to the host.

| Command | Does |
|---|---|
| `/vc play app:` | Joins the host's voice channel (or moves there) and plays that app. As you type, the suggestions are the apps making sound now (`deets-tap --list`). `deetsmusic` works without `.exe`. |
| `/vc tone` | Plays a 5 s 440 Hz tone with no capture: checks Discord's half of the path. Replies with the DAVE readout. |
| `/vc stop` | Stops playing; stays in the channel. |
| `/vc leave` | Stops and leaves. |

## 1. What `/vc play` checks first
1. deets-tap exists at `crates/tap/target/release/deets-tap.exe` (or `DEETS_TAP` in `.env`).
2. Its `--info` matches `EXPECTED_TAP` (ops/TAP.md §3). A mismatch replies with the rebuild
   command instead of playing noise.
3. PyNaCl and davey are installed.
4. The host is in a voice channel.
5. The helper is still running after 0.5 s (an app that isn't running fails here, with the
   helper's own message: "nope.exe is not running").

When the app is DeetsMusic, the reply adds "Everyone here can use `/dm`".

## 2. In the call
- The bot joins **self-deafened**: it never needs to hear the call.
- Audio as in VOICE-PIPELINE.md, at the `/dm volume` gain.
- **The bot leaves when it is the only one left** in its channel, which also ends the helper.
- While it plays DeetsMusic, the channel status and the bot's "Listening to" follow the song
  (STATUS.md).

## 3. Desk test (passed 2026-11-10)
1. `/vc tone` in a call: the tone is heard; the readout shows DAVE version 1.
2. `/vc play app:deetsmusic.exe` with DeetsMusic at 100% in the Windows mixer and routed to an
   unused output: the call hears it, the PC is quiet.
3. Everyone else leaves: the bot leaves.
