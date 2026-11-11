---
status: sop
desk_test: none
sources: [bot/voice.py, bot/dm.py, crates/tap/src/main.rs, crates/host/src/main.rs, crates/logic/src/watch.rs]
updated: 2026-11-10
---
# Debugging

Measure first. Each of these was a real failure on 2026-11-10.

## 1. The call hears nothing, but the bot's ring is green
Work from Discord's end back to the app:
1. **`/vc tone`.** Heard? Then Discord's side works; go to step 3. Not heard? Step 2.
2. **The DAVE readout.** `ready False` with silence that stays silent → check `pip show davey`.
   0.2.x with discord.py 2.7.x is the known cause (VOICE-PIPELINE.md §4).
   Also check the listener's own Discord: right-click Lil Deets → not muted, user volume > 0.
3. **The capture.** `deets-tap --meters`: is the app's session on a device, with a peak, and
   its mixer volume near 1.00? A low mixer volume makes a near-silent stream.
   **No session for the app at all** while it says it plays: check where it sends its
   sound. DeetsMusic on **AirPlay** plays to the HomePod, not to Windows, so there is nothing
   to capture (found 2026-11-10 with the Rust voice spike). Switch it to this PC.
4. **The right process.** Several processes may share a name (DeetsMusic's CLI and MCP run as
   `deetsmusic.exe` too). `--list` shows which apps have sound; the helper picks the top-most
   process of that name and its WebView2 child.

## 2. The stream is quiet or grainy
- Quiet: the app's Windows mixer volume (§1.3). Raise the app; lower the device or master
  volume instead (they come after the capture).
- Grainy, bass lost: the encoder settings (VOICE-PIPELINE.md §3). Check `voice.ENCODER` is
  passed to `play`.

## 3. Slash commands missing
- Ctrl+R in the Discord client (global commands are cached).
- Did another process sync over them? (BOT.md §2.) `GET /applications/<id>/commands` shows the
  global list; `…/guilds/<gid>/commands` a guild's.

## 4. /dm says "DeetsMusic isn't running"
- Is a port in 47825–47828 listening? `Get-NetTCPConnection -State Listen -LocalPort 47825`.
- Agent control on in DeetsMusic (Settings › Connections)?
- A 401 twice: the token in DeetsMusic's settings changed; restart the bot.

## 5. The bot's own log
The bot's terminal. Lines under `discord.lildeets.*` are this repo's. A capture error from the
helper is logged when the source closes: `deets-tap (app): …`.

## 6. The network dropped (the Rust bot, built 2026-11-10 night)
What the bot does by itself, and the log lines to look for (`lildeets_host`, `lildeets_logic`):
- **The call is lost** (songbird's own retries failed, or a new gateway session dropped the
  bot): `voice: lost the call; rejoining it`. If the call heard DeetsMusic, DeetsMusic is
  paused (`paused DeetsMusic while the call can't hear it`). The host leaves cleanly, waits
  2 s, and rejoins while a person is still in the channel (`voice: rejoining the lost call`);
  a failed try waits 4 s, 8 s, then 15 s. Back means in the call and hearing the app again.
  Back: `voice: back in the call`, and DeetsMusic plays again 2 s later. Nobody left:
  `not rejoining`. A `/vc play`, `/vc stop` or `/vc leave` cancels a rejoin.
- **The pause is one record for the whole bot** (one DeetsMusic serves every call). A drop
  pauses DeetsMusic only if no other call still hears it. DeetsMusic plays again only when a
  call hears it and no call that heard it is still lost (`resume: DeetsMusic playing
  again`). If nobody is listening any more, it stays paused; a `/vc play` of it plays it.
- **A restart during a rejoin** writes the lost call into `.lildeets-resume.json`; the new
  host rejoins it the same way. Every resumed call goes through the rejoin path.
- **Alone in a call** (everyone left during a rejoin or a resume): the host leaves within a
  second (`voice: left … nobody is in the channel`).
- **The gateway closed**: `gateway: the connection closed`. twilight reconnects with waits
  that grow to 255 s, so from 3 s on the host tries one TCP connect to
  `gateway.discord.gg:443` every 3 s; when it answers, a new shard resumes the session
  (`the network answers; reconnecting now`, at most every 15 s). Back: `gateway: back`. After
  a new session, "Listening to" is sent again and each call is checked.
- **Discord moved a call to another voice server**: `voice: Discord moved the call …; following it`.
- **The status line or the now-playing card** could not be written: tried again every 5 s
  until Discord takes it. The np card stops only when it is deleted (403 / 404).
- **Interactions during a drop** never reach the bot; Discord shows "The application did not
  respond". A reply that Discord refuses (an error status, not a network failure) is
  rescinded: the "thinking" message is deleted and the asker gets a private note
  (`rescinded the thinking message`).
- **A guest's `/dm` during a rejoin** answers "Lil Deets lost the call and is getting back
  in." (his wording, 2026-11-10; the host's snapshot carries `rejoining`).
- **Known gap:** a rejoin can start before the gateway has noticed the drop; it times out
  (15 s) and the backoff retries.
- **The cause may be this PC.** On 2026-11-10 the System log showed Tcpip 4266 / 4231 (all
  ephemeral ports in use) about once a day, and the Ethernet link at 100 Mbps on a 2.5 GbE
  port. Check those before blaming the network.

**Desk test (open):** with the bot playing DeetsMusic into a call, unplug the Ethernet cable
for 10 s, then 60 s, then 5 min. Each time: the music comes back by itself, no part of a song
is lost (DeetsMusic paused and played again), the status line and "Listening to" are right.

