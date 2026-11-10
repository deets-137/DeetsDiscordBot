#!/usr/bin/env python3
"""Letterboxd film journal: import exports, ingest the RSS feed, export JSON.

The film journal lives in storage.db (see letterboxd_store.py), fed from two
sources and rendered to the site from the DB — the JSON is never the state:

  import  — a Letterboxd data export (Settings -> Data -> Export), the full
            archive. Seeds the DB and remains the true-up path when history
            needs correcting (edits/deletions older than the feed's window).
  rss     — the public feed (letterboxd.com/<user>/rss/), the last ~50 diary
            entries. The nightly incremental top-up: new sittings, rating and
            review edits, plus each film's TMDB id. Watch-only entries (no
            review) count as sittings too; their boilerplate "Watched on ..."
            description is not a review and is never stored as one.
  web     — write movies.json for the site's movies page from the DB.

Usage:
    python -m journal.letterboxd_web import <export-dir> [--web [-o PATH]]
    python -m journal.letterboxd_web rss [--user d33ts] [--web [-o PATH]]
    python -m journal.letterboxd_web web [-o PATH]

Point -o at the site repo to publish: ../DeetsSolutions/movies/movies.json

Posters: Letterboxd carries no artwork the site may hotlink, so posters come
from TMDB (TMDB_API_KEY in .env; free key: themoviedb.org -> Settings ->
API). A film seen in the RSS feed brings its exact tmdb_id — no search
needed. Export-only films fall back to a title+year search validated on
release year (TMDB ranks by popularity, so an unchecked first hit grabs a
louder same-title namesake). exports/tmdb_posters.json is consulted first
and doubles as the hand-pin override: set a film's "<Name> (<Year>)" key to
the right /poster_path.jpg and every regen honors it (the cache outranks
the URL mirrored onto the film row, which exists for DB self-containment).
The site must credit TMDB (the movies page footer does).
"""

from __future__ import annotations

import argparse
import csv
import html
import json
import os
import re
import sys
import time
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
from pathlib import Path

import requests
from dotenv import load_dotenv

from . import letterboxd_store as store
from .paths import EXPORTS_DIR, PROJECT_ROOT

load_dotenv(PROJECT_ROOT / ".env")

TMDB_SEARCH = "https://api.themoviedb.org/3/search/movie"
TMDB_MOVIE = "https://api.themoviedb.org/3/movie/{id}"
TMDB_IMG = "https://image.tmdb.org/t/p/w342"      # ~card-sized poster
POSTER_CACHE = EXPORTS_DIR / "tmdb_posters.json"

RSS_URL = "https://letterboxd.com/{user}/rss/"
RSS_UA = "DeetsOTD film journal (deets.solutions)"
LB_NS = "https://letterboxd.com"
TMDB_NS = "https://themoviedb.org"


# ─── Export CSVs → store ─────────────────────────────────────────────────────

def _load(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with open(path, encoding="utf-8-sig", newline="") as f:
        return list(csv.DictReader(f))


def _key(row: dict) -> tuple[str, str]:
    return (row["Name"], row["Year"])


def _year(s) -> int | None:
    return int(s) if isinstance(s, str) and s.isdigit() else None


def parse_export(export_dir: Path) -> tuple[list[dict], list[dict]]:
    """Join the export's CSVs on (Name, Year) — verified unique per film —
    into the film/watch dicts letterboxd_store.import_export takes."""
    watched = _load(export_dir / "watched.csv")
    diary = _load(export_dir / "diary.csv")
    ratings = {_key(r): float(r["Rating"]) for r in _load(export_dir / "ratings.csv")}
    reviews = _load(export_dir / "reviews.csv")
    liked = {_key(r) for r in _load(export_dir / "likes" / "films.csv")}
    watchlist = _load(export_dir / "watchlist.csv")

    films: dict[tuple, dict] = {}
    for r in watched:
        films[_key(r)] = {
            "name": r["Name"], "year": _year(r["Year"]),
            "uri": r["Letterboxd URI"], "logged_date": r["Date"],
            "rating": ratings.get(_key(r)), "liked": _key(r) in liked,
            "status": "watched",
        }
    for r in watchlist:
        if _key(r) not in films:   # already watched — the queue entry is stale
            films[_key(r)] = {
                "name": r["Name"], "year": _year(r["Year"]),
                "uri": r["Letterboxd URI"], "logged_date": r["Date"],
                "rating": None, "liked": False, "status": "watchlist",
            }

    # Reviews are diary entries with text — (film, watched date) pairs them
    # back up, so a rewatch keeps its own review on its own watch.
    review_by_watch = {(_key(r), r.get("Watched Date") or r["Date"]): r
                       for r in reviews}

    watches = []
    for r in diary:
        wd = r.get("Watched Date") or r["Date"]
        rev = review_by_watch.get((_key(r), wd))
        watches.append({
            "name": r["Name"], "year": _year(r["Year"]),
            "watched_date": wd, "logged_date": r["Date"],
            "entry_uri": r["Letterboxd URI"],
            "rating": float(r["Rating"]) if r.get("Rating") else None,
            "rewatch": r.get("Rewatch") == "Yes",
            "tags": r.get("Tags") or None,
            # \n throughout: the CSVs carry \r\n but the RSS round-trip yields
            # \n, and both sources must store a review identically (verified
            # byte-equal across the whole overlap window once normalized).
            "review": rev["Review"].replace("\r\n", "\n") if rev else None,
            "review_date": rev["Date"] if rev else None,
            "raw": r,
        })
    return list(films.values()), watches


# ─── RSS feed → store ────────────────────────────────────────────────────────

def _review_text(desc_html: str | None) -> str | None:
    """The review as plain text, or None.

    The feed's description is rendered HTML: a leading poster <p><img></p>,
    an optional '<em>This review may contain spoilers.</em>' banner, then the
    review as <p> paragraphs. Strip the furniture, keep the words."""
    if not desc_html:
        return None
    s = re.sub(r"<p>\s*<img[^>]*>\s*</p>", "", desc_html, flags=re.I)
    s = re.sub(r"<p>\s*<em>\s*This review may contain spoilers\.?\s*</em>\s*</p>",
               "", s, flags=re.I, count=1)
    s = re.sub(r"<br\s*/?>", "\n", s, flags=re.I)
    paras = re.findall(r"<p>(.*?)</p>", s, flags=re.S | re.I)
    s = "\n\n".join(paras) if paras else s
    s = re.sub(r"<[^>]+>", "", s)
    return html.unescape(s).strip() or None


def _text(item, tag: str, ns: str = LB_NS) -> str | None:
    el = item.find(f"{{{ns}}}{tag}")
    return el.text if el is not None and el.text else None


def parse_rss(xml_text: str) -> list[dict]:
    """Feed items → upsert_rss_watch dicts. Only diary entries pass: guids
    are 'letterboxd-review-N' (has a review) / 'letterboxd-watch-N' (none) —
    anything else (published lists, ...) is not a sitting."""
    root = ET.fromstring(xml_text)
    out = []
    for item in root.iter("item"):
        guid = (item.findtext("guid") or "").strip()
        is_review = guid.startswith("letterboxd-review-")
        if not (is_review or guid.startswith("letterboxd-watch-")):
            continue
        watched = _text(item, "watchedDate")
        if not watched:        # a diary entry always has one; skip defensively
            continue
        rating = _text(item, "memberRating")
        tmdb_id = _text(item, "movieId", TMDB_NS)
        pub = item.findtext("pubDate")
        pub_date = logged_ts = None
        try:
            if pub:
                dt = parsedate_to_datetime(pub)
                pub_date = dt.date().isoformat()
                # Full timestamp, normalized to UTC so strings sort correctly
                # across the feed's shifting local offset.
                logged_ts = dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        except (TypeError, ValueError):
            pass
        desc = item.findtext("description")
        out.append({
            "guid": guid,
            "name": _text(item, "filmTitle") or (item.findtext("title") or "").strip(),
            "year": _year(_text(item, "filmYear")),
            "watched_date": watched,
            "rating": float(rating) if rating else None,
            "liked": {"Yes": 1, "No": 0}.get(_text(item, "memberLike")),
            "rewatch": _text(item, "rewatch") == "Yes",
            "review": _review_text(desc) if is_review else None,
            "uri": (item.findtext("link") or "").strip() or None,
            "tmdb_id": int(tmdb_id) if tmdb_id and tmdb_id.isdigit() else None,
            "pub_date": pub_date,
            "logged_ts": logged_ts,
            "raw": {"title": item.findtext("title"), "pubDate": pub,
                    "description": desc},
        })
    return out


def fetch_rss(user: str) -> str:
    r = requests.get(RSS_URL.format(user=user),
                     headers={"User-Agent": RSS_UA}, timeout=30)
    r.raise_for_status()
    return r.text


# ─── TMDB posters ────────────────────────────────────────────────────────────

def _load_poster_cache() -> dict:
    if POSTER_CACHE.exists():
        with open(POSTER_CACHE, encoding="utf-8") as f:
            return json.load(f)
    return {}


def _save_poster_cache(cache: dict) -> None:
    POSTER_CACHE.parent.mkdir(parents=True, exist_ok=True)
    with open(POSTER_CACHE, "w", encoding="utf-8") as f:
        json.dump(cache, f, ensure_ascii=False, indent=1, sort_keys=True)


# Regional premieres drift a year from a film's canonical Letterboxd year, so
# accept ±1; a hit further off than that is a different film with the same title.
_YEAR_TOL = 1


def _tmdb_search(api_key: str, name: str, year: int | None) -> list[dict]:
    params = {"api_key": api_key, "query": name}
    if year:
        params["year"] = str(year)
    r = requests.get(TMDB_SEARCH, params=params, timeout=15)
    r.raise_for_status()
    return r.json().get("results") or []


def _hit_year(hit: dict) -> int | None:
    rd = (hit.get("release_date") or "")[:4]
    return int(rd) if rd.isdigit() else None


def _best_poster(results: list[dict], year: int | None) -> str | None:
    """First postered candidate whose release year matches (exact, then ±1).

    Year validation is what keeps an underground/foreign title from grabbing a
    wrong same-title film: TMDB ranks by popularity, so the top hit for a niche
    film is often a louder namesake from another year. With no year to check
    against, fall back to TMDB's own ranking (first postered hit)."""
    postered = [h for h in results if h.get("poster_path")]
    if not postered:
        return None
    if not year:
        return postered[0]["poster_path"]
    for tol in range(_YEAR_TOL + 1):        # exact match first, then widen to ±tol
        for h in postered:
            hy = _hit_year(h)
            if hy is not None and abs(hy - year) == tol:
                return h["poster_path"]
    return None                             # results exist, but none the right year


def _tmdb_poster_search(api_key: str, name: str, year: int | None) -> str | None:
    poster = _best_poster(_tmdb_search(api_key, name, year), year)
    if poster:
        return poster
    # A year-constrained search can whiff on off-by-one release dates (regional
    # premieres); retry unconstrained, but still validate the year on the results
    # so the wider net doesn't drag in a wrong-year namesake.
    if year:
        return _best_poster(_tmdb_search(api_key, name, None), year)
    return None


def _tmdb_poster_by_id(api_key: str, tmdb_id: int) -> str | None:
    r = requests.get(TMDB_MOVIE.format(id=tmdb_id),
                     params={"api_key": api_key}, timeout=15)
    if r.status_code == 404:
        return None
    r.raise_for_status()
    return r.json().get("poster_path")


class PosterResolver:
    """poster_fn for letterboxd_store.export_web.

    Order: the hand-pinnable cache file, then TMDB by exact id (films the RSS
    feed has seen), then the year-validated title search. Every network result
    lands in the cache; the store persists the URL onto the film row."""

    def __init__(self):
        self.api_key = os.getenv("TMDB_API_KEY")
        self.cache = _load_poster_cache()
        self.fresh = 0
        self.misses = 0

    def __call__(self, name, year, tmdb_id):
        ck = f"{name} ({year or '?'})"
        if ck not in self.cache:
            if not self.api_key:
                self.misses += 1
                return None
            try:
                if tmdb_id:
                    self.cache[ck] = _tmdb_poster_by_id(self.api_key, tmdb_id)
                else:
                    self.cache[ck] = _tmdb_poster_search(self.api_key, name, year)
            except requests.RequestException as e:
                print(f"  tmdb: {ck}: {e}", file=sys.stderr)
                self.misses += 1
                return None          # transient; retry on the next run
            self.fresh += 1
            time.sleep(0.05)         # stay friendly to TMDB's rate limit
        path = self.cache.get(ck)
        if not path:
            self.misses += 1
        return TMDB_IMG + path if path else None

    def finish(self):
        if self.fresh:
            _save_poster_cache(self.cache)
        if not self.api_key and self.misses:
            print(f"note: TMDB_API_KEY not set in .env — {self.misses} films "
                  "left without a poster (cache/DB only)", file=sys.stderr)
        elif self.fresh:
            print(f"Posters: {self.fresh} new TMDB lookups")


# ─── CLI ─────────────────────────────────────────────────────────────────────

def _write_web(out: Path | None) -> None:
    resolver = PosterResolver()
    try:
        path = store.export_web(out, poster_fn=resolver)
    finally:
        resolver.finish()
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    n_watch = sum(1 for m in data["movies"] if m["status"] == "watchlist")
    n_poster = sum(1 for m in data["movies"] if m["poster_url"])
    print(f"Wrote {path}: {data['count']} films "
          f"({data['count'] - n_watch} watched, {n_watch} watchlist, "
          f"{n_poster} posters)")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_imp = sub.add_parser("import", help="seed/true-up the DB from a data export")
    p_imp.add_argument("export_dir", type=Path,
                       help="unzipped Letterboxd export folder")

    p_rss = sub.add_parser("rss", help="ingest the public RSS feed")
    p_rss.add_argument("--user", default="d33ts", help="Letterboxd username")

    p_web = sub.add_parser("web", help="write movies.json from the DB")

    for p in (p_imp, p_rss):
        p.add_argument("--web", action="store_true",
                       help="also write movies.json afterwards")
    for p in (p_imp, p_rss, p_web):
        p.add_argument("-o", "--out", type=Path, default=None,
                       help="where to write movies.json (default: "
                            "exports/movies.json). Point it at the site to "
                            "publish: ../DeetsSolutions/movies/movies.json")
    args = ap.parse_args()

    if args.cmd == "import":
        if not (args.export_dir / "watched.csv").exists():
            print(f"error: {args.export_dir} has no watched.csv — "
                  "not a Letterboxd export?", file=sys.stderr)
            return 1
        films, watches = parse_export(args.export_dir)
        r = store.import_export(films, watches)
        print(f"Import: {r['films']} films ({r['new_films']} new), "
              f"{r['watches']} diary entries ({r['new_watches']} inserted)")

    elif args.cmd == "rss":
        items = parse_rss(fetch_rss(args.user))
        results = [store.upsert_rss_watch(i) for i in items]
        t = store.record_rss_sync(len(items), results)
        print(f"RSS: {len(items)} entries — {t['new']} new, "
              f"{t['claimed']} claimed, {t['updated']} updated, "
              f"{t['unchanged']} unchanged")

    if args.cmd == "web" or getattr(args, "web", False):
        _write_web(args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
