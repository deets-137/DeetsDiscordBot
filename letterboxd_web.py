#!/usr/bin/env python3
"""Merge a Letterboxd data export into movies/movies.json for the site.

Letterboxd's export (Settings -> Data -> Export) is a folder of CSVs that
each describe one slice of the account: watched.csv is the master film
list, diary.csv adds dated watches/rewatches, ratings.csv the star
ratings, reviews.csv the written reviews, likes/films.csv the hearts,
and watchlist.csv the to-watch queue. This script joins them on
(Name, Year) — verified unique per film — into one flat JSON array the
site's movies page renders, same spirit as scan.py's --web songs.json
export (display data only; the JSON is committed to the site repo).

Posters: the export carries no artwork, so if TMDB_API_KEY is set in
.env each film is looked up on TMDB (search by title + year) and its
poster URL is baked into the JSON. Lookups are cached in
exports/tmdb_posters.json — a regeneration only queries films it hasn't
seen before, so re-runs are instant and offline-safe. No key = no
posters, everything else still works (the site falls back to its
monogram tiles). Free key: https://www.themoviedb.org/settings/api
The site must credit TMDB (the movies page footer does).

Usage:
    python letterboxd_web.py <export-dir> \
        -o "../DeetsSolutions/movies/movies.json"
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import requests
from dotenv import load_dotenv

load_dotenv(Path(__file__).resolve().parent / ".env")

TMDB_SEARCH = "https://api.themoviedb.org/3/search/movie"
TMDB_IMG = "https://image.tmdb.org/t/p/w342"      # ~card-sized poster
POSTER_CACHE = Path(__file__).resolve().parent / "exports" / "tmdb_posters.json"


def load(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with open(path, encoding="utf-8-sig", newline="") as f:
        return list(csv.DictReader(f))


def key(row: dict) -> tuple[str, str]:
    return (row["Name"], row["Year"])


def decade(year: str) -> str | None:
    try:
        return f"{int(year) // 10 * 10}s"
    except (TypeError, ValueError):
        return None


def load_poster_cache() -> dict:
    if POSTER_CACHE.exists():
        with open(POSTER_CACHE, encoding="utf-8") as f:
            return json.load(f)
    return {}


def save_poster_cache(cache: dict) -> None:
    POSTER_CACHE.parent.mkdir(parents=True, exist_ok=True)
    with open(POSTER_CACHE, "w", encoding="utf-8") as f:
        json.dump(cache, f, ensure_ascii=False, indent=1, sort_keys=True)


def tmdb_poster(api_key: str, name: str, year: int | None) -> str | None:
    """Best-match poster path for a film, or None if TMDB doesn't know it."""
    params = {"api_key": api_key, "query": name}
    if year:
        params["year"] = str(year)
    r = requests.get(TMDB_SEARCH, params=params, timeout=15)
    r.raise_for_status()
    results = r.json().get("results") or []
    # A year-constrained search can whiff on off-by-one release dates
    # (regional premieres); retry unconstrained before giving up.
    if not results and year:
        del params["year"]
        r = requests.get(TMDB_SEARCH, params=params, timeout=15)
        r.raise_for_status()
        results = r.json().get("results") or []
    for hit in results:
        if hit.get("poster_path"):
            return hit["poster_path"]
    return None


def add_posters(movies: list[dict]) -> None:
    """Fill each film's poster_url from TMDB, via the on-disk cache."""
    api_key = os.getenv("TMDB_API_KEY")
    cache = load_poster_cache()
    fresh = 0
    try:
        for m in movies:
            m["poster_url"] = None
            ck = f"{m['name']} ({m['year'] or '?'})"
            if ck not in cache:
                if not api_key:
                    continue          # no key: use what the cache has, skip the rest
                try:
                    cache[ck] = tmdb_poster(api_key, m["name"], m["year"])
                except requests.RequestException as e:
                    print(f"  tmdb: {ck}: {e}", file=sys.stderr)
                    continue          # transient; retry on the next run
                fresh += 1
                time.sleep(0.05)      # stay friendly to TMDB's rate limit
            path = cache.get(ck)
            m["poster_url"] = TMDB_IMG + path if path else None
    finally:
        if fresh:
            save_poster_cache(cache)
    hit = sum(1 for m in movies if m.get("poster_url"))
    if not api_key and hit < len(movies):
        print("note: TMDB_API_KEY not set in .env — "
              f"posters only from cache ({hit}/{len(movies)})", file=sys.stderr)
    else:
        print(f"Posters: {hit}/{len(movies)} ({fresh} new lookups)")


def build(export_dir: Path) -> dict:
    watched = load(export_dir / "watched.csv")
    diary = load(export_dir / "diary.csv")
    ratings = load(export_dir / "ratings.csv")
    reviews = load(export_dir / "reviews.csv")
    liked = {key(r) for r in load(export_dir / "likes" / "films.csv")}
    watchlist = load(export_dir / "watchlist.csv")

    movies: dict[tuple[str, str], dict] = {}
    for r in watched:
        movies[key(r)] = {
            "name": r["Name"],
            "year": int(r["Year"]) if r["Year"].isdigit() else None,
            "decade": decade(r["Year"]),
            "uri": r["Letterboxd URI"],
            "logged_date": r["Date"],       # when it entered the account
            "watched_date": None,           # latest diary watch, if diarised
            "watch_count": 0,               # diary entries; 0 = watched, undated
            "rewatch": False,
            "rating": None,
            "liked": key(r) in liked,
            "review": None,
            "review_date": None,
            "status": "watched",
        }

    for r in ratings:
        m = movies.get(key(r))
        if m:
            m["rating"] = float(r["Rating"])

    for r in diary:
        m = movies.get(key(r))
        if not m:
            continue
        m["watch_count"] += 1
        if r.get("Rewatch") == "Yes":
            m["rewatch"] = True
        wd = r.get("Watched Date") or r["Date"]
        if not m["watched_date"] or wd > m["watched_date"]:
            m["watched_date"] = wd

    for r in reviews:  # keep the most recent review per film
        m = movies.get(key(r))
        if not m:
            continue
        if not m["review_date"] or r["Date"] >= m["review_date"]:
            m["review"] = r["Review"]
            m["review_date"] = r["Date"]

    for r in watchlist:
        if key(r) in movies:  # already watched — the queue entry is stale
            continue
        movies[key(r)] = {
            "name": r["Name"],
            "year": int(r["Year"]) if r["Year"].isdigit() else None,
            "decade": decade(r["Year"]),
            "uri": r["Letterboxd URI"],
            "logged_date": r["Date"],
            "watched_date": None,
            "watch_count": 0,
            "rewatch": False,
            "rating": None,
            "liked": False,
            "review": None,
            "review_date": None,
            "status": "watchlist",
        }

    items = sorted(
        movies.values(),
        key=lambda m: (m["watched_date"] or m["logged_date"]),
        reverse=True,
    )
    add_posters(items)
    return {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "letterboxd-export",
        "count": len(items),
        "movies": items,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("export_dir", type=Path, help="unzipped Letterboxd export folder")
    ap.add_argument("-o", "--out", type=Path,
                    default=Path(__file__).resolve().parent / "exports" / "movies.json",
                    help="where to write movies.json (default: exports/movies.json). "
                         "Point it at the site to publish, e.g. "
                         "../DeetsSolutions/movies/movies.json")
    args = ap.parse_args()

    if not (args.export_dir / "watched.csv").exists():
        print(f"error: {args.export_dir} has no watched.csv — not a Letterboxd export?",
              file=sys.stderr)
        return 1

    data = build(args.export_dir)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
        f.write("\n")

    n_watch = sum(1 for m in data["movies"] if m["status"] == "watchlist")
    print(f"Wrote {args.out}: {data['count']} films "
          f"({data['count'] - n_watch} watched, {n_watch} watchlist)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
