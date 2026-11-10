"""
Persistent film journal for DeetsOTD, backed by the same SQLite file as songs.

Mirrors storage.py's posts/songs split for the Letterboxd side of the house:
  films         — one row per film the account knows: identity (name, year),
                  the Letterboxd URI, TMDB id + poster, film-level rating /
                  like / watched-vs-watchlist status.
  film_watches  — the journal: one row per diary sitting, with its own date,
                  rating, rewatch flag, tags, and review. raw_json keeps the
                  full source payload (CSV row or RSS item) so no field is
                  ever lost. rss_guid is Letterboxd's stable per-entry id
                  ('letterboxd-review-N' / 'letterboxd-watch-N') — the dedupe
                  key that lets the RSS feed be re-read forever without
                  double-recording, and stays put when an entry is edited.
  film_syncs    — append-only audit log of every import/ingest run.

Two writers feed it, with a clear seniority rule:
  * a full Letterboxd data export (the archive; seeds and true-ups history),
  * the public RSS feed (the last ~50 diary entries; the nightly top-up).
Where the two overlap, the export wins for text: RSS reviews are HTML
rendered from what was typed, so a sitting first recorded from CSV keeps its
verbatim review and an RSS re-read only fills fields the CSV left NULL.
Sittings born from RSS have no better source, so a re-read updates them
freely (that's how a rating edit or late review lands).

Re-importing an export REPLACES the guid-less export-era rows wholesale
(that's how deletions and edits in Letterboxd's own archive propagate), but
never touches a row that carries a guid — those are the feed's to manage.

This module is the only thing that writes the film tables; letterboxd_web.py
does the fetching/parsing and calls in here. storage.py stays the sole
writer of the song tables. Both connect to the same storage.db (autocommit,
additive-only DDL), which is what makes cross-journal analysis one JOIN away.
"""

from __future__ import annotations

import json
import sqlite3
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Optional

from .paths import DB_PATH as _DB_PATH
from .paths import EXPORTS_DIR

_conn: Optional[sqlite3.Connection] = None


def _db() -> sqlite3.Connection:
    global _conn
    if _conn is None:
        _conn = sqlite3.connect(_DB_PATH, isolation_level=None)  # autocommit
        _conn.row_factory = sqlite3.Row
        _conn.execute("PRAGMA foreign_keys = ON")
        _init_schema(_conn)
    return _conn


def _init_schema(c: sqlite3.Connection) -> None:
    # Additive-only: every statement is idempotent. Never DROP or rename.
    c.executescript("""
        CREATE TABLE IF NOT EXISTS films (
            film_id     INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            year        INTEGER,           -- NULL when Letterboxd has none
            uri         TEXT,              -- film page (boxd.it from exports)
            tmdb_id     INTEGER,           -- from RSS <tmdb:movieId>
            poster_url  TEXT,              -- resolved TMDB poster, full URL
            status      TEXT NOT NULL DEFAULT 'watched',  -- watched | watchlist
            liked       INTEGER NOT NULL DEFAULT 0,       -- film-level heart
            rating      REAL,              -- current star rating (0.5–5.0)
            logged_date TEXT,              -- when it entered the account
            recorded_at INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_films_identity ON films(name, year);

        CREATE TABLE IF NOT EXISTS film_watches (
            watch_id     INTEGER PRIMARY KEY AUTOINCREMENT,
            film_id      INTEGER NOT NULL REFERENCES films(film_id)
                             ON DELETE CASCADE,
            rss_guid     TEXT UNIQUE,      -- NULL until the feed has seen it
            watched_date TEXT NOT NULL,    -- ISO date of the sitting
            logged_date  TEXT,             -- when the diary entry was created
            logged_at    TEXT,             -- full UTC timestamp of the logging,
                                           -- from RSS pubDate; NULL pre-feed.
                                           -- The same-day tiebreaker: entries
                                           -- logged in watch order, seconds
                                           -- apart, sort a marathon correctly
            entry_uri    TEXT,             -- per-entry boxd.it link (exports)
            rating       REAL,
            liked        INTEGER,          -- RSS memberLike; NULL = unknown
            rewatch      INTEGER NOT NULL DEFAULT 0,
            tags         TEXT,             -- diary tags, comma-joined (exports)
            review       TEXT,
            review_date  TEXT,             -- when the review was logged
            source       TEXT NOT NULL,    -- 'export' | 'rss'
            raw_json     TEXT NOT NULL DEFAULT '{}',
            recorded_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_film_watches_film
            ON film_watches(film_id, watched_date);

        CREATE TABLE IF NOT EXISTS film_syncs (
            sync_id     INTEGER PRIMARY KEY AUTOINCREMENT,
            source      TEXT NOT NULL,     -- 'export' | 'rss'
            synced_at   INTEGER NOT NULL,
            items       INTEGER NOT NULL,  -- rows/feed items seen this run
            new_films   INTEGER NOT NULL,
            new_watches INTEGER NOT NULL,
            updated     INTEGER NOT NULL   -- watches touched (claimed/edited)
        );
    """)
    # Additive column migrations (CREATE IF NOT EXISTS can't grow a table).
    cols = {r[1] for r in c.execute("PRAGMA table_info(film_watches)")}
    if "logged_at" not in cols:
        c.execute("ALTER TABLE film_watches ADD COLUMN logged_at TEXT")


# ─── Films ───────────────────────────────────────────────────────────────────

_UNSET = object()


def _find_film(c: sqlite3.Connection, name: str, year: Optional[int]):
    return c.execute(
        "SELECT * FROM films WHERE name = ? AND year IS ?", (name, year)
    ).fetchone()


def upsert_film(name: str, year: Optional[int], *, uri=_UNSET, tmdb_id=_UNSET,
                status=_UNSET, liked=_UNSET, rating=_UNSET,
                logged_date=_UNSET) -> tuple[int, bool]:
    """Insert or update a film by (name, year). Returns (film_id, created).

    Only the keyword fields actually passed are written, so a caller that
    doesn't know a field can't blank it. uri and logged_date are
    first-writer-wins (the export's boxd.it link and account-entry date are
    better than anything the feed can offer later)."""
    c = _db()
    now = int(time.time())
    row = _find_film(c, name, year)
    if row is None:
        c.execute(
            """INSERT INTO films (name, year, uri, tmdb_id, status, liked,
                   rating, logged_date, recorded_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (name, year,
             None if uri is _UNSET else uri,
             None if tmdb_id is _UNSET else tmdb_id,
             "watched" if status is _UNSET else status,
             0 if liked is _UNSET else int(bool(liked)),
             None if rating is _UNSET else rating,
             None if logged_date is _UNSET else logged_date,
             now, now))
        return c.execute("SELECT last_insert_rowid()").fetchone()[0], True

    sets, args = [], []
    if uri is not _UNSET and uri and not row["uri"]:
        sets.append("uri = ?"); args.append(uri)
    if logged_date is not _UNSET and logged_date and not row["logged_date"]:
        sets.append("logged_date = ?"); args.append(logged_date)
    if tmdb_id is not _UNSET and tmdb_id is not None:
        sets.append("tmdb_id = ?"); args.append(tmdb_id)
    if status is not _UNSET and status:
        sets.append("status = ?"); args.append(status)
    if liked is not _UNSET and liked is not None:
        sets.append("liked = ?"); args.append(int(bool(liked)))
    if rating is not _UNSET and rating is not None:
        sets.append("rating = ?"); args.append(rating)
    if sets:
        sets.append("updated_at = ?"); args.append(now)
        args.append(row["film_id"])
        c.execute(f"UPDATE films SET {', '.join(sets)} WHERE film_id = ?", args)
    return row["film_id"], False


def set_film_poster(film_id: int, poster_url: Optional[str]) -> None:
    _db().execute("UPDATE films SET poster_url = ?, updated_at = ? WHERE film_id = ?",
                  (poster_url, int(time.time()), film_id))


# ─── Export import (seed / true-up) ──────────────────────────────────────────

def import_export(films: list[dict], watches: list[dict]) -> dict:
    """Fold a parsed Letterboxd data export into the journal.

    films:   {name, year, uri, logged_date, rating, liked, status}
    watches: {name, year, watched_date, logged_date, entry_uri, rating,
              rewatch, tags, review, review_date, raw}

    Guid-less export-era watches are deleted and re-inserted (the archive is
    authoritative for its own history — that's how a diary edit or deletion
    made on Letterboxd propagates). Guid-bearing rows are the feed's and are
    left alone; a CSV sitting that matches one on (film, watched date) is
    skipped rather than duplicated."""
    c = _db()
    now = int(time.time())
    new_films = new_watches = 0

    ids: dict[tuple, int] = {}
    for f in films:
        fid, created = upsert_film(
            f["name"], f["year"], uri=f.get("uri"),
            logged_date=f.get("logged_date"), rating=f.get("rating"),
            liked=f.get("liked"), status=f.get("status", "watched"))
        ids[(f["name"], f["year"])] = fid
        new_films += created

    c.execute("BEGIN")
    try:
        c.execute("DELETE FROM film_watches WHERE rss_guid IS NULL AND source = 'export'")
        # How many guid-bearing rows already cover each (film, date)? Skip
        # that many CSV twins so the overlap window never double-records.
        covered: dict[tuple, int] = {}
        for r in c.execute("SELECT film_id, watched_date FROM film_watches"):
            k = (r["film_id"], r["watched_date"])
            covered[k] = covered.get(k, 0) + 1
        for w in watches:
            fid = ids.get((w["name"], w["year"]))
            if fid is None:
                continue
            k = (fid, w["watched_date"])
            if covered.get(k, 0) > 0:
                covered[k] -= 1
                continue
            c.execute(
                """INSERT INTO film_watches
                       (film_id, watched_date, logged_date, entry_uri, rating,
                        rewatch, tags, review, review_date, source, raw_json,
                        recorded_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'export', ?, ?)""",
                (fid, w["watched_date"], w.get("logged_date"),
                 w.get("entry_uri"), w.get("rating"),
                 int(bool(w.get("rewatch"))), w.get("tags"), w.get("review"),
                 w.get("review_date"), json.dumps(w.get("raw") or {}), now))
            new_watches += 1
        c.execute("COMMIT")
    except Exception:
        c.execute("ROLLBACK")
        raise

    c.execute(
        """INSERT INTO film_syncs (source, synced_at, items, new_films,
               new_watches, updated) VALUES ('export', ?, ?, ?, ?, 0)""",
        (now, len(watches), new_films, new_watches))
    return {"films": len(films), "new_films": new_films,
            "watches": len(watches), "new_watches": new_watches}


# ─── RSS ingest (the nightly top-up) ─────────────────────────────────────────

def _bump_film_meta(c: sqlite3.Connection, fid: int, item: dict) -> None:
    """Let a 'new'/'updated' sitting drive the film-level rating and like —
    but only if it is the film's latest sitting (call AFTER the watch row is
    written, so MAX includes it). An older entry carries the rating given
    back then; re-reading it must not clobber the current one."""
    if item.get("rating") is None and item.get("liked") is None:
        return
    newest = c.execute(
        "SELECT MAX(watched_date) FROM film_watches WHERE film_id = ?",
        (fid,)).fetchone()[0]
    if newest is not None and item["watched_date"] < newest:
        return
    upsert_film(item["name"], item["year"],
                rating=item.get("rating"), liked=item.get("liked"))


def upsert_rss_watch(item: dict) -> str:
    """Fold one feed item in. Returns 'new' | 'claimed' | 'updated' | 'unchanged'.

    item: {guid, name, year, watched_date, rating, liked, rewatch, review,
           uri, tmdb_id, pub_date, raw}

    Three cases, in order:
      * guid already on record → refresh it (an edit re-read). Review text is
        only overwritten on rss-born rows — export-recorded reviews are
        verbatim and outrank the feed's HTML round-trip.
      * same film + watched date recorded from an export, no guid yet → claim
        it: attach the guid and fill only the NULLs. (The seed export and the
        feed's ~6-month window overlap; this is that seam.)
      * genuinely new → insert.
    The film row is upserted either way — a new film arrives with its tmdb_id
    in hand and a watchlisted film flips to watched. The film-level rating and
    like follow the feed only on a 'new' or 'updated' item that is also the
    film's LATEST sitting: an item carries the rating given at that sitting,
    so an older entry re-read tomorrow must not clobber today's rating, and a
    claim never moves them at all — the export that recorded the sitting also
    supplied the film's current rating, which outranks the feed's older
    per-sitting one (ratings do get changed without a new diary entry).
    """
    c = _db()
    now = int(time.time())
    fid, _ = upsert_film(
        item["name"], item["year"], uri=item.get("uri"),
        tmdb_id=item.get("tmdb_id"), status="watched",
        logged_date=item.get("pub_date"))

    raw = json.dumps(item.get("raw") or {})
    row = c.execute("SELECT * FROM film_watches WHERE rss_guid = ?",
                    (item["guid"],)).fetchone()
    if row:
        keep_review = row["review"] if (row["source"] == "export" and row["review"]) \
            else (item.get("review") or row["review"])
        review_date = row["review_date"] or (item.get("pub_date") if item.get("review") else None)
        fields = (item["watched_date"], item.get("rating"), item.get("liked"),
                  int(bool(item.get("rewatch"))), keep_review, review_date,
                  item.get("logged_ts") or row["logged_at"], raw)
        current = (row["watched_date"], row["rating"], row["liked"],
                   row["rewatch"], row["review"], row["review_date"],
                   row["logged_at"], row["raw_json"])
        if fields == current:
            return "unchanged"
        c.execute(
            """UPDATE film_watches SET watched_date=?, rating=?, liked=?,
                   rewatch=?, review=?, review_date=?, logged_at=?, raw_json=?
                 WHERE watch_id=?""", (*fields, row["watch_id"]))
        _bump_film_meta(c, fid, item)
        return "updated"

    row = c.execute(
        """SELECT * FROM film_watches
            WHERE film_id = ? AND watched_date = ? AND rss_guid IS NULL
            ORDER BY watch_id LIMIT 1""",
        (fid, item["watched_date"])).fetchone()
    if row:
        c.execute(
            """UPDATE film_watches SET rss_guid=?,
                   rating   = COALESCE(rating, ?),
                   liked    = COALESCE(liked, ?),
                   review   = COALESCE(review, ?),
                   review_date = COALESCE(review_date, ?),
                   logged_at = ?,
                   raw_json = ?
                 WHERE watch_id=?""",
            (item["guid"], item.get("rating"), item.get("liked"),
             item.get("review"),
             item.get("pub_date") if item.get("review") else None,
             item.get("logged_ts"), raw, row["watch_id"]))
        return "claimed"

    c.execute(
        """INSERT INTO film_watches
               (film_id, rss_guid, watched_date, logged_date, logged_at,
                rating, liked, rewatch, review, review_date, source, raw_json,
                recorded_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'rss', ?, ?)""",
        (fid, item["guid"], item["watched_date"], item.get("pub_date"),
         item.get("logged_ts"),
         item.get("rating"), item.get("liked"), int(bool(item.get("rewatch"))),
         item.get("review"),
         item.get("pub_date") if item.get("review") else None, raw, now))
    _bump_film_meta(c, fid, item)
    return "new"


def record_rss_sync(items: int, results: list[str]) -> dict:
    """Log an RSS run to film_syncs and return its tallies."""
    tally = {"new": 0, "claimed": 0, "updated": 0, "unchanged": 0}
    for r in results:
        tally[r] += 1
    _db().execute(
        """INSERT INTO film_syncs (source, synced_at, items, new_films,
               new_watches, updated) VALUES ('rss', ?, ?, 0, ?, ?)""",
        (int(time.time()), items, tally["new"],
         tally["claimed"] + tally["updated"]))
    return tally


# ─── Website export ──────────────────────────────────────────────────────────

def _decade(year) -> Optional[str]:
    return f"{year // 10 * 10}s" if isinstance(year, int) else None


def export_web(out_path: Optional[Path] = None,
               poster_fn: Optional[Callable] = None) -> Path:
    """Write the film journal as a website-ready movies.json.

    Shape matches what the movies page has always rendered: one flat object
    per film with computed aggregates (latest watched date, watch count,
    any-rewatch, most recent review) plus the full `watches` array, newest
    sitting first. Aggregates are computed here, never stored, so they can't
    drift from the sittings they summarize.

    poster_fn(name, year, tmdb_id) -> url|None resolves missing posters
    (letterboxd_web.py supplies the TMDB-backed one); results persist onto
    the film row so later runs are free. Returns the path written."""
    c = _db()
    films = c.execute("SELECT * FROM films").fetchall()
    # Same-day sittings order on the logging timestamp (RSS pubDate) — a
    # marathon night's entries are logged in watch order, seconds apart.
    # Pre-feed history has no timestamp and keeps diary-position order.
    watches = c.execute(
        """SELECT * FROM film_watches
           ORDER BY watched_date DESC, COALESCE(logged_at, '') DESC,
                    watch_id DESC""").fetchall()
    by_film: dict[int, list[sqlite3.Row]] = {}
    for w in watches:
        by_film.setdefault(w["film_id"], []).append(w)

    movies = []
    tiebreak: dict[int, str] = {}   # id(m) -> latest sitting's logged_at
    for f in films:
        ws = by_film.get(f["film_id"], [])
        best_review = None
        for w in ws:
            if w["review"] and (best_review is None or
                                (w["review_date"] or "") >= (best_review["review_date"] or "")):
                best_review = w
        m = {
            "name": f["name"],
            "year": f["year"],
            "decade": _decade(f["year"]),
            "uri": f["uri"],
            "logged_date": f["logged_date"],
            "watched_date": ws[0]["watched_date"] if ws else None,
            "watch_count": len(ws),
            "rewatch": any(w["rewatch"] for w in ws),
            "rating": f["rating"],
            "liked": bool(f["liked"]) or any(w["liked"] for w in ws),
            "review": best_review["review"] if best_review else None,
            "review_date": best_review["review_date"] if best_review else None,
            "watches": [{
                "date": w["watched_date"],
                "rating": w["rating"],
                "rewatch": bool(w["rewatch"]),
                "review": w["review"],
            } for w in ws],
            "status": f["status"],
            "poster_url": f["poster_url"],
        }
        # Always re-ask the resolver (its cache file is the hand-pin override
        # and must win over the stored URL); the film row keeps a copy so the
        # DB is self-contained for analysis, and covers a missing resolver.
        if poster_fn is not None:
            m["poster_url"] = poster_fn(f["name"], f["year"], f["tmdb_id"]) \
                or f["poster_url"]
            if m["poster_url"] != f["poster_url"]:
                set_film_poster(f["film_id"], m["poster_url"])
        tiebreak[id(m)] = (ws[0]["logged_at"] or "") if ws else ""
        movies.append(m)

    # Films watched the same day tie-break on their latest sitting's logging
    # timestamp, so a marathon lists in watch order (newest first). The key
    # rides outside the JSON — the page renders array order for ties.
    movies.sort(key=lambda m: (m["watched_date"] or m["logged_date"] or "",
                               tiebreak[id(m)]),
                reverse=True)
    payload = {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "letterboxd",
        "count": len(movies),
        "movies": movies,
    }

    if out_path is None:
        EXPORTS_DIR.mkdir(parents=True, exist_ok=True)
        out_path = EXPORTS_DIR / "movies.json"
    else:
        out_path = Path(out_path)
        out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, indent=1)
        fh.write("\n")
    return out_path


def counts() -> dict:
    """Quick tallies for CLI summaries."""
    c = _db()
    return {
        "films": c.execute("SELECT COUNT(*) FROM films").fetchone()[0],
        "watched": c.execute(
            "SELECT COUNT(*) FROM films WHERE status = 'watched'").fetchone()[0],
        "watchlist": c.execute(
            "SELECT COUNT(*) FROM films WHERE status = 'watchlist'").fetchone()[0],
        "watches": c.execute("SELECT COUNT(*) FROM film_watches").fetchone()[0],
    }


def close() -> None:
    global _conn
    if _conn is not None:
        _conn.close()
        _conn = None
