# DeetsOTD — handoff notes

> Internal working notes for continuing development between sessions — kept
> as-is for context. Ids below are placeholders. Full how-to is in
> `docs/USAGE.md`.

Quick orientation for picking this up cold.

## What DeetsOTD is
A personal Song-of-the-Day journal: it scans one Discord channel, treats each
posted song link as a journal entry in local SQLite (`storage.db`, the **source
of truth**), enriches each to a real track, and mirrors everything to a Notion
gallery. Spreadsheets and Notion are read-only views of the DB.

## Current state (2026-06-28)
- **292 posts, 292 resolved (100%). Zero flagged for review — fully clean.**
  Apple Music API is wired into `enrich.py` (see `docs/APPLE_MUSIC.md`).
- `match` mix: 208 `direct` (free iTunes id), 66 `search` (validated), 6 `apple`
  (Apple Music API), 3 `manual` (hand-verified), 1 `spotify` (a Bach/Yo-Yo Ma
  track Apple search wouldn't validate).
- **#56/#84 (delisted Apple ids) are resolved and hand-verified.** Both were
  enriched to `review` guesses, then confirmed via `review.py`: #56 accepted as
  "Want You Gone — Aperture Science Psychoacoustic Laboratories" (the correct
  Portal 2 credit); #84 corrected to **Ashe — "We Get High"** (the auto-guess
  "Logic" was wrong — exactly the case `review` exists to catch). Both now `manual`.
- Gif/meme noise (10 Tenor + 2 YouTube) has been **purged**; the scanner now
  filters non-music hosts going forward.
- Notion "SongOTD Journal" DB is **fully synced** (Apple backfill + the two
  corrections pushed), views: Covers (gallery), By Poster, Calendar, By Genre.

## Key identifiers
- Discord channel id: `YOUR_CHANNEL_ID` (#song-of-the-day)
- Notion parent page "DeetsOTD": `YOUR_NOTION_PARENT_PAGE_ID`
- Notion DB "SongOTD Journal": `YOUR_NOTION_DB_ID`
- All secrets live in `.env` (gitignored): `DISCORD_TOKEN`, `NOTION_TOKEN`,
  `NOTION_DB_ID`, `NOTION_PARENT_PAGE_ID`, and Apple Music
  (`APPLE_PRIVATE_KEY_PATH` → `secrets/AuthKey_XXXXXXXXXX.p8`, `APPLE_KEY_ID`,
  `APPLE_TEAM_ID`) — all live and verified working.

## The one routine command
```bash
python -m journal.scan YOUR_CHANNEL_ID --enrich --export xlsx --notion
```
Scan → archive new posts → enrich → write `exports/SOTD_Export_MMDDYYYY.xlsx`
(All tab + one per month) → mirror to Notion. Idempotent; safe to re-run.

## Next session — START HERE
Apple Music wiring, the `review.py` flow, the backfill, and the #56/#84
hand-verification + Notion re-sync are all **done (2026-06-28)**. The journal is
clean (100% resolved, nothing flagged). Next is presentation, not data — pick up
the largest open arc:

1. **Notion "Wrapped"-style stats dashboard** (discussed; user directs aesthetic):
   top artists, genre split, posting streaks, busiest months, per-poster
   leaderboards. All derivable from `storage.db`; mostly a `notion_sync` extension
   + new views.
2. **The harness web UI** — the original vision's end state: a UI to browse the
   journal/catalog (`harness.py` @ `127.0.0.1:8077` is the stub). Notion is the
   presentation layer today; UI/aesthetic direction is open.

Smaller opportunistic wins:
- Re-resolve the 66 `search` rows through the Apple API to upgrade them to `apple`
  (richer genre/preview/art). Low-risk — validation already guards it; just churns
  rows. Use the backfill recipe in `docs/APPLE_MUSIC.md`, widened to `search`.
- A Spotify Web API key would unlock the precise ISRC bridge (`song_by_isrc`,
  currently dormant) and exact-version matching (avoids "(Instrumental)/(Live)"
  ambiguity in title+artist search).

## Gotchas
- `notion_sync.py` uses an **internal integration token**, separate from the
  Claude/OAuth Notion connector. The integration must be added to the DeetsOTD
  page (⋯ → Connections) or it 403s.
- The enricher trusts a Spotify page's own `og` metadata over iTunes search
  results; iTunes is accepted only when it validates. Don't "simplify" that back
  to a bare-title search — that's what caused the Kidz-Bop / wrong-artist matches.
- Notion select option names can't contain commas (handled in `notion_sync`).
</content>
