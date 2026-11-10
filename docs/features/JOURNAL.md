---
status: shipped
desk_test: passed 2026-11-10
sources: [journal/scan.py, journal/linkscan.py, journal/storage.py, journal/enrich.py, journal/applemusic.py, journal/review.py, journal/notion_sync.py, journal/harness.py]
updated: 2026-11-10
---
# The Song-of-the-Day journal

A friends' `#song-of-the-day` channel, read as a journal: every song link posted is an entry.
It becomes deets.solutions' Song of the Day page through `songs.json`. Moved from DeetsOTD
unchanged except for paths and imports (2026-11-10).

## 1. The pipeline
```
python -m journal.scan <channel> --enrich --web --web-out <site>/sotd/songs.json
```
1. **Scan** (`linkscan.collect_links`): log in with the bot token, read the channel's history,
   pull out every link with its author (display name and user id), time and service
   (`apple_music`, `spotify`, `youtube`, …). Non-music hosts (Tenor, image hosts) are dropped.
   Also writes `exports/links_*.md` and `.csv`. The scan logs out when done and **never syncs
   slash commands** (architecture/BOT.md §2).
2. **Archive** (`storage`): fold the posts into `posts`, keyed by `(message_id, url)`, so a
   re-scan never doubles one.
3. **Enrich** (`enrich`): resolve each new post to a track in `songs`. Apple Music links: by
   id. Others: oEmbed title → iTunes search, accepted only when it validates. The Apple Music
   API (`applemusic`) fills what the free API can't. A doubtful guess is tagged `review`.
4. **Review** (`python -m journal.review`): a human fixes the `review` guesses
   (integrations/APPLE-MUSIC.md).
5. **Export** (`storage.export_web`): `{generated_at, channel, count, songs[]}` with display
   fields only. **Private Discord ids never leave**: message id, author id and jump link are
   dropped. Numbers are real numbers or null.

## 2. Other outputs
- `--export xlsx`: `exports/SOTD_Export_MMDDYYYY.xlsx`, an All tab and one per month.
- `--notion`: mirror to the "SongOTD Journal" Notion database (`notion_sync`), optional.
- `--by-user [--format csv|tsv|xml]`: every user's links together.
- `journal.harness`: a localhost-only FastAPI form to trigger a scan (no auth: never expose it).

The guide with every flag: guide/USAGE.md.

## 3. Who runs it
The 6 PM nightly job (ops/NIGHTLY.md). By hand when a correction is needed.

## 4. Desk test (passed 2026-11-10)
From this repo, against a copy of `storage.db`: the export matched DeetsOTD's byte for byte
(483 songs, minus `generated_at`); a live scan found 1 new post and wrote 484.
