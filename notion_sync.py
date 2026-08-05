"""Mirror the SongOTD journal from storage.db into a Notion database.

storage.db stays the source of truth; this pushes a read-friendly copy up to a
Notion database (one page per post, album art as the page cover so the gallery
view shows covers). It is idempotent: every post carries its stable `post_id`
in a hidden PostID property, so re-running updates existing pages instead of
creating duplicates.

Setup (one-time):
  1. Create an *internal* integration at https://www.notion.so/profile/integrations
     and copy its "Internal Integration Secret".
  2. Open the DeetsOTD page in Notion -> ... -> Connections -> add that
     integration (this is what grants the script access).
  3. Put the secret + parent page id in .env:
        NOTION_TOKEN=secret_xxx
        NOTION_PARENT_PAGE_ID=your_parent_page_id_here
     On the first run the script creates the database and prints its id; paste
     that back into .env as NOTION_DB_ID so later runs reuse it.

Usage:
  python notion_sync.py                       # uses DEFAULT_CHANNEL_ID
  python notion_sync.py YOUR_CHANNEL_ID       # a specific channel
"""

from __future__ import annotations

import os
import sys
import time

import requests
from dotenv import load_dotenv

import storage

load_dotenv()

API = "https://api.notion.com/v1"
NOTION_VERSION = "2022-06-28"
WRITE_DELAY = 0.34  # ~3 req/s, Notion's published rate limit

# Notion database schema. Title must exist; PostID is our idempotency key.
SCHEMA = {
    "Track": {"title": {}},
    "Cover": {"files": {}},
    "Artist": {"rich_text": {}},
    "Album": {"rich_text": {}},
    "Genre": {"select": {}},
    "Poster": {"select": {}},
    "Posted": {"date": {}},
    "Day": {"select": {}},
    "Month": {"select": {}},
    "Duration": {"rich_text": {}},
    "Released": {"date": {}},
    "Release Year": {"number": {}},
    "Age (yrs)": {"number": {}},
    "Service": {"select": {}},
    "Match": {"select": {}},
    "Resolved": {"checkbox": {}},
    "Discord User ID": {"rich_text": {}},
    "Source URL": {"url": {}},
    "Apple Music": {"url": {}},
    "Preview": {"url": {}},
    "Artwork": {"url": {}},
    "Jump": {"url": {}},
    "PostID": {"number": {}},
}


def _headers() -> dict:
    token = os.getenv("NOTION_TOKEN")
    if not token:
        sys.exit("NOTION_TOKEN missing from .env (see notion_sync.py docstring).")
    return {
        "Authorization": f"Bearer {token}",
        "Notion-Version": NOTION_VERSION,
        "Content-Type": "application/json",
    }


def _post(path: str, payload: dict) -> dict:
    r = requests.post(f"{API}/{path}", headers=_headers(), json=payload, timeout=30)
    if not r.ok:
        sys.exit(f"Notion API error {r.status_code} on {path}: {r.text}")
    return r.json()


def _patch(path: str, payload: dict) -> dict:
    r = requests.patch(f"{API}/{path}", headers=_headers(), json=payload, timeout=30)
    if not r.ok:
        sys.exit(f"Notion API error {r.status_code} on {path}: {r.text}")
    return r.json()


def ensure_database() -> str:
    """Return the target database id, creating it under the parent page if needed."""
    db_id = os.getenv("NOTION_DB_ID")
    if db_id:
        return db_id

    parent = os.getenv("NOTION_PARENT_PAGE_ID")
    if not parent:
        sys.exit("Set NOTION_DB_ID (existing db) or NOTION_PARENT_PAGE_ID "
                 "(to create one) in .env.")

    db = _post("databases", {
        "parent": {"type": "page_id", "page_id": parent},
        "title": [{"type": "text", "text": {"content": "SongOTD Journal"}}],
        "properties": SCHEMA,
    })
    db_id = db["id"]
    print(f"Created Notion database {db_id}")
    print("  -> add this to .env as NOTION_DB_ID to reuse it next time:")
    print(f"     NOTION_DB_ID={db_id}")
    return db_id


def fetch_existing(db_id: str) -> dict:
    """Map every PostID already in the database to its Notion page id."""
    out, cursor = {}, None
    while True:
        payload = {"page_size": 100}
        if cursor:
            payload["start_cursor"] = cursor
        page = _post(f"databases/{db_id}/query", payload)
        for row in page["results"]:
            pid = row["properties"].get("PostID", {}).get("number")
            if pid is not None:
                out[pid] = row["id"]
        if not page.get("has_more"):
            return out
        cursor = page["next_cursor"]


def _text(value: str) -> list:
    return [{"type": "text", "text": {"content": (value or "")[:2000]}}]


def _select(value: str):
    # Notion select option names may not contain commas.
    value = (value or "").replace(",", "/").strip()
    return {"select": {"name": value[:100]}} if value else {"select": None}


def _num(value):
    try:
        return {"number": int(value)}
    except (TypeError, ValueError):
        return {"number": None}


def _cover_files(url: str) -> dict:
    """A Files & media value from an external image URL (renders as a thumbnail
    and can be used as the gallery card image)."""
    if not url:
        return {"files": []}
    return {"files": [{"type": "external", "name": "cover.jpg",
                       "external": {"url": url}}]}


def build_properties(row: dict) -> dict:
    return {
        "Track": {"title": _text(row["track_name"] or "(unresolved)")},
        "Cover": _cover_files(row["artwork_url"]),
        "Artist": {"rich_text": _text(row["artist_name"])},
        "Album": {"rich_text": _text(row["album"])},
        "Genre": _select(row["genre"]),
        "Poster": _select(row["author"]),
        "Posted": {"date": {"start": row["posted_at"]} if row["posted_at"] else None},
        "Day": _select(row["day"]),
        "Month": _select(row["month"]),
        "Duration": {"rich_text": _text(row["duration"])},
        "Released": {"date": {"start": row["release_date"]} if row["release_date"] else None},
        "Release Year": _num(row["release_year"]),
        "Age (yrs)": _num(row["age_yrs"]),
        "Service": _select(row["service"]),
        "Match": _select(row["match_type"]),
        "Resolved": {"checkbox": row["resolved"] == "yes"},
        "Discord User ID": {"rich_text": _text(row["author_id"])},
        "Source URL": {"url": row["url"] or None},
        "Apple Music": {"url": row["apple_music_url"] or None},
        "Preview": {"url": row["preview_url"] or None},
        "Artwork": {"url": row["artwork_url"] or None},
        "Jump": {"url": row["jump_url"] or None},
        "PostID": {"number": row["post_id"]},
    }


def sync(channel_id) -> dict:
    """Upsert every post for the channel into Notion. Returns counts + db id."""
    db_id = ensure_database()
    existing = fetch_existing(db_id)
    rows = storage._journal_rows(channel_id)

    created = updated = 0
    for row in rows:
        props = build_properties(row)
        cover = ({"type": "external", "external": {"url": row["artwork_url"]}}
                 if row["artwork_url"] else None)
        page_id = existing.get(row["post_id"])
        if page_id:
            _patch(f"pages/{page_id}", {"properties": props, "cover": cover})
            updated += 1
        else:
            _post("pages", {
                "parent": {"database_id": db_id},
                "properties": props,
                "cover": cover,
            })
            created += 1
        time.sleep(WRITE_DELAY)

    # Mirror deletions: archive Notion pages whose post no longer exists (e.g.
    # purged gif/meme links), so Notion stays a faithful copy of storage.db.
    live = {row["post_id"] for row in rows}
    archived = 0
    for post_id, page_id in existing.items():
        if post_id not in live:
            _patch(f"pages/{page_id}", {"archived": True})
            archived += 1
            time.sleep(WRITE_DELAY)

    return {"created": created, "updated": updated,
            "archived": archived, "db_id": db_id}


def main():
    try:
        from config import DEFAULT_CHANNEL_ID
    except ModuleNotFoundError:
        DEFAULT_CHANNEL_ID = 0
    channel_id = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_CHANNEL_ID
    if not channel_id:
        sys.exit("no channel id given and DEFAULT_CHANNEL_ID is 0 in config.py")

    result = sync(channel_id)
    print(f"Notion sync: {result['created']} created, {result['updated']} updated, "
          f"{result['archived']} archived (db {result['db_id']}).")


if __name__ == "__main__":
    main()
