---
status: shipped
desk_test: open
sources: [bot/dm.py]
updated: 2026-11-10
---
# The cards

Discord's Components V2 (`discord.ui.LayoutView`, discord.py 2.6+): a container with an
accent bar, sections of text with a thumbnail beside them, separators, and rows of buttons.
DISCORD-LIMITS.md has the limits.

## 1. The three cards
- **Now playing** (`np_view`): "Now playing in #channel" (or "Paused"), the title, artist ·
  album, a scrubber drawn once, then "3:06 · ends in 2 minutes · up next: …", the artwork at
  300 px, and ⏮ ⏯ ⏭ Queue. ⏯ shows ⏸ or ▶ by state.
- **Queue** (`queue_view`): the current song with its artwork; "Up Next · 27 songs · 1 h 38 m ·
  page 1 of 3"; then 10 rows, each `` ` 1` ` 2:40 | Explicit` **Title** · Artist `` over the
  album, with a 120 px thumbnail; then ◀ ▶. **38 components** with 10 rows: Discord's limit is
  40. Don't add a component per row.
- **Picker** (`picker_view`): the top 5 with thumbnails and buttons 1–5. Only the asker sees
  it. After a pick it turns into "Picked …".

## 2. The accent
The accent bar is DeetsMusic's current theme colour (BRIDGE.md §4). Moonlight → `#DFECFC`.

## 3. The column (the owner, 2026-11-10: "the same width for every row of one list")
- **Cards** use a code span, which is monospace: `` `{length:>W} | {rating:<8}` `` where W is
  the longest length on that page. Rows line up exactly.
- **Suggestions** are plain text in a proportional font, cut off on the right. The column
  leads the row: the length padded to the list's longest with **figure spaces** (U+2007, one
  digit wide), then 🅴, 🅲 or ▫️ (emoji, all the same width in Discord).
  > **Open:** confirm in Discord with a ≥10-minute song in the list. If the font lacks a
  > figure space, switch to zero-padded minutes (`02:57` beside `12:34`).
- **One-off lines** (the add line, the np card's current song) use the short form:
  `2:57 | Explicit`, or `3:45` with no rating.

## 4. Edit on change, never on a timer (the owner, 2026-11-10)
The np card is watched for 3 hours or until a newer `/dm np` in that channel. Every 5 s the
bot asks the bridge (local, free); it edits the message **only** when the song or play/pause
changes — about once a song. The countdown needs no edits: `<t:unix:R>` is drawn live by each
client. Edits go through the message (the bot's token), not the interaction, whose token dies
after 15 minutes.
