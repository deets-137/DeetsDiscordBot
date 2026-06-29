# DeetsOTD — handoff notes

Quick orientation for picking this up cold. Full how-to is in `docs/USAGE.md`.

## What DeetsOTD is
A personal Song-of-the-Day journal: it scans one Discord channel, treats each
posted song link as a journal entry in local SQLite (`storage.db`, the **source
of truth**), enriches each to a real track, and mirrors everything to a Notion
gallery. Spreadsheets and Notion are read-only views of the DB.

## Current state (2026-06-28)
- **292 posts**, **290 resolved (99%)**. The 2 unresolved (#56, #84) are real
  Apple Music tracks the *free* iTunes API can't see — the Apple Music API will
  fix them.
- `match` mix: ~213 `direct`, ~68 `search` (validated), ~7 `spotify` (Spotify's
  own facts kept), 1 `manual`.
- Gif/meme noise (10 Tenor + 2 YouTube) has been **purged**; the scanner now
  filters non-music hosts going forward.
- Notion "SongOTD Journal" DB is fully synced (292 pages, 12 stale ones archived)
  with views: Covers (gallery), By Poster, Calendar, By Genre.

## Key identifiers
- Discord channel id: `1463626949430612267` (#song-of-the-day)
- Notion parent page "DeetsOTD": `38d86876-b378-8019-8264-f2d923c56f1c`
- Notion DB "SongOTD Journal": `cff663746866485ca23fc3d8d44bc478`
- All secrets live in `.env` (gitignored): `DISCORD_TOKEN`, `NOTION_TOKEN`,
  `NOTION_DB_ID`, `NOTION_PARENT_PAGE_ID`. Apple Music vars are placeholders.

## The one routine command
```bash
python scan.py 1463626949430612267 --enrich --export xlsx --notion
```
Scan → archive new posts → enrich → write `exports/SOTD_Export_MMDDYYYY.xlsx`
(All tab + one per month) → mirror to Notion. Idempotent; safe to re-run.

## Next session — START HERE
Apple Music API is **verified working** (2026-06-28). Key
`secrets/AuthKey_22CB27A4ZK.p8`, `.env` filled in; developer token mints and
catalog calls return 200. `applemusic.search_song` resolves both tracks the free
iTunes API couldn't (#84 → Logic "We Get High"; #56 → a live "Want You Gone").

Go straight to **wiring `applemusic` into `enrich.py`** per **`docs/APPLE_MUSIC.md`**:
make Apple Music the first resolver, then backfill and
`python notion_sync.py 1463626949430612267`.

**One learned nuance:** direct `song_by_id` (the `?i=` from an Apple Music URL)
can return **404 for delisted tracks** (that's why #56/#84 were unresolved). So
the resolver must **try id first, then fall back to `search_song`**. Sanity check:
```bash
python3 -c "import applemusic; print(applemusic.search_song('We Get High')['attributes']['artistName'])"  # -> Logic
```

## Then — Notion presentation polish
The user directs the aesthetic; a "Wrapped"-style stats dashboard (top artists,
genre split, posting streaks) was discussed.

## Gotchas
- `notion_sync.py` uses an **internal integration token**, separate from the
  Claude/OAuth Notion connector. The integration must be added to the DeetsOTD
  page (⋯ → Connections) or it 403s.
- The enricher trusts a Spotify page's own `og` metadata over iTunes search
  results; iTunes is accepted only when it validates. Don't "simplify" that back
  to a bare-title search — that's what caused the Kidz-Bop / wrong-artist matches.
- Notion select option names can't contain commas (handled in `notion_sync`).
</content>
