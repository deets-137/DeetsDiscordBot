---
status: shipped
desk_test: passed 2026-11-10
sources: [crates/logic/src/commands.rs, crates/bridge/src/lib.rs]
updated: 2026-11-10
---
# /dm — DeetsMusic from Discord

The people in the call control the host's DeetsMusic, within his Rooms rules
(PERMISSIONS.md). Everything goes through the bridge (BRIDGE.md). The cards are in CARDS.md.

## 1. The commands

| Command | Does | Who sees the reply |
|---|---|---|
| `/dm help` | Every command, each marked ✅ (you can) or 🔒 (host only right now) | you |
| `/dm np` | The now-playing card, with ⏮ ⏯ ⏭ and Queue | channel |
| `/dm pause` · `resume` · `skip` · `back` | The transport; a line names who did it | channel |
| `/dm add song: [next:]` | Search as you type; queue it last, or next with `next: True` | channel (the picker: you) |
| `/dm queue [page]` | Up Next, 10 a page, ◀ ▶ | channel |
| `/dm remove n:` | Removes row n of Up Next | channel |
| `/dm move n: to:` | Moves row n to row `to` (1 = plays next) | channel |
| `/dm play song:` | Replaces what's playing (host) | channel |
| `/dm volume percent:` | The bot's own gain in the call, 0–200 (host) | you |

## 2. `/dm add`
Discord labels every input, so it shows as `/dm add song: …`. The box opens focused.
- **Suggestions** come from DeetsMusic's search as you type (2 characters or more, answered in
  under 3 s, cached 2 min per term). Each row leads with a column: length, then 🅴 (explicit),
  🅲 (clean) or ▫️ (no rating), then "Title — Artist · Album" (CARDS.md §3).
- **Picking a suggestion** queues that exact song.
- **Enter on your own text** queues the top result **if it is a sure match**: every word you
  typed is in its title, artist or album (a word may be the start of one), and the second
  result does not also match every word. "pink white frank" → Pink + White at once; "pink" →
  the picker.
- **Otherwise the picker**: the top 5 with artwork and buttons 1–5, visible only to you.
- **The line in the channel** says where the song landed and when it plays:
  `➕ Deets added **DNA. — Kendrick Lamar** `3:06 | Explicit` · #3 of 10 in Up Next · plays in 8 minutes.`
  "plays in" is a Discord timestamp that counts down in every client. While DeetsMusic is
  paused it says how much music is ahead instead.

## 3. Apple's rating
`contentRating` is `explicit`, `clean` (the edited version of an explicit song), or absent (no
edit exists). Absent is shown as nothing, not as "Clean": Apple doesn't call it clean.

## 4. Desk test (passed 2026-11-10)
Native cards look right in a real call; the np card edits only on a song change; an add shows
its row; the Explicit/Clean column appears in suggestions.
**Open:** the suggestion column's width in Discord's own font with a 10-minute song in the
list (CARDS.md §3).
