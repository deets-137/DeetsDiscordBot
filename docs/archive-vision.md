# DeetsOTD — Vision

> Working doc. Captures only what's been decided/provided so far. The harness
> vision itself is intentionally left for the author to fill in.

## Origin

A Discord bot to read all the hyperlinks in a channel.

## Decided

- **Standalone.** Its own project (`~/Documents/GitHub/DeetsOTD`), not wired into
  DeetsCode.
- **Output:** Markdown + CSV.
- **Triggers:** Discord slash command, one-off CLI script, and a device-side
  harness — all on a single shared core.
- A new harness UI will be built from scratch — new ideas, new architecture —
  directed piece by piece.

## Exists today (substrate)

- `linkscan.py` — core: walk channel history, extract links, write outputs.
  Seams: `collect_links(channel, limit)` and `write_outputs(records, label)`.
- `bot.py` — `/links` slash command.
- `scan.py` — one-off CLI.
- `harness.py` — minimal local FastAPI trigger (placeholder; to be replaced/rethought).
- `paths.py`, `config.example.py`, `.env.example`, `requirements.txt`, `README.md`.

## Decided — the harness concept (2026-06-21)

- **The big concept — a Song-of-the-Day journal.** The channel is treated as a
  journal: every song link posted is an entry, kept in chronological order. The
  harness accumulates these into a running record of *what's been shared, by
  whom, and when* — answerable across the whole history, not just the last scan.
- **What we capture per post:** the user (Discord display name), the user's
  **Discord ID** (stable, so nickname changes never fragment history), the
  **message's** date + time, the URL, and the **service** it's from (apple_music,
  spotify, youtube, …) detected from the host.
- **SongOTDs — an enriched catalog.** Each posted song is resolved against the
  free iTunes APIs into a `songs` table (title, artist, album, genre, release
  date, artwork, preview, store link, full raw payload). Apple Music links use a
  direct id Lookup; Spotify/YouTube/etc. use oEmbed→iTunes-Search (best-effort,
  recorded as `match_type`). Many posts can point at one song.
- **Where state lives — local SQLite (`storage.db`), mirroring DeetsCode.**
  Singleton autocommit connection, `Row` factory, additive-only idempotent DDL.
  Gitignored. Files in `exports/` remain a *view*; the DB is the source of truth.
- **Architecture stance.** `linkscan.py` is a pure extractor; `enrich.py` only
  fetches/returns; `storage.py` owns the DB and is the only writer. The journal
  (`posts`) is idempotent on `(message_id, url)`, so re-scanning never
  double-records. Every entry point (CLI, slash command, harness) feeds the same
  store.

## Open — still for the author

- UI / aesthetic direction for the harness front-end (next to build):
- Anything else:
