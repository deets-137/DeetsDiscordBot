---
status: shipped
desk_test: none
sources: [bot/__main__.py, journal/linkscan.py]
updated: 2026-11-10
---
# /links — a channel's links

The bot's first command, from DeetsOTD.

`/links [limit]` reads the channel's history (all of it, or the last `limit` messages), pulls
out every link with `journal.linkscan.collect_links`, and writes `exports/links_<channel>_<time>.md`
(deduplicated, grouped, with author, date and jump links) and `.csv` (every record). The bot
DMs both files to you; if your DMs are closed, it replies with them, visible only to you.

- `config.ALLOWED_CHANNEL_IDS`: empty = anywhere; otherwise only those channels.
- Needs the **Message Content** intent and **Read Message History** in the channel.
- The same core feeds the journal's scan (JOURNAL.md); `/links` never writes `storage.db`.
