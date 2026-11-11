---
status: shipped
desk_test: open
sources: [crates/logic/src/links.rs, crates/logic/src/commands.rs, journal/linkscan.py]
updated: 2026-11-10
---
# /links — a channel's links

The bot's first command, from DeetsOTD.

`/links [limit]` reads the channel's history (all of it, or the **last** `limit` messages,
listed oldest first; the owner, 2026-11-10: until then it read the *first* `limit`), pulls
out every link with `journal.linkscan.collect_links`, and writes `exports/links_<channel>_<time>.md`
(deduplicated, grouped, with author, date and jump links) and `.csv` (every record). The bot
DMs both files to you; if your DMs are closed, it replies with them, visible only to you.

- `config.ALLOWED_CHANNEL_IDS`: empty = anywhere; otherwise only those channels.
- Needs the **Message Content** intent and **Read Message History** in the channel.
- The same core feeds the journal's scan (JOURNAL.md); `/links` never writes `storage.db`.
  `collect_links(..., newest=True)` is `/links` only. The journal's `--limit` still reads the
  channel's **first** N messages, so the nightly job is unchanged.

## Desk test (open, 2026-11-10)
In a channel with more than 5 messages that have links, run `/links limit:5`. The DM's
Markdown lists the links from the **last 5** messages, oldest of them first. Then run
`/links` with no limit: every link in the channel, oldest first, as before.
