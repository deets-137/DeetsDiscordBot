---
status: foundation
desk_test: none
sources: [journal/storage.py, journal/letterboxd_store.py, journal/paths.py]
updated: 2026-11-10
---
# storage.db

One SQLite file at the repo root, gitignored. **It is the source of truth** for both
journals; the JSON on deets.solutions, the spreadsheets and Notion are views of it.

## 1. Rules (from DeetsOTD, unchanged)
- Singleton autocommit connection, `Row` factory, foreign keys on.
- **Additive-only, idempotent DDL**, run on every start.
- **One writer per side:** `journal/storage.py` writes the song tables;
  `journal/letterboxd_store.py` writes the film tables. `linkscan.py` only extracts;
  `enrich.py` only fetches and returns.

## 2. The song side (`storage.py`)
| Table | One row per | Key |
|---|---|---|
| `scans` | scan run (append-only log) | |
| `posts` | song link posted in the channel: display name, **Discord user id**, message time, URL, service | `(message_id, url)` — a re-scan never double-records |
| `songs` | resolved track (title, artist, album, genre, release, artwork, preview, store link, `match_type`, `raw_json`) | many posts → one song |

`match_type`: `direct` (Apple id lookup), `search` (validated title search), `apple` (Apple
Music API), `manual` (hand-verified with `review`), `review` (a guess waiting for a human).
integrations/APPLE-MUSIC.md has the details.

## 3. The film side (`letterboxd_store.py`)
| Table | One row per |
|---|---|
| `films` | film the account knows: name, year, Letterboxd URI, TMDB id and poster, rating, status (watched / watchlist) |
| `film_watches` | diary sitting: date, rating, rewatch, tags, review; `rss_guid` dedupes the feed |
| `film_syncs` | import / RSS run (log) |

## 4. Size and counts (2026-11-10)
1.8 MB. 483 posts, 477 songs, 322 films, 213 watches (DeetsOTD's copy, before the cut-over).

## 5. Moving it
Copy with SQLite's backup API, never a plain file copy while a writer may be open:
`sqlite3.connect(src).backup(sqlite3.connect(dst))`. Compare table counts after.
