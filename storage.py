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

import csv
import json
import sqlite3
import time
from collections import OrderedDict
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from datetime import timedelta

from linkscan import detect_service, is_non_music
from paths import DB_PATH as _DB_PATH
from paths import EXPORTS_DIR

# ── Journal-day semantics ────────────────────────────────────────────
# posted_at is stored in UTC (the raw truth, never rewritten); the derived
# display fields (date / day / month / time) live in the journal's home
# timezone with a late-night grace rule: the day flips at 05:00, not
# midnight, so a 3 AM upload is the tail of yesterday, not the start of
# today. America/Los_Angeles anchors it — Aditya moved to the West coast,
# and the journal follows wherever he is posting from. The 5-hour grace
# still absorbs the travel cases (5 AM Pacific = 7 AM Central / 8 AM
# Eastern, so a Central late-nighter rolls back and only a genuine
# morning post starts a new day). Falls back to fixed PST when no tz
# database is installed (on Windows, `pip install tzdata` gets real DST
# handling — it's in requirements.txt).
#
# This anchor is deliberately retroactive: the whole journal is re-bucketed
# under the current rule rather than carrying a cutover date per move. Only
# posts sent between 2 and 5 AM Pacific are affected at all (Chicago-4h and
# Pacific-5h differ by exactly 3 hours, year-round — both zones share DST
# dates), which was 5 of 341 posts at the time of the switch.
GRACE_HOURS = 5
try:
    from zoneinfo import ZoneInfo
    JOURNAL_TZ = ZoneInfo("America/Los_Angeles")
except Exception:
    JOURNAL_TZ = timezone(timedelta(hours=-8))

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
    auto-generated embed/attachment urls are noise for a song journal. Non-music
    hosts (Tenor gifs, image CDNs, …) are dropped too. Idempotent: a post already
    on record (same message_id + url) is skipped. Logs a scan row. Returns counts.
    """
    c = _db()
    now = int(time.time())
    cid = str(channel_id)

    posts = [r for r in records
             if r.source == "text" and not is_non_music(r.url)]
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
    if tid is not None:
        row = c.execute("SELECT song_id FROM songs WHERE itunes_track_id = ?",
                        (tid,)).fetchone()
    else:
        # Synthetic (e.g. Spotify-truth) songs have no iTunes id; dedupe on identity.
        row = c.execute(
            """SELECT song_id FROM songs WHERE itunes_track_id IS NULL
                   AND track_name = ? AND artist_name = ? AND collection_name = ?""",
            (track.get("trackName"), track.get("artistName"),
             track.get("collectionName"))).fetchone()
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


def set_song_match_type(song_id: int, match_type: str) -> None:
    """Re-tag a song's match (e.g. flip a hand-verified 'review' to 'manual')."""
    _db().execute("UPDATE songs SET match_type = ? WHERE song_id = ?",
                  (match_type, song_id))


def songs_needing_review(channel_id=None) -> list[sqlite3.Row]:
    """Songs whose match is an unconfirmed best-effort guess ('review'), joined to
    the post(s) pointing at them so a human has the original link for context.
    One row per (song, post); oldest post first."""
    q = """SELECT s.song_id, s.track_name, s.artist_name, s.collection_name,
                  s.primary_genre, s.release_date, s.track_view_url,
                  p.post_id, p.author, p.posted_at, p.service, p.url, p.jump_url
             FROM songs s
             JOIN posts p ON p.song_id = s.song_id
            WHERE s.match_type = 'review'"""
    args: tuple = ()
    if channel_id is not None:
        q += " AND p.channel_id = ?"
        args = (str(channel_id),)
    q += " ORDER BY p.posted_at"
    return _db().execute(q, args).fetchall()


def posts_needing_enrichment(channel_id=None) -> list[sqlite3.Row]:
    """Posts that don't yet have a resolved song, newest first."""
    q = "SELECT * FROM posts WHERE song_id IS NULL"
    args: tuple = ()
    if channel_id is not None:
        q += " AND channel_id = ?"
        args = (str(channel_id),)
    q += " ORDER BY posted_at DESC"
    return _db().execute(q, args).fetchall()


def find_songs(query, channel_id=None) -> list[sqlite3.Row]:
    """Songs whose track / artist / album contains a substring, with the number of
    posts pointing at each and the earliest one for context.

    The lookup behind `review.py search`: for hand-correcting a song that resolved
    *confidently but wrong* (tagged 'apple'/'search'/'spotify'), which
    songs_needing_review never surfaces because nothing flagged it. Find it here by
    what shows on the site, then correct it by song_id."""
    like = f"%{query}%"
    q = """SELECT s.song_id, s.track_name, s.artist_name, s.collection_name,
                  s.match_type, s.track_view_url,
                  COUNT(p.post_id)  AS post_count,
                  MIN(p.posted_at)  AS first_posted,
                  (SELECT jump_url FROM posts
                     WHERE song_id = s.song_id ORDER BY posted_at LIMIT 1) AS jump_url
             FROM songs s
             JOIN posts p ON p.song_id = s.song_id
            WHERE (s.track_name LIKE ? OR s.artist_name LIKE ?
                   OR s.collection_name LIKE ?)"""
    args: list = [like, like, like]
    if channel_id is not None:
        q += " AND p.channel_id = ?"
        args.append(str(channel_id))
    q += " GROUP BY s.song_id ORDER BY first_posted"
    return _db().execute(q, args).fetchall()


def song_row(song_id) -> Optional[sqlite3.Row]:
    """One catalog song by id, or None."""
    return _db().execute(
        "SELECT * FROM songs WHERE song_id = ?", (song_id,)).fetchone()


def posts_for_song(song_id) -> list[sqlite3.Row]:
    """Every post pointing at a song — the set a hand-correction must re-link."""
    return _db().execute(
        "SELECT * FROM posts WHERE song_id = ? ORDER BY posted_at",
        (song_id,)).fetchall()


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


# Journal column order: (row-dict key, spreadsheet header). Grouped when / who /
# what / release / meta / ids+links for readability.
_JOURNAL_COLS = [
    ("date", "date"), ("time", "time"), ("day", "day"), ("month", "month"),
    ("author", "author"),
    ("track_name", "track"), ("artist_name", "artist"), ("album", "album"),
    ("genre", "genre"), ("duration", "duration"),
    ("release_date", "released"), ("release_year", "rel_year"),
    ("age_yrs", "age_yrs"),
    ("service", "service"), ("match_type", "match"), ("resolved", "resolved"),
    ("author_id", "author_id"),
    ("url", "source_url"), ("apple_music_url", "apple_music_url"),
    ("preview_url", "preview_url"), ("artwork_url", "artwork_url"),
    ("jump_url", "jump_url"),
]

# In xlsx, these columns render as a compact clickable label instead of a raw URL.
_LINK_LABELS = {
    "url": "source", "apple_music_url": "Apple", "preview_url": "▶ play",
    "artwork_url": "cover", "jump_url": "↗ msg",
}


def _journal_rows(channel_id) -> list[dict]:
    """Posts joined to resolved songs, enriched with derived fields, oldest→newest.

    Adds: day-of-week, month bucket (MM_YYYY), track duration (mm:ss, parsed from
    the cached iTunes payload), release year, song age in years at post time, a
    resolved flag, and the artwork / preview / Apple-Music links.
    """
    raw = _db().execute(
        """SELECT p.post_id, p.message_id, p.author, p.author_id, p.posted_at,
                  p.service, p.url, p.jump_url, p.song_id, s.track_name,
                  s.artist_name, s.collection_name, s.primary_genre,
                  s.release_date, s.match_type, s.artwork_url, s.preview_url,
                  s.track_view_url, s.raw_json
             FROM posts p
             LEFT JOIN songs s ON s.song_id = p.song_id
            WHERE p.channel_id = ?
            ORDER BY p.posted_at""", (str(channel_id),)).fetchall()

    out = []
    for r in raw:
        # Journal day: local (JOURNAL_TZ) with the 5 AM grace boundary —
        # implemented by sliding the timestamp back GRACE_HOURS before
        # taking .date(). `time` stays the post's actual local clock time.
        date, t, day = "", "", ""
        if r["posted_at"]:
            try:
                dt = datetime.fromisoformat(r["posted_at"])
                if dt.tzinfo is None:
                    dt = dt.replace(tzinfo=timezone.utc)
                local = dt.astimezone(JOURNAL_TZ)
                jday = (local - timedelta(hours=GRACE_HOURS)).date()
                date = jday.isoformat()
                day = jday.strftime("%a")
                t = local.strftime("%H:%M:%S")
            except ValueError:              # malformed timestamp: old behavior
                date, _, t = r["posted_at"].partition("T")
                t = t[:8]
                try:
                    day = datetime.strptime(date, "%Y-%m-%d").strftime("%a")
                except ValueError:
                    pass

        rel = (r["release_date"] or "")[:10]
        rel_year = rel[:4]
        age = ""
        if rel_year.isdigit() and date[:4].isdigit():
            age = int(date[:4]) - int(rel_year)

        duration = ""
        duration_sec = None
        if r["raw_json"]:
            try:
                ms = json.loads(r["raw_json"]).get("trackTimeMillis")
                if ms:
                    duration_sec = int(ms) // 1000
                    duration = f"{duration_sec // 60}:{duration_sec % 60:02d}"
            except (ValueError, TypeError):
                pass

        art = (r["artwork_url"] or "").replace("100x100bb", "600x600bb")

        out.append({
            # stable keys for downstream sinks (not part of _JOURNAL_COLS)
            "post_id": r["post_id"], "message_id": r["message_id"],
            "posted_at": r["posted_at"],
            "date": date, "time": t[:8], "day": day,
            "month": f"{date[5:7]}_{date[:4]}" if len(date) >= 7 else "unknown",
            "author": r["author"],
            "track_name": r["track_name"] or "",
            "artist_name": r["artist_name"] or "",
            "album": r["collection_name"] or "",
            "genre": r["primary_genre"] or "", "duration": duration,
            "duration_sec": duration_sec,
            "release_date": rel, "release_year": rel_year, "age_yrs": age,
            "service": r["service"], "match_type": r["match_type"] or "",
            "resolved": "yes" if r["song_id"] else "no",
            "author_id": r["author_id"],
            "url": r["url"], "apple_music_url": r["track_view_url"] or "",
            "preview_url": r["preview_url"] or "", "artwork_url": art,
            "jump_url": r["jump_url"],
        })
    return out


def export_journal(channel_id, fmt: str = "xlsx") -> Path:
    """Write the channel's enriched journal to SOTD_Export_MMDDYYYY.<fmt>.

    csv/tsv → one flat sheet, oldest→newest. xlsx → an 'All' tab plus one tab
    per month (MM_YYYY), styled for readability. Returns the path written.
    """
    fmt = fmt.lower()
    if fmt not in ("csv", "tsv", "xlsx"):
        raise ValueError(f"unknown format {fmt!r} (use csv, tsv, or xlsx)")

    rows = _journal_rows(channel_id)
    EXPORTS_DIR.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%m%d%Y")
    path = EXPORTS_DIR / f"SOTD_Export_{stamp}.{fmt}"

    if fmt in ("csv", "tsv"):
        with path.open("w", newline="", encoding="utf-8") as f:
            w = csv.writer(f, delimiter="\t" if fmt == "tsv" else ",")
            w.writerow([h for _, h in _JOURNAL_COLS])
            for r in rows:
                w.writerow([r[k] for k, _ in _JOURNAL_COLS])
        return path

    _write_workbook(path, rows)
    return path


# Fields safe to publish on a PUBLIC website: display data only. The Discord
# message_id / author_id / jump_url from the journal are deliberately omitted so
# songs.json never carries private server, user, or message ids. `author` (the
# poster's display name) stays because the cards show it.
_WEB_FIELDS = (
    "post_id", "posted_at", "date", "day", "month",
    "track_name", "artist_name", "album", "genre", "duration", "duration_sec",
    "release_date", "release_year", "age_yrs",
    "service", "resolved", "author",
    "url", "apple_music_url", "preview_url", "artwork_url",
)


def _int_or_none(value):
    """Coerce a possibly-empty string to int, or None — so the JSON carries real
    numbers the browser can sort/range-filter without parsing."""
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def export_web(channel_id, out_path: Optional[Path] = None,
               label: Optional[str] = None) -> Path:
    """Write the journal as a website-ready songs.json (display data only).

    Shape: ``{"generated_at", "channel", "count", "songs": [...]}`` — one flat,
    JSON-typed object per song. This is the bridge to the deets.solutions SOTD
    page: it fetches this file and renders a card grid, and can filter/sort/search
    the array client-side. So the export is shaped for that: numbers come out as
    real numbers (or null), private Discord ids are dropped, and rows stay
    oldest->newest (the site reverses for display). Returns the path written.

    out_path lets a caller drop the file straight into the site repo
    (…/deetssolutions/sotd/songs.json); default is exports/songs.json.
    """
    rows = _journal_rows(channel_id)
    songs = []
    for r in rows:
        song = {k: r.get(k) for k in _WEB_FIELDS}
        song["release_year"] = _int_or_none(r["release_year"])
        song["age_yrs"] = _int_or_none(r["age_yrs"])
        songs.append(song)

    payload = {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "channel": label or str(channel_id),
        "count": len(songs),
        "songs": songs,
    }

    if out_path is None:
        EXPORTS_DIR.mkdir(parents=True, exist_ok=True)
        out_path = EXPORTS_DIR / "songs.json"
    else:
        out_path = Path(out_path)
        out_path.parent.mkdir(parents=True, exist_ok=True)

    with out_path.open("w", encoding="utf-8") as f:
        json.dump(payload, f, ensure_ascii=False, indent=2)
    return out_path


def _write_workbook(path: Path, rows: list[dict]) -> None:
    """Render the journal as a styled multi-tab .xlsx (All + one tab per month)."""
    from openpyxl import Workbook
    from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
    from openpyxl.utils import get_column_letter

    headers = [h for _, h in _JOURNAL_COLS]
    keys = [k for k, _ in _JOURNAL_COLS]

    widths = {
        "date": 11, "time": 9, "day": 6, "month": 9, "author": 16,
        "track": 30, "artist": 22, "album": 26, "genre": 14, "duration": 9,
        "released": 11, "rel_year": 9, "age_yrs": 8, "service": 12,
        "match": 8, "resolved": 9, "author_id": 20, "source_url": 9,
        "apple_music_url": 9, "preview_url": 9, "artwork_url": 9, "jump_url": 9,
    }
    centered = {"time", "day", "month", "duration", "released", "rel_year",
                "age_yrs", "service", "match", "resolved", "source_url",
                "apple_music_url", "preview_url", "artwork_url", "jump_url"}

    header_font = Font(bold=True, color="FFFFFF", size=11)
    header_fill = PatternFill("solid", fgColor="4F46E5")
    link_font = Font(color="2563EB", underline="single")
    stripe = PatternFill("solid", fgColor="F1F5F9")
    border = Border(bottom=Side(style="thin", color="E2E8F0"))
    center = Alignment(horizontal="center", vertical="center")
    left = Alignment(horizontal="left", vertical="center")

    # group rows by month; rows arrive oldest→newest so order is chronological
    months: "OrderedDict[str, list]" = OrderedDict()
    for r in rows:
        months.setdefault(r["month"], []).append(r)

    def fill_sheet(ws, sheet_rows):
        ws.append(headers)
        for cell in ws[1]:
            cell.font = header_font
            cell.fill = header_fill
            cell.alignment = center
            cell.border = border
        for ri, r in enumerate(sheet_rows, start=2):
            shade = ri % 2 == 0
            for ci, (k, h) in enumerate(_JOURNAL_COLS, start=1):
                cell = ws.cell(row=ri, column=ci)
                if k in _LINK_LABELS and r[k]:
                    cell.value = _LINK_LABELS[k]
                    cell.hyperlink = r[k]
                    cell.font = link_font
                    cell.alignment = center
                else:
                    cell.value = r[k]
                    cell.alignment = center if h in centered else left
                if shade:
                    cell.fill = stripe
        for ci, (_, h) in enumerate(_JOURNAL_COLS, start=1):
            ws.column_dimensions[get_column_letter(ci)].width = widths.get(h, 14)
        ws.freeze_panes = "A2"
        ws.auto_filter.ref = ws.dimensions
        ws.sheet_view.showGridLines = False

    wb = Workbook()
    wb.remove(wb.active)
    fill_sheet(wb.create_sheet("All"), rows)
    for month, mrows in months.items():
        fill_sheet(wb.create_sheet(month), mrows)
    wb.save(path)


def purge_non_music(channel_id=None) -> list[sqlite3.Row]:
    """Delete journal posts whose URL is a non-music host. Returns the deleted rows."""
    c = _db()
    q, args = "SELECT post_id, author, url FROM posts", ()
    if channel_id is not None:
        q += " WHERE channel_id = ?"
        args = (str(channel_id),)
    victims = [r for r in c.execute(q, args).fetchall() if is_non_music(r["url"])]
    for r in victims:
        c.execute("DELETE FROM posts WHERE post_id = ?", (r["post_id"],))
    return victims


def delete_posts(post_ids) -> int:
    """Delete specific posts by id (e.g. confirmed non-song meme links)."""
    c = _db()
    return sum(c.execute("DELETE FROM posts WHERE post_id = ?", (pid,)).rowcount
               for pid in post_ids)


def prune_orphan_songs() -> int:
    """Delete catalog songs no post points at anymore. Returns the count removed."""
    return _db().execute(
        "DELETE FROM songs WHERE song_id NOT IN "
        "(SELECT song_id FROM posts WHERE song_id IS NOT NULL)").rowcount


def close() -> None:
    global _conn
    if _conn is not None:
        _conn.close()
        _conn = None
