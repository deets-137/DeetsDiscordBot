# DeetsOTD — handoff notes

Quick orientation for picking this up cold. Full how-to is in `docs/USAGE.md`.

## What DeetsOTD is
A personal Song-of-the-Day journal: it scans one Discord channel, treats each
posted song link as a journal entry in local SQLite (`storage.db`, the **source
of truth**), enriches each to a real track, and mirrors everything to a Notion
gallery. Spreadsheets and Notion are read-only views of the DB.

## Current state (2026-06-28)
- **292 posts, 292 resolved (100%).** Apple Music API is wired into `enrich.py`
  (see `docs/APPLE_MUSIC.md`).
- `match` mix: 208 `direct` (free iTunes id), 66 `search` (validated), 6 `apple`
  (Apple Music API), 2 `review` (delisted #56/#84 — best-effort guesses awaiting
  hand-verification), 1 `spotify` (a classical track Apple search wouldn't
  validate), 1 `manual`.
- **#56/#84 are no longer "unresolved" — they're `review`.** The free API can't
  see those delisted ids; the only fallback is a title search that's often wrong
  with no way to auto-catch it. So they carry a flagged guess for you to confirm
  via `python review.py` (see below). #56's guess looks correct already.
- Gif/meme noise (10 Tenor + 2 YouTube) has been **purged**; the scanner now
  filters non-music hosts going forward.
- Notion "SongOTD Journal" DB is fully synced (292 pages, 12 stale ones archived)
  with views: Covers (gallery), By Poster, Calendar, By Genre. **Notion is now
  stale vs storage.db** after the Apple backfill — re-run `notion_sync.py` to push
  the 6 upgraded + 2 review rows (see Next session).

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
Apple Music wiring + the `review.py` hand-verification flow are **done and tested**
(2026-06-28). Two follow-ups remain:

1. **Hand-verify the 2 `review` posts**, then push to Notion:
   ```bash
   python review.py 1463626949430612267   # confirm/correct #56 & #84
   python notion_sync.py 1463626949430612267
   ```
   `review.py` shows each guess + the original Discord post; **[u]** paste the
   correct Apple URL, **[a]** accept, **[s]** skip. #56 ("Want You Gone — Aperture
   Science Psychoacoustic Laboratories") is the correct Portal 2 credit and can be
   accepted; #84 ("We Get High — Logic") needs your eyes on whether that's the
   actually-posted track.
2. **Re-sync Notion regardless** — the Apple backfill upgraded 6 Spotify posts to
   `apple` (real artwork/genre/preview); Notion still shows the old `spotify` data.

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
