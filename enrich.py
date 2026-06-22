"""
SongOTD enrichment via the (free, key-less) iTunes APIs.

Two paths, by service:
  - Apple Music links carry a track id, so we Lookup it directly — a reliable
    "direct" match.
  - Spotify / YouTube / everything else carry no iTunes id. We read the page's
    title via the service's oEmbed endpoint to get a "Title - Artist" string,
    then Search iTunes for the closest song — a fuzzier "search" match.

Pure functions: this module only fetches and returns dicts. storage.py decides
what to persist. Uses stdlib urllib so there's no extra dependency.
"""

from __future__ import annotations

import json
import re
import urllib.parse
import urllib.request

from linkscan import detect_service

_UA = "DeetsOTD/0.1 (+https://github.com/deets-137)"
_TIMEOUT = 10


def _get_json(url: str) -> dict | None:
    try:
        req = urllib.request.Request(url, headers={"User-Agent": _UA})
        with urllib.request.urlopen(req, timeout=_TIMEOUT) as resp:
            return json.loads(resp.read().decode("utf-8", "replace"))
    except Exception:
        return None


# ─── Apple Music: direct id lookup ───────────────────────────────────────────

def _apple_track_id(url: str) -> str | None:
    """Pull the song id from an Apple Music / iTunes url.

    Album-track links put the song id in ?i=..., a bare song link in /song/<id>,
    and classic iTunes links end in /idNNN. Returns the numeric id as a string.
    """
    m = re.search(r"[?&]i=(\d+)", url)
    if m:
        return m.group(1)
    m = re.search(r"/song/(?:[^/]+/)?(\d+)", url)
    if m:
        return m.group(1)
    m = re.search(r"/id(\d+)", url)
    if m:
        return m.group(1)
    return None


def _lookup(track_id: str) -> dict | None:
    data = _get_json(f"https://itunes.apple.com/lookup?id={track_id}&entity=song")
    if not data:
        return None
    for r in data.get("results", []):
        if r.get("wrapperType") == "track" or r.get("kind") == "song":
            return r
    return None


# ─── Other services: oEmbed title → iTunes search ────────────────────────────

_OEMBED = {
    "spotify": "https://open.spotify.com/oembed?url={url}",
    "youtube": "https://www.youtube.com/oembed?url={url}&format=json",
    "youtube_music": "https://www.youtube.com/oembed?url={url}&format=json",
    "soundcloud": "https://soundcloud.com/oembed?format=json&url={url}",
    "deezer": "https://api.deezer.com/oembed?url={url}&format=json",
}


def _oembed_title(url: str, service: str) -> str | None:
    tmpl = _OEMBED.get(service)
    if not tmpl:
        return None
    data = _get_json(tmpl.format(url=urllib.parse.quote(url, safe="")))
    if not data:
        return None
    # oEmbed gives a "title"; for most music services it reads like the track,
    # often with the artist in "author_name". Combine for a better search term.
    title = (data.get("title") or "").strip()
    author = (data.get("author_name") or "").strip()
    if title and author and author.lower() not in title.lower():
        return f"{title} {author}"
    return title or None


def _search(term: str) -> dict | None:
    # Strip junk that hurts matching: "(Official Video)", " - Topic", feat tags.
    cleaned = re.sub(r"\b(official\s+(music\s+)?video|lyrics?|audio|hd|hq|m/?v)\b",
                     "", term, flags=re.I)
    cleaned = re.sub(r"\s*-\s*topic\b", "", cleaned, flags=re.I)
    cleaned = re.sub(r"[\(\[][^)\]]*[\)\]]", " ", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip() or term
    q = urllib.parse.quote(cleaned)
    data = _get_json(
        f"https://itunes.apple.com/search?term={q}&entity=song&limit=1")
    if not data:
        return None
    results = data.get("results") or []
    return results[0] if results else None


# ─── Public entry point ──────────────────────────────────────────────────────

def enrich(url: str) -> tuple[dict, str] | None:
    """Resolve a posted url to an iTunes track payload.

    Returns (track_dict, match_type) where match_type is "direct" (Apple id) or
    "search" (best-effort title lookup), or None if nothing could be resolved.
    """
    service = detect_service(url)

    if service == "apple_music":
        tid = _apple_track_id(url)
        if tid:
            track = _lookup(tid)
            if track:
                return track, "direct"
        # Fall through to search if the id path failed.

    term = _oembed_title(url, service)
    if term:
        track = _search(term)
        if track:
            return track, "search"
    return None
