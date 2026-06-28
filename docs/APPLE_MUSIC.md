# Apple Music API — integration plan (next session)

`applemusic.py` is scaffolded but **not yet called by `enrich.py`**. This is the
plan for wiring it in.

## Why
The free iTunes Search/Lookup API can't see every Apple Music track. Two real
journal posts prove it — both return `resultCount 0`:

| Post | Track | Apple Music link |
|---|---|---|
| #56 | Want You Gone | `music.apple.com/.../1657048600?i=1657050617` |
| #84 | We Get High   | `music.apple.com/.../1720473897?i=1720474315` |

The authenticated Apple Music API resolves these directly. It also enables
**Spotify → Apple mapping by ISRC**, which would let *every* Spotify post carry
real Apple artwork/preview/genre instead of the current Spotify-truth fallback.

## One-time credentials
From <https://developer.apple.com> (paid membership) → Certificates, IDs &
Profiles → Keys → **+** → enable **MusicKit** → download `AuthKey_XXXX.p8`
(once only). Note the **Key ID** and your **Team ID**. Put them in `.env`
(`APPLE_PRIVATE_KEY_PATH`, `APPLE_KEY_ID`, `APPLE_TEAM_ID`), and
`pip install 'PyJWT[crypto]'`.

## What's already built (`applemusic.py`)
- `developer_token()` — ES256 JWT, cached.
- `song_by_id(catalog_id)` — the `?i=…` in an Apple album-track URL **is** the
  catalog id, so this resolves #56/#84.
- `song_by_isrc(isrc)` — the Spotify→Apple bridge.
- `search_song(term)` — authenticated search.
- `to_itunes_shape(song)` — adapts an Apple Music song to the **same dict shape**
  `storage.upsert_song()` already consumes, so no schema changes are needed.

## Wiring it into `enrich.py` (the actual TODO)
Make Apple Music the **first, most-trusted** resolver, with the current iTunes /
Spotify-truth logic as fallback:

1. **Apple Music links** — extract the `?i=` id (reuse `_apple_track_id`) →
   `applemusic.song_by_id(id)` → `to_itunes_shape` → `match_type="apple"`. This is
   strictly better than today's `_lookup` and fixes #56/#84.
2. **Spotify links** — keep `_spotify_meta()` as the source of truth, but try to
   recover the **ISRC** (from the Spotify page's embedded JSON, or later the
   Spotify Web API) and call `applemusic.song_by_isrc(isrc)`. If it matches the
   Spotify title/artist (reuse `_matches`), use it (`match_type="apple"`);
   otherwise fall back to the existing iTunes-validate → synthetic chain.
3. Gate the whole thing on credentials being present (`applemusic` raises a clear
   `RuntimeError` if env is missing) so the free path still works without keys.

## Backfill after wiring
Re-resolve the API-limited and Spotify posts:
```python
# pseudocode
for post in posts where match_type in ('spotify', None) or service=='apple_music' and unresolved:
    re-run enrich(post.url); upsert_song; link_post_to_song
storage.prune_orphan_songs()
python notion_sync.py <channel_id>   # push corrections to Notion
```

## Notes / gotchas
- Apple Music catalog ids are the same Apple track ids as iTunes, so storing them
  in `songs.itunes_track_id` stays consistent (no dupes vs. existing `direct`
  rows).
- Developer tokens last up to 6 months; minting one per process is fine.
- Respect rate limits; keep the ~1 req/sec courtesy delay used elsewhere.
</content>
