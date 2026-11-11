---
status: built
desk_test: open
sources: [crates/logic/src/watch.rs, crates/host/src/main.rs]
updated: 2026-11-10
---
# The channel status and "Listening to"

While the bot plays DeetsMusic into a call (`status_watcher` in `crates/logic/src/watch.rs`; the presence goes through the host):
- **The voice channel's status** — the line under the channel's name in the channel list,
  seen by the whole server — reads `🎵 Title — Artist`, or `⏸ Title — Artist` while paused.
- **The bot's profile status** reads "Listening to Title — Artist" (cleared while paused).

## 1. How
- Starts with the client (wraps `client.setup_hook`; never replaces `on_ready`).
- Every 5 s: which guilds is the bot tapping deetsmusic.exe in? What is playing (bridge)?
- Calls Discord **only when the line changes** (about once a song). Status ≤ 500 characters,
  activity name ≤ 128.
- Clears both when the stream stops, the bot leaves, or DeetsMusic quits.

## 2. Permission
Setting a channel's status needs **Set Voice Channel Status** in that channel. Checked
2026-11-10: the bot has it in all four servers. A channel that denies it: one warning in the
log, the channel line is skipped, the profile status still updates.

## 3. Desk test (open)
1. `/vc play app:deetsmusic.exe`: the channel list shows 🎵 and the song within 5 s; Lil Deets
   shows "Listening to …".
2. Pause in DeetsMusic: ⏸; the profile status clears. Resume: 🎵 again.
3. Next song: both change. `/vc stop`: both clear.
