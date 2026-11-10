---
status: sop
desk_test: none
sources: [bot/voice.py, bot/dm.py, crates/tap/src/main.rs]
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
