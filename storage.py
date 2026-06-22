"""
Persistent archive for DeetsOTD, backed by SQLite.

The channel is treated as a *journal*: every song link posted is one entry, kept
in order. This module is the only thing that writes the DB; linkscan.py stays a
pure extractor and enrich.py only returns data.

Design (mirrors DeetsCode's storage.py):
  - One DB file at project root: storage.db (gitignored).
  - Singleton autocommit connection, Row factory, foreign keys on.
  - Additive-only, idempotent DDL. Safe to re-run on every startup.

Model:
  scans  — append-only log of every scan run (audit / "last scanned").
  posts  — the journal: one row per song link posted. Keyed by (message_id, url)
           so re-scanning the same channel never double-records a post. Carries
           the Discord user id, so a nickname change never fragments a person's
           history. A nullable song_id links a post to its resolved track.
  songs  — the SongOTD catalog: one row per iTunes-resolved track. Many posts can
           point at one song. raw_json keeps the full payload so no field is lost.
"""

from __future__ import annotations

import json
import sqlite3
import time
from collections import OrderedDict
from typing import Optional

from linkscan import detect_service
from paths import DB_PATH as _DB_PATH

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
        CREATE TABLE IF NOT EXISTS scans (
            scan_id       INTEGER PRIMARY KEY AUTOINCREMENT,
            channel_id    TEXT NOT NULL,
            channel_label TEXT NOT NULL,
            scanned_at    INTEGER NOT NULL,
            msg_limit     INTEGER,
            total_posts   INTEGER NOT NULL DEFAULT 0,
            new_posts     INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_scans_channel ON scans(channel_id);

        CREATE TABLE IF NOT EXISTS songs (
            song_id          INTEGER PRIMARY KEY AUTOINCREMENT,
            itunes_track_id  INTEGER UNIQUE,
            track_name       TEXT,
            artist_name      TEXT,
            collection_name  TEXT,
            primary_genre    TEXT,
            release_date     TEXT,
            artwork_url      TEXT,
            preview_url      TEXT,
            track_view_url   TEXT,
            match_type       TEXT,          -- 'direct' (Apple id) | 'search' (best-effort)
            raw_json         TEXT NOT NULL DEFAULT '{}',
            enriched_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS posts (
            post_id     INTEGER PRIMARY KEY AUTOINCREMENT,
            channel_id  TEXT NOT NULL,
            author      TEXT NOT NULL,
            author_id   TEXT NOT NULL,
            posted_at   TEXT NOT NULL,      -- the message's timestamp, ISO 8601 UTC
            url         TEXT NOT NULL,
            service     TEXT NOT NULL,      -- apple_music | spotify | youtube | ... | other
            message_id  TEXT NOT NULL,
            jump_url    TEXT NOT NULL,
            song_id     INTEGER,
            recorded_at INTEGER NOT NULL,
            UNIQUE (message_id, url),
            FOREIGN KEY (song_id) REFERENCES songs(song_id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_posts_channel ON posts(channel_id, posted_at);
        CREATE INDEX IF NOT EXISTS idx_posts_author  ON posts(channel_id, author_id);
        CREATE INDEX IF NOT EXISTS idx_posts_service ON posts(service);
    """)


# ─── Writing the journal ─────────────────────────────────────────────────────

def record_scan(channel_id, label, records, limit=None) -> dict:
    """Fold a scan's link records into the journal.

    Only links the user actually typed (source == "text") become posts — Discord's
    auto-generated embed/attachment urls are noise for a song journal. Idempotent:
    a post already on record (same message_id + url) is skipped. Logs a scan row.
    Returns counts for reporting.
    """
    c = _db()
    now = int(time.time())
    cid = str(channel_id)

    posts = [r for r in records if r.source == "text"]
    new_count = 0
    for r in posts:
        cur = c.execute(
            """INSERT OR IGNORE INTO posts
                   (channel_id, author, author_id, posted_at, url, service,
                    message_id, jump_url, recorded_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (cid, r.author, str(r.author_id), r.timestamp, r.url,
             detect_service(r.url), str(r.message_id), r.jump_url, now))
        if cur.rowcount:
            new_count += 1

    cur = c.execute(
        """INSERT INTO scans
               (channel_id, channel_label, scanned_at, msg_limit, total_posts, new_posts)
           VALUES (?, ?, ?, ?, ?, ?)""",
        (cid, label, now, limit, len(posts), new_count))

    return {
        "scan_id": cur.lastrowid,
        "total": len(posts),
        "new": new_count,
        "known": len(posts) - new_count,
    }


# ─── SongOTD enrichment ──────────────────────────────────────────────────────

def upsert_song(track: dict, match_type: str) -> int:
    """Insert (or fetch existing) a song from an iTunes track payload. Returns song_id.

    Keyed on itunes_track_id; re-enriching a known track refreshes its fields.
    """
    c = _db()
    tid = track.get("trackId")
    row = c.execute("SELECT song_id FROM songs WHERE itunes_track_id = ?",
                    (tid,)).fetchone() if tid is not None else None
    fields = (
        tid,
        track.get("trackName"),
        track.get("artistName"),
        track.get("collectionName"),
        track.get("primaryGenreName"),
        track.get("releaseDate"),
        track.get("artworkUrl100"),
        track.get("previewUrl"),
        track.get("trackViewUrl"),
        match_type,
        json.dumps(track),
        int(time.time()),
    )
    if row:
        c.execute(
            """UPDATE songs SET itunes_track_id=?, track_name=?, artist_name=?,
                   collection_name=?, primary_genre=?, release_date=?, artwork_url=?,
                   preview_url=?, track_view_url=?, match_type=?, raw_json=?, enriched_at=?
                 WHERE song_id=?""", (*fields, row["song_id"]))
        return row["song_id"]
    cur = c.execute(
        """INSERT INTO songs (itunes_track_id, track_name, artist_name, collection_name,
               primary_genre, release_date, artwork_url, preview_url, track_view_url,
               match_type, raw_json, enriched_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""", fields)
    return cur.lastrowid


def link_post_to_song(post_id: int, song_id: int) -> None:
    _db().execute("UPDATE posts SET song_id = ? WHERE post_id = ?", (song_id, post_id))


def posts_needing_enrichment(channel_id=None) -> list[sqlite3.Row]:
    """Posts that don't yet have a resolved song, newest first."""
    q = "SELECT * FROM posts WHERE song_id IS NULL"
    args: tuple = ()
    if channel_id is not None:
        q += " AND channel_id = ?"
        args = (str(channel_id),)
    q += " ORDER BY posted_at DESC"
    return _db().execute(q, args).fetchall()


# ─── Reading ─────────────────────────────────────────────────────────────────

def posts_by_user(channel_id) -> "OrderedDict[str, list[sqlite3.Row]]":
    """The whole journal for a channel, grouped by author (most posts first)."""
    rows = _db().execute(
        "SELECT * FROM posts WHERE channel_id = ? ORDER BY author_id, posted_at",
        (str(channel_id),)).fetchall()
    grouped: "OrderedDict[str, list[sqlite3.Row]]" = OrderedDict()
    for row in rows:
        grouped.setdefault(row["author_id"], []).append(row)
    return OrderedDict(
        sorted(grouped.items(), key=lambda kv: len(kv[1]), reverse=True))


def list_scans(channel_id=None) -> list[sqlite3.Row]:
    """Scan-run history, newest first. Optionally filtered to one channel."""
    if channel_id is None:
        return _db().execute("SELECT * FROM scans ORDER BY scanned_at DESC").fetchall()
    return _db().execute(
        "SELECT * FROM scans WHERE channel_id = ? ORDER BY scanned_at DESC",
        (str(channel_id),)).fetchall()


def channels() -> list[sqlite3.Row]:
    """One row per channel in the journal, with post/author tallies."""
    return _db().execute(
        """SELECT p.channel_id,
                  (SELECT channel_label FROM scans s
                    WHERE s.channel_id = p.channel_id
                    ORDER BY scanned_at DESC LIMIT 1) AS label,
                  COUNT(DISTINCT p.author_id)         AS authors,
                  COUNT(*)                            AS posts,
                  MAX(p.posted_at)                    AS latest
             FROM posts p
            GROUP BY p.channel_id
            ORDER BY latest DESC""").fetchall()


def close() -> None:
    global _conn
    if _conn is not None:
        _conn.close()
        _conn = None
