---
status: shipped
desk_test: passed 2026-11-10
sources: [journal/letterboxd_web.py, journal/letterboxd_store.py]
updated: 2026-11-10
---
# The film journal

The Letterboxd diary, kept in `storage.db` (`films`, `film_watches`) and exported as
deets.solutions' `movies.json`. Moved from DeetsOTD unchanged except for paths (2026-11-10).

## 1. Three ways in or out
```
python -m journal.letterboxd_web import <export-folder> [--web [-o PATH]]   # the full archive
python -m journal.letterboxd_web rss [--user NAME] [--web [-o PATH]]        # the nightly top-up
python -m journal.letterboxd_web web [-o PATH]                              # just re-export
```
- **import** — a Letterboxd data export (Settings → Data → Export). Seeds the database, and
  corrects history older than the feed's window.
- **rss** — the public feed, the last ~50 diary entries: new sittings, rating and review
  edits, each film's TMDB id. `rss_guid` dedupes.
- **web** — renders `movies.json` from the database. The JSON is never the state.

## 2. Posters
`TMDB_API_KEY` in `.env` (free key). Lookups cache in `exports/tmdb_posters.json`, so a run
only asks TMDB for new films.

## 3. Desk test (passed 2026-11-10)
The export from a copy of `storage.db` matched DeetsOTD's (322 films, minus `generated_at`); a
live RSS run found 1 new entry, 1 new poster, and wrote 323.
