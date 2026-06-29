"""
SongOTD enrichment. Prefers the authenticated Apple Music API when credentials
are present, and always keeps the free, key-less iTunes APIs as a fallback so the
pipeline still works without keys.

Resolution paths, by service:
  - Apple Music links carry a catalog id. We resolve it via the Apple Music API
    ("apple"), falling back to the free iTunes Lookup ("direct"). If the id is
    *delisted* (404 on both — e.g. journal posts #56/#84), we make a best-effort
    Apple search but tag it "review": a guess no algorithm can safely confirm, so
    a human verifies it by hand (see review.py).
  - Spotify links: the page's og tags are the source of truth. We search the
    Apple Music API for that title+artist and, when the hit validates against the
    Spotify truth, keep Apple's richer artwork/genre/preview ("apple"). Otherwise
    we fall back to a validated iTunes search ("search"), and finally to Spotify's
    own facts + cover ("spotify"). (A precise ISRC bridge — applemusic.song_by_isrc
    — is wired but dormant: Spotify no longer exposes ISRC in page HTML and we hold
    no Spotify API key, so title+artist is the reachable anchor today.)
  - YouTube / everything else: oEmbed title → iTunes Search ("search").

match_type values: "apple" (Apple Music API), "direct" (free iTunes id lookup),
"search" (validated iTunes search), "spotify" (Spotify-truth fallback), "review"
(unconfirmed Apple search guess awaiting hand-verification), "manual" (hand-fixed).

Pure functions: this module only fetches and returns dicts. storage.py decides
what to persist. Uses stdlib urllib so there's no extra dependency.
"""

from __future__ import annotations

import html
import json
import re
import urllib.parse
import urllib.request
from functools import lru_cache

import applemusic
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


def _get_text(url: str) -> str | None:
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=_TIMEOUT) as resp:
            return resp.read().decode("utf-8", "replace")
    except Exception:
        return None


def _meta(page: str, prop: str) -> str | None:
    m = re.search(r'<meta[^>]+(?:property|name)="' + re.escape(prop)
                  + r'"[^>]+content="([^"]*)"', page)
    return html.unescape(m.group(1).strip()) if m else None


def _spotify_meta(url: str) -> dict | None:
    """Authoritative track facts from a Spotify track page's og tags.

    Spotify's oEmbed gives only the bare title (no artist), which mis-resolves on
    iTunes. The page's og tags carry the truth: og:title is the track, and
    og:description reads "<Artist> · <Album> · Song · <year>". og:image is the
    cover. Returns {title, artist, album, year, thumb} — the source of truth we
    trust over whatever iTunes search happens to return.
    """
    html = _get_text(url)
    if not html:
        return None
    title = _meta(html, "og:title")
    if not title:
        return None
    parts = [p.strip() for p in (_meta(html, "og:description") or "").split(" · ")]
    artist = parts[0] if parts else ""
    album = parts[1] if len(parts) > 1 else ""
    year = next((p for p in parts if p.isdigit() and len(p) == 4), "")
    return {"title": title, "artist": artist, "album": album,
            "year": year, "thumb": _meta(html, "og:image") or ""}


def _norm(s: str) -> str:
    """Loosely normalize a title/artist for comparison (drop case, brackets, feat)."""
    s = (s or "").lower()
    s = re.sub(r"\(.*?\)|\[.*?\]", " ", s)
    s = re.sub(r"\b(feat|ft|featuring)\b.*", " ", s)
    s = re.sub(r"[^a-z0-9]+", " ", s)
    return re.sub(r"\s+", " ", s).strip()


def _matches(cand: dict, title: str, artist: str) -> bool:
    """True if an iTunes candidate is plausibly the same song as the Spotify truth."""
    ct, tt = _norm(cand.get("trackName")), _norm(title)
    if not (ct and tt and (ct == tt or ct in tt or tt in ct)):
        return False
    ca = _norm(cand.get("artistName"))
    main = _norm(re.split(r"[,&]| and | x | feat", artist, flags=re.I)[0])
    return bool(main and (main in ca or ca in main))


def _synthetic(meta: dict) -> dict:
    """An iTunes-shaped track built from Spotify's own facts, for when no iTunes
    hit validates. Uses Spotify's cover as artwork; year-only release; no preview."""
    return {
        "trackId": None,
        "trackName": meta["title"],
        "artistName": meta["artist"],
        "collectionName": meta["album"],
        "primaryGenreName": None,
        "releaseDate": meta["year"] or None,   # year only — honest, not a fake day
        "artworkUrl100": meta["thumb"],
        "previewUrl": None,
        "trackViewUrl": None,
    }


def _oembed_title(url: str, service: str) -> str | None:
    tmpl = _OEMBED.get(service)
    if not tmpl:
        return None
    data = _get_json(tmpl.format(url=urllib.parse.quote(url, safe="")))
    if not data:
        return None
    # oEmbed gives a "title"; for some services the artist is in "author_name".
    # On YouTube that field is the *channel* (e.g. "RatsRecords", "…VEVO"), which
    # is not the artist and pollutes the iTunes search — so skip it there.
    title = (data.get("title") or "").strip()
    author = (data.get("author_name") or "").strip()
    if (title and author and service not in ("youtube", "youtube_music")
            and author.lower() not in title.lower()):
        return f"{title} {author}"
    return title or None


# Cover-factory / soundalike markers. When the search term lacks an artist (e.g.
# Spotify's oEmbed returns only the track title), iTunes often ranks these above
# the original, so skip them and prefer the next hit. Kept conservative so real
# posts like "Snowblood (Instrumental)" still resolve.
_COVER_RE = re.compile(
    r"kidz bop|karaoke|\btribute\b|made famous by|originally performed by"
    r"|in the style of|8.?bit|rockabye baby|lullaby renditions?|as made famous",
    re.I)


def _is_cover(track: dict) -> bool:
    blob = " ".join(str(track.get(k, "")) for k in
                    ("artistName", "trackName", "collectionName"))
    return bool(_COVER_RE.search(blob))


def _search(term: str) -> dict | None:
    # Strip junk that hurts matching: "(Official Video)", " - Topic", feat tags.
    cleaned = re.sub(r"\b(official\s+(music\s+)?video|lyrics?|audio|hd|hq|m/?v)\b",
                     "", term, flags=re.I)
    cleaned = re.sub(r"\s*-\s*topic\b", "", cleaned, flags=re.I)
    cleaned = re.sub(r"[\(\[][^)\]]*[\)\]]", " ", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip() or term
    q = urllib.parse.quote(cleaned)
    data = _get_json(
        f"https://itunes.apple.com/search?term={q}&entity=song&limit=10")
    if not data:
        return None
    results = data.get("results") or []
    if not results:
        return None
    # Prefer the first non-cover hit; fall back to the top result if all look
    # like covers (better something than nothing).
    return next((r for r in results if not _is_cover(r)), results[0])


# ─── Apple Music API (authenticated; optional) ───────────────────────────────

@lru_cache(maxsize=1)
def _apple_ready() -> bool:
    """True if Apple Music credentials are configured and a token can be minted.
    Cached: the free path must work untouched when keys are absent."""
    try:
        return bool(applemusic.developer_token())
    except Exception:
        return False


def _apple_slug_term(url: str) -> str | None:
    """A search term recovered from an Apple Music URL's slug, e.g.
    .../album/want-you-gone/123?i=456 → "want you gone". Used only when the id is
    delisted, so we can still offer a (review-flagged) best guess."""
    m = re.search(r"/(?:album|song)/([^/?#]+)/\d", url)
    if not m:
        return None
    return urllib.parse.unquote(m.group(1)).replace("-", " ").strip() or None


def _apple_search_validated(title: str, artist: str) -> dict | None:
    """Search the Apple Music API for title+artist and return the first hit that
    validates against the given (Spotify-truth) facts, iTunes-shaped. None if the
    API is unavailable or nothing validates. Prefers an exact title over a
    variant like "(Instrumental)"/"(Live)"."""
    if not _apple_ready():
        return None
    songs = applemusic.search_songs(f"{title} {artist}".strip(), limit=10) or []
    cands = [applemusic.to_itunes_shape(s) for s in songs]
    valid = [c for c in cands if _matches(c, title, artist)]
    if not valid:
        return None
    want = _norm(title)
    return next((c for c in valid if _norm(c.get("trackName")) == want), valid[0])


# ─── Public entry point ──────────────────────────────────────────────────────

def enrich(url: str) -> tuple[dict, str] | None:
    """Resolve a posted url to an iTunes-shaped track payload.

    Returns (track_dict, match_type) — see the module docstring for the match_type
    taxonomy — or None if nothing could be resolved.
    """
    service = detect_service(url)

    if service == "apple_music":
        tid = _apple_track_id(url)
        if tid:
            # Authenticated API first (richer metadata), then the free iTunes id
            # lookup, which occasionally resolves ids the API path doesn't.
            if _apple_ready():
                song = applemusic.song_by_id(tid)
                if song:
                    return applemusic.to_itunes_shape(song), "apple"
            track = _lookup(tid)
            if track:
                return track, "direct"
        # Delisted id (404 on both): a title search is the only option, but it
        # mis-resolves to live cuts / wrong artists with no reliable signal to
        # catch it — so flag it 'review' for a human instead of trusting it.
        if _apple_ready():
            term = _apple_slug_term(url)
            if term:
                song = applemusic.search_song(term)
                if song:
                    return applemusic.to_itunes_shape(song), "review"
        # Fall through to the generic oEmbed path as a last resort.

    # Spotify: the page's own og tags are the source of truth. Prefer Apple
    # Music's richer record when an Apple search validates against that truth;
    # otherwise a validated iTunes search; otherwise keep Spotify's own facts.
    if service == "spotify":
        meta = _spotify_meta(url)
        if meta:
            apple = _apple_search_validated(meta["title"], meta["artist"])
            if apple:
                return apple, "apple"
            cand = _search(f"{meta['title']} {meta['artist']}".strip())
            if cand and _matches(cand, meta["title"], meta["artist"]):
                return cand, "search"
            return _synthetic(meta), "spotify"

    # Everything else: best-effort oEmbed title → iTunes search.
    term = _oembed_title(url, service)
    if term:
        track = _search(term)
        if track:
            return track, "search"
    return None
