"""
Apple Music API client — the authenticated resolver enrich.py prefers.

Why this exists: the free, key-less iTunes Search/Lookup API can't see some real
Apple Music tracks — e.g. journal posts #56 "Want You Gone" and #84 "We Get High"
return resultCount 0. The authenticated Apple Music API (part of a paid developer
subscription) resolves them directly and gives richer metadata. enrich.py calls
song_by_id for Apple links and search_songs for the Spotify→Apple bridge; it
gracefully no-ops to the free path when credentials are absent. song_by_isrc is
the precise Spotify→Apple bridge, kept for when a Spotify API key is available
(Spotify no longer exposes ISRC in page HTML, so it's dormant today).

Auth: Apple Music uses a *developer token* — a short-lived ES256 JWT signed with
a MusicKit private key. You need three things from developer.apple.com:
  - a MusicKit private key  → the downloaded AuthKey_XXXX.p8 file
  - that key's Key ID       → 10 chars
  - your Team ID            → 10 chars
Put them in .env:
  APPLE_PRIVATE_KEY_PATH=/path/to/AuthKey_XXXXXXXXXX.p8
  APPLE_KEY_ID=XXXXXXXXXX
  APPLE_TEAM_ID=YYYYYYYYYY
  APPLE_STOREFRONT=us            # optional, defaults to us

Needs PyJWT[crypto] (see requirements.txt). See docs/integrations/APPLE-MUSIC.md for setup.
"""

from __future__ import annotations

import os
import time
import urllib.parse
import urllib.request
from functools import lru_cache

from dotenv import load_dotenv

from .paths import PROJECT_ROOT

load_dotenv(PROJECT_ROOT / ".env")

API = "https://api.music.apple.com/v1"


def _storefront() -> str:
    return os.getenv("APPLE_STOREFRONT", "us")


@lru_cache(maxsize=1)
def developer_token() -> str:
    """Build (and cache) the ES256 developer JWT from the MusicKit private key.

    Tokens are valid up to 6 months; we mint a short one per process. Requires
    PyJWT with the cryptography backend.
    """
    try:
        import jwt  # PyJWT
    except ModuleNotFoundError as e:  # pragma: no cover - setup guard
        raise RuntimeError("PyJWT not installed — `pip install 'PyJWT[crypto]'`") from e

    key_path = os.getenv("APPLE_PRIVATE_KEY_PATH")
    key_id = os.getenv("APPLE_KEY_ID")
    team_id = os.getenv("APPLE_TEAM_ID")
    if not (key_path and key_id and team_id):
        raise RuntimeError(
            "Apple Music env missing — set APPLE_PRIVATE_KEY_PATH, APPLE_KEY_ID, "
            "APPLE_TEAM_ID in .env (see applemusic.py docstring).")

    with open(key_path, "r", encoding="utf-8") as f:
        private_key = f.read()

    now = int(time.time())
    return jwt.encode(
        {"iss": team_id, "iat": now, "exp": now + 60 * 60 * 12},
        private_key,
        algorithm="ES256",
        headers={"alg": "ES256", "kid": key_id},
    )


def _get(path: str, params: dict | None = None) -> dict | None:
    """GET a catalog endpoint with the developer token. Returns parsed JSON."""
    import json
    url = f"{API}/{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params, doseq=True)
    req = urllib.request.Request(
        url, headers={"Authorization": f"Bearer {developer_token()}"})
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode("utf-8", "replace"))
    except Exception:
        return None


# ─── Lookups ─────────────────────────────────────────────────────────────────

def song_by_id(catalog_id: str) -> dict | None:
    """Resolve a catalog song id (the ?i=… in an Apple Music album-track URL)."""
    data = _get(f"catalog/{_storefront()}/songs/{catalog_id}")
    items = (data or {}).get("data") or []
    return items[0] if items else None


def song_by_isrc(isrc: str) -> dict | None:
    """Resolve a song by ISRC — the bridge for mapping Spotify tracks to Apple."""
    data = _get(f"catalog/{_storefront()}/songs", {"filter[isrc]": isrc})
    items = (data or {}).get("data") or []
    return items[0] if items else None


def search_songs(term: str, limit: int = 10) -> list[dict]:
    """Search the catalog for songs, returning up to `limit` raw song objects."""
    limit = max(1, min(limit, 25))  # Apple caps the songs search at 25
    data = _get(f"catalog/{_storefront()}/search",
                {"term": term, "types": "songs", "limit": limit})
    return (((data or {}).get("results") or {}).get("songs") or {}).get("data") or []


def search_song(term: str) -> dict | None:
    """The single best search hit (or None) — convenience over search_songs."""
    songs = search_songs(term, limit=1)
    return songs[0] if songs else None


# ─── Adapter ─────────────────────────────────────────────────────────────────

def to_itunes_shape(song: dict) -> dict:
    """Map an Apple Music API song object to the iTunes-style dict that
    storage.upsert_song / enrich already consume, so this can slot in with no
    schema changes. (Apple Music ids are the same Apple track ids as iTunes.)"""
    a = song.get("attributes", {})
    artwork = (a.get("artwork") or {}).get("url", "")
    if artwork:
        artwork = artwork.replace("{w}", "600").replace("{h}", "600")
    previews = a.get("previews") or []
    return {
        "trackId": int(song["id"]) if str(song.get("id", "")).isdigit() else None,
        "trackName": a.get("name"),
        "artistName": a.get("artistName"),
        "collectionName": a.get("albumName"),
        "primaryGenreName": (a.get("genreNames") or [None])[0],
        "releaseDate": a.get("releaseDate"),
        "artworkUrl100": artwork,
        "previewUrl": previews[0].get("url") if previews else None,
        "trackViewUrl": a.get("url"),
        # storage.py reads the runtime from trackTimeMillis, the free iTunes
        # API's name for it; Apple Music calls the same milliseconds
        # durationInMillis. Without this line every Apple-matched song
        # publishes a blank duration.
        "trackTimeMillis": a.get("durationInMillis"),
    }
