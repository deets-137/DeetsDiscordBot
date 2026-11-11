---
status: foundation
desk_test: none
sources: [crates/cards/src/cards.rs, crates/logic/src/commands.rs]
updated: 2026-11-10
---
# Discord limits the bot designs around

As of 2026-11-10, from Discord's documentation and what the bot hit.

| Limit | Value | Where it matters |
|---|---|---|
| Interaction answer | 3 s, or defer; the token lives 15 min | every command defers before a bridge call; card edits after 15 min go through the message |
| Autocomplete | 25 choices, 100 characters each, **text only**, answer in 3 s | `/dm add` suggestions; the column goes on the left (CARDS.md §3) |
| Components V2 | **40 components** per message, nested ones counted | the queue card is 38 with 10 rows |
| Buttons | 5 per action row, 5 rows | np card: 4; picker: 5 |
| Text display | 4000 characters per message total | queue rows clip titles at 80, artists at 60 |
| Plain message | 2000 characters | the add line |
| Message edits | about 5 per 5 s per channel | the np card edits once a song |
| Voice channel status | 500 characters; needs Set Voice Channel Status | STATUS.md |
| Presence activity name | 128 characters | STATUS.md |
| Global commands | one list per application; the last sync wins | BOT.md §2 |
| Voice | DAVE end-to-end encryption required | VOICE-PIPELINE.md §4 |
| Voice connections | one per guild per bot | one stream at a time per server |
| Timestamps | `<t:unix:R>` renders "in 2 minutes" and ticks in every client | countdowns with no edits |
