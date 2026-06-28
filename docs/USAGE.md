# Using DeetsOTD

DeetsOTD turns a Discord channel into a **Song-of-the-Day journal**: it reads
every song link posted, enriches each one against iTunes, and keeps the result
in a local SQLite database (`storage.db`). From there you can export a
spreadsheet or mirror everything into Notion.

```
Discord  ─scan.py→  storage.db  ─export→  SOTD_Export_*.{csv,tsv,xlsx}
                         │
                         └─notion_sync.py→  Notion "SongOTD Journal" gallery
```

`storage.db` is the **source of truth**. The spreadsheet and Notion are
read-only views generated from it — they never write back.

---

## 1. Setup

```bash
pip install -r requirements.txt
cp config.example.py config.py   # set DEFAULT_CHANNEL_ID / ALLOWED_CHANNEL_IDS
cp .env.example .env             # paste secrets (see below)
```

**Discord:** in the [developer portal](https://discord.com/developers/applications),
your app → **Bot** → enable **Message Content Intent**, and invite the bot with
**Read Message History**. Put the bot token in `.env` as `DISCORD_TOKEN`.

**Channel ID:** enable Developer Mode in Discord (Settings → Advanced), then
right-click the channel → **Copy ID**.

---

## 2. Scanning & the journal

Everything runs through `scan.py`. Flags stack; each implies `--archive`
(folding the scan into `storage.db`) where noted.

```bash
python scan.py <channel_id>                      # one-off: links → exports/*.md + *.csv
python scan.py <channel_id> --limit 500          # only the last 500 messages
python scan.py <channel_id> --by-user            # per-user breakdown (csv/tsv/xml)
python scan.py <channel_id> --archive            # fold into storage.db (the journal)
python scan.py <channel_id> --enrich             # + resolve songs against iTunes
python scan.py <channel_id> --export xlsx        # + write the spreadsheet
python scan.py <channel_id> --notion             # + sync to Notion
```

Idempotent: re-scanning the same channel only adds posts it hasn't seen
(`posts` is unique on `message_id` + `url`), and enrichment skips songs already
resolved. So this is safe to re-run any time.

**Non-music links are filtered out.** Discord's gif keyboard (`tenor.com`),
image hosts, and CDNs never enter the journal — see `linkscan.is_non_music`.
Meme/ad clips on YouTube can still slip through (they look like normal links);
purge those with `storage.delete_posts([...])` (see Maintenance below).

**The typical "do everything" command:**

```bash
python scan.py <channel_id> --enrich --export xlsx --notion
```

Other entry points wrap the same core:
- `python bot.py` — `/links` slash command inside Discord (DMs you the files).
- `python harness.py` — local web/API trigger at <http://127.0.0.1:8077>.

---

## 3. The spreadsheet export

`--export {csv,tsv,xlsx}` writes `exports/SOTD_Export_MMDDYYYY.<ext>` straight
from `storage.db`:

- **csv / tsv** — one flat sheet, oldest → newest.
- **xlsx** — an **All** tab plus **one tab per month** (`MM_YYYY`), styled with a
  frozen header, auto-filter, zebra striping, and compact clickable links.

22 columns: `date, time, day, month, author, track, artist, album, genre,
duration, released, rel_year, age_yrs, service, match, resolved, author_id,
source_url, apple_music_url, preview_url, artwork_url, jump_url`.

**`match` (how a song was resolved):**
- `direct` — Apple Music link, looked up by its track id (most reliable).
- `search` — title/artist searched on iTunes and **validated** against the
  source's own metadata before being accepted.
- `spotify` — the Spotify page's own facts are the source of truth and no iTunes
  result validated, so we keep Spotify's title/artist/album + cover art.
- `manual` — hand-corrected.
- blank / `resolved=no` — no match (e.g. a real Apple Music track the free iTunes
  API can't see; the Apple Music API will fix these — see `docs/APPLE_MUSIC.md`).

You can also generate one without a fresh scan:

```bash
python -c "import storage; print(storage.export_journal('<channel_id>', 'xlsx'))"
```

---

## 4. Notion mirror

`notion_sync.py` mirrors the journal into a Notion database — one page per post,
with the album art as each page's cover (so the gallery view is a wall of
covers). It's **idempotent on `post_id`**: re-runs update existing pages instead
of creating duplicates.

### One-time Notion setup

1. Create an **internal integration** at
   <https://www.notion.so/profile/integrations> and copy its secret.
2. Open the **DeetsOTD** page in Notion → **⋯ → Connections** → add that
   integration. *(This grants the script write access. It is separate from any
   Claude/OAuth connector.)*
3. Fill in `.env`:
   ```
   NOTION_TOKEN=secret_xxx
   NOTION_PARENT_PAGE_ID=38d86876-b378-8019-8264-f2d923c56f1c
   NOTION_DB_ID=cff663746866485ca23fc3d8d44bc478
   ```
   - `NOTION_DB_ID` points at the existing "SongOTD Journal" database. Leave it
     blank to have the first run create a fresh database under
     `NOTION_PARENT_PAGE_ID` and print the new id to paste back here.

### Run it

```bash
python notion_sync.py <channel_id>       # standalone full upsert (rate-limited, ~2 min for 300 rows)
# or as part of a scan:
python scan.py <channel_id> --notion
```

It's a **true mirror**: it creates new posts, updates changed ones, and
**archives** Notion pages whose post no longer exists in `storage.db` (e.g. after
purging gif/meme links). Idempotent on `post_id` — safe to re-run.

### Views

The database ships with: **🖼 Covers** (gallery), **👤 By Poster** (board),
**📅 Calendar**, and **🎧 By Genre** (board). Add your own from the **+** next to
the view tabs — group/filter/sort on any of the 22 properties.

---

## 5. Maintenance

Occasional cleanups, all in `storage.py`. Re-run `notion_sync.py` afterward to
push the changes (it auto-archives removed posts).

```python
import storage, enrich

# Drop gif/image-host links that slipped in (Tenor, etc.)
storage.purge_non_music("<channel_id>")

# Delete specific non-song posts (meme/ad clips on YouTube, etc.)
storage.delete_posts([81, 194])

# Re-resolve a single post (e.g. a known-bad match)
hit = enrich.enrich(url)
if hit:
    sid = storage.upsert_song(*hit); storage.link_post_to_song(post_id, sid)

# Remove catalog songs nothing points at anymore
storage.prune_orphan_songs()
```

To re-resolve a batch (e.g. all Spotify search matches after an enricher change),
loop `enrich.enrich` over the relevant posts with a ~1s delay, then
`prune_orphan_songs()`. The enricher is conservative — a Spotify result is only
overwritten when the page's own metadata confirms it.

## 6. Files at a glance

| File | Role |
|------|------|
| `scan.py` | CLI entry point: scan → archive → enrich → export → notion |
| `linkscan.py` | link extraction, service detection, non-music filter |
| `storage.py` | SQLite journal, `export_journal()` spreadsheet, maintenance helpers |
| `enrich.py` | song resolution (Apple id lookup / Spotify-truth / iTunes search) |
| `applemusic.py` | Apple Music API client — **scaffold, not wired yet** |
| `notion_sync.py` | idempotent Notion mirror (creates/updates/archives) |
| `bot.py` / `harness.py` | Discord slash-command / local web triggers |
| `storage.db` | **the database — source of truth** |
| `exports/` | generated spreadsheets and link dumps |
| `docs/APPLE_MUSIC.md` | plan for wiring the Apple Music API in next |

---

## 7. Troubleshooting

- **`DISCORD_TOKEN not set`** — `.env` is missing or not in the working dir.
- **`Set NOTION_DB_ID … or NOTION_PARENT_PAGE_ID`** — Notion vars absent from
  `.env`; the sync has no target.
- **Notion 401/403** — the integration token is wrong, or the DeetsOTD page
  hasn't been shared with the integration (step 2 above).
- **A song didn't resolve** — some links (gifs, odd hosts, or real Apple Music
  tracks the free iTunes API can't see) have no match; those rows show blank
  track fields and `resolved = no`. Re-running `--enrich` retries only the
  unresolved ones; the Apple Music API (`docs/APPLE_MUSIC.md`) will close most of
  the remaining gap.
