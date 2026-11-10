# Using DeetsOTD

DeetsOTD turns a Discord channel into a **Song-of-the-Day journal**: it reads
every song link posted, enriches each one against iTunes, and keeps the result
in a local SQLite database (`storage.db`). From there it exports a website-ready
`songs.json` for the deets.solutions SOTD page — the live consumer.

```
Discord  ─scan.py→  storage.db  ─scan.py --web→  songs.json  →  deets.solutions
                         │
                         ├─export→  SOTD_Export_*.{csv,tsv,xlsx}
                         └─notion_sync.py→  Notion gallery   (legacy — see §4)
```

`storage.db` is the **source of truth**. The site's `songs.json`, the
spreadsheet, and the (legacy) Notion mirror are read-only views generated from
it — they never write back.

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
python -m journal.scan <channel_id>                      # one-off: links → exports/*.md + *.csv
python -m journal.scan <channel_id> --limit 500          # only the last 500 messages
python -m journal.scan <channel_id> --by-user            # per-user breakdown (csv/tsv/xml)
python -m journal.scan <channel_id> --archive            # fold into storage.db (the journal)
python -m journal.scan <channel_id> --enrich             # + resolve songs against iTunes
python -m journal.scan <channel_id> --export xlsx        # + write the spreadsheet
python -m journal.scan <channel_id> --web \              # + write songs.json for the site
    --web-out ../DeetsSolutions/sotd/songs.json
python -m journal.scan <channel_id> --notion             # + mirror to Notion (legacy; site uses --web)
```

Idempotent: re-scanning the same channel only adds posts it hasn't seen
(`posts` is unique on `message_id` + `url`), and enrichment skips songs already
resolved. So this is safe to re-run any time.

**Non-music links are filtered out.** Discord's gif keyboard (`tenor.com`),
image hosts, and CDNs never enter the journal — see `linkscan.is_non_music`.
Meme/ad clips on YouTube can still slip through (they look like normal links);
purge those with `storage.delete_posts([...])` (see Maintenance below).

**The typical "do everything" command** (scan → enrich → publish the site JSON):

```bash
python -m journal.scan <channel_id> --enrich --web --web-out ../DeetsSolutions/sotd/songs.json
```

Other entry points wrap the same core:
- `python -m bot` — `/links` slash command inside Discord (DMs you the files).
- `python -m journal.harness` — local web/API trigger at <http://127.0.0.1:8077>.

**Correcting matches (`review.py`).** Two failure modes, two entry points.

*Songs the enricher flagged.* A delisted Apple Music track can't be resolved
automatically and a title search for it is unreliable, so the enricher tags those
`match_type='review'` rather than trusting a guess. `--enrich` tells you when any
turn up. Walk them by hand:

```bash
python -m journal.review                         # walk each flagged song interactively
python -m journal.review review -c <channel_id>  # ...limited to one channel
python -m journal.review list                    # just print what's flagged
```

Per song: **[u]** paste the correct Apple Music URL (re-resolves → `manual`),
**[a]** accept the guess (→ `manual`), **[s]** skip, **[q]** quit.

*Songs that resolved confidently but wrong.* A bare YouTube/Spotify title often
matches a more-famous namesake (a cover, a live cut, the wrong artist's same-title
song) and validates, so nothing flags it — `review` never shows it. Find it by
what's on the site, then fix it by id, non-interactively:

```bash
python -m journal.review search "<track or artist>"            # → a song_id + current match
python -m journal.review fix <song_id> --url "<correct link>"  # re-resolve, re-point, tag manual
python -m journal.review fix <song_id> --accept                # current match is fine — just tag manual
```

The correct link can be Apple Music (most precise — resolved by track id, and it
falls back to the free iTunes lookup when no Apple key is configured) or any
Spotify/YouTube/etc. URL the enricher can resolve; either way the result is tagged
`manual`. A fix re-points every post on that song and prunes the orphaned guess.

`storage.db` stays the source of truth — after correcting, regenerate `songs.json`
(`scan.py --web`, or `storage.export_web(...)` for a no-scan re-export) and commit
it to the site to publish. See `docs/APPLE_MUSIC.md`.

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

## 4. Notion mirror (legacy — optional)

> **The site does not use Notion.** deets.solutions reads `songs.json` (§2,
> `--web`); the Notion mirror is an independent, optional view and is **not**
> part of the publish or correction flow. The script is kept for anyone who
> still wants the gallery, but you can ignore this whole section otherwise.

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
   NOTION_PARENT_PAGE_ID=your_parent_page_id_here
   NOTION_DB_ID=your_db_id_here
   ```
   - `NOTION_DB_ID` points at the existing "SongOTD Journal" database. Leave it
     blank to have the first run create a fresh database under
     `NOTION_PARENT_PAGE_ID` and print the new id to paste back here.

### Run it

```bash
python -m journal.notion_sync <channel_id>       # standalone full upsert (rate-limited, ~2 min for 300 rows)
# or as part of a scan:
python -m journal.scan <channel_id> --notion
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

Occasional cleanups, all in `storage.py`. Regenerate `songs.json` afterward
(`scan.py --web`) and commit it to the site to publish the changes.

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
# ...or, for a confidently-wrong match, just: python -m journal.review fix <song_id> --url <link>

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
| `scan.py` | CLI entry point: scan → archive → enrich → export → `--web` songs.json |
| `linkscan.py` | link extraction, service detection, non-music filter |
| `storage.py` | SQLite journal, `export_web()` site JSON, `export_journal()` spreadsheet, maintenance helpers |
| `enrich.py` | song resolution (Apple Music API → iTunes id → Spotify-truth → search) |
| `applemusic.py` | authenticated Apple Music API client (preferred resolver) |
| `review.py` | correct mis-resolved songs: interactive `review`/`list` for flagged ones, `search`/`fix` for any confidently-wrong match |
| `notion_sync.py` | Notion mirror — **legacy/optional**, not in the publish flow |
| `bot.py` / `harness.py` | Discord slash-command / local web triggers |
| `storage.db` | **the database — source of truth** |
| `exports/` | generated spreadsheets and link dumps |
| `docs/APPLE_MUSIC.md` | Apple Music API setup, match_type taxonomy, review flow |

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
