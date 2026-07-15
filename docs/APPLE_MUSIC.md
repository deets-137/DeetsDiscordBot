# Apple Music API — how it works

`applemusic.py` is the authenticated resolver `enrich.py` prefers. The free,
key-less iTunes Search/Lookup API can't see every Apple Music track; the paid
Apple Music API resolves them directly and returns richer metadata (genre,
preview, hi-res art). Wired and verified 2026-06-28.

## One-time credentials
From <https://developer.apple.com> (paid membership) → Certificates, IDs &
Profiles → Keys → **+** → enable **MusicKit** → download `AuthKey_XXXX.p8`
(once only). Note the **Key ID** and your **Team ID**. Put them in `.env`:

```
APPLE_PRIVATE_KEY_PATH=secrets/AuthKey_XXXXXXXXXX.p8
APPLE_KEY_ID=XXXXXXXXXX
APPLE_TEAM_ID=YYYYYYYYYY
APPLE_STOREFRONT=us            # optional, defaults to us
```

`pip install 'PyJWT[crypto]'`. The `.p8` and `secrets/` are gitignored — never
commit them. If the vars are absent, `enrich.py` silently falls back to the free
iTunes path, so the pipeline still runs without keys.

## What `applemusic.py` provides
- `developer_token()` — ES256 JWT, cached (valid up to 6 months; we mint a short
  one per process).
- `song_by_id(catalog_id)` — the `?i=…` in an Apple album-track URL **is** the
  catalog id.
- `search_songs(term, limit)` / `search_song(term)` — authenticated search.
- `song_by_isrc(isrc)` — the precise Spotify→Apple bridge. **Dormant** (see below).
- `to_itunes_shape(song)` — adapts an Apple song to the same dict shape
  `storage.upsert_song()` consumes, so no schema changes were needed.

## How `enrich.py` uses it (match_type taxonomy)
- **Apple Music links** → `song_by_id` → `apple`. Falls back to the free iTunes
  lookup (`direct`) if the API has no record.
- **Delisted Apple ids** (404 on both — e.g. posts #56/#84) → best-effort slug
  search, tagged **`review`**. There is no reliable signal to confirm a title
  search is the right track (it happily returns live cuts / wrong artists), so we
  don't trust it — a human verifies by hand. See **the review flow** below.
- **Spotify links** → the page's `og` tags stay the source of truth; we search
  the Apple Music API for that title+artist and keep Apple's richer record when
  it **validates** against the Spotify truth (`apple`). Otherwise the existing
  validated-iTunes (`search`) → Spotify-truth (`spotify`) chain.
- **YouTube / everything else** → oEmbed title → iTunes search (`search`). The
  channel/uploader is normally dropped (labels, VEVO, reaction channels pollute
  the query), except a `<Artist> - Topic` auto-channel — that name *is* the
  artist, so it's folded into the search to stop a bare title mis-resolving to a
  more-famous namesake.

match_type values: `apple`, `direct`, `search`, `spotify`, `review`, `manual`.

## The review flow (`review.py`)
Songs the enricher couldn't safely resolve are tagged `review` and surface in
exports/Notion automatically (it's just a `match_type`). Verify them by hand:

```bash
python review.py                 # walk each flagged song interactively
python review.py list            # just print what's flagged
```

Per song you can: **[u]** paste the correct Apple Music URL (re-resolves via the
API → `manual`), **[a]** accept the current guess (→ `manual`), **[s]** skip, or
**[q]** quit.

**Confidently-wrong matches** — a `search`/`spotify`/`apple` row that *validated*
but is still wrong (a bare YouTube/Spotify title matching a more-famous namesake:
a cover, a live cut, the wrong artist's same-title song) — never get flagged, so
the interactive walk won't show them. Correct one by id instead:

```bash
python review.py search "<track or artist>"            # → a song_id + current match
python review.py fix <song_id> --url "<correct link>"  # re-resolve, re-point, tag manual
python review.py fix <song_id> --accept                # keep the current match, tag manual
```

`--url` resolves via the Apple Music API when a track id is present (falling back
to the free iTunes lookup with no key), or the full enricher for any other link.
Corrections re-point the post(s) and prune the orphaned guess. `storage.db` stays
the source of truth — re-run `python notion_sync.py <channel>` afterward to push
corrections to Notion.

## ISRC bridge — why it's dormant
The precise Spotify→Apple mapping is by **ISRC** (`song_by_isrc`), but it isn't
reachable today: Spotify no longer exposes ISRC in page HTML, and this project
holds no Spotify Web API credentials (by design — enrichment scrapes `og` tags to
avoid them). So the Spotify→Apple bridge keys on **title+artist** (validated)
instead. `song_by_isrc` is kept ready for the day a Spotify app/key is added —
prefer it over the search in `enrich.enrich`'s Spotify branch.

## Notes / gotchas
- Apple Music catalog ids are the same Apple track ids as iTunes, so storing them
  in `songs.itunes_track_id` stays consistent (no dupes vs. existing `direct` rows).
- `apple` vs `direct` both mean an authoritative id match; the split just records
  which API answered (paid vs free).
- Respect rate limits; the ~1 req/sec courtesy delay used elsewhere applies.

## Backfill recipe (re-resolving existing rows)
After wiring or credential changes, re-resolve unresolved + Spotify-fallback posts:
```python
import time, storage, enrich
c = storage._db()
rows = c.execute("""SELECT p.post_id, p.url FROM posts p
                    LEFT JOIN songs s ON s.song_id = p.song_id
                    WHERE p.song_id IS NULL OR s.match_type = 'spotify'""").fetchall()
for r in rows:
    hit = enrich.enrich(r["url"])
    if hit:
        track, mt = hit
        storage.link_post_to_song(r["post_id"], storage.upsert_song(track, mt))
    time.sleep(1.0)
storage.prune_orphan_songs()
# then: python review.py <channel>   and   python notion_sync.py <channel>
```
