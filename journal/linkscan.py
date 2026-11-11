"""
Core link-extraction logic for DeetsOTD.

Shared by every entry point (the Discord slash command, the one-off CLI, and the
device-side harness). Nothing in here knows *how* a scan was triggered — it just
walks a channel's history, pulls out every hyperlink it can find, and writes the
results to Markdown + CSV.

A "link" can come from three places in a Discord message:
  - text      : a raw URL or a markdown masked link [text](url) in message.content
  - embed     : the .url / image / thumbnail of an auto-generated or rich embed
  - attachment: an uploaded file's CDN url

Each is recorded with author, timestamp, a jump_url back to the message, and (for
masked markdown links) the human link text.
"""

from __future__ import annotations

import csv
import re
from collections import OrderedDict
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

from .paths import EXPORTS_DIR

# ─── URL patterns ────────────────────────────────────────────────────────────

# Markdown masked link: [visible text](https://url) — captured first so we can
# keep the link text and avoid double-counting the bare url inside it.
_MASKED_RE = re.compile(r"\[([^\]]+)\]\((https?://[^\s)]+)\)")

# Bare URLs. Trailing punctuation that's almost never part of a url is trimmed
# afterwards in _clean(). Angle-bracket <url> (Discord's "no embed" form) works
# because the leading < / trailing > are not in the allowed char class.
_BARE_RE = re.compile(r"https?://[^\s<>()\[\]]+")

_TRAILING = ".,;:!?\"'"

# Map a URL's host to a streaming service. Order doesn't matter; first host
# substring that matches wins. Anything unrecognised is "other".
_SERVICES = {
    "music.apple.com": "apple_music",
    "itunes.apple.com": "apple_music",
    "open.spotify.com": "spotify",
    "spotify.com": "spotify",
    "music.youtube.com": "youtube_music",
    "youtube.com": "youtube",
    "youtu.be": "youtube",
    "soundcloud.com": "soundcloud",
    "bandcamp.com": "bandcamp",
    "tidal.com": "tidal",
    "deezer.com": "deezer",
    "music.amazon.com": "amazon_music",
}


# Hosts that are never songs — Discord's gif keyboard, image hosts, CDNs. Links
# to these are noise in a song journal and are kept out of the `posts` table.
_NON_MUSIC_HOSTS = {
    "tenor.com", "media.tenor.com", "c.tenor.com",
    "giphy.com", "media.giphy.com", "gph.is",
    "imgur.com", "i.imgur.com",
    "media.discordapp.net", "cdn.discordapp.com",
    "twitter.com", "x.com", "fxtwitter.com", "vxtwitter.com",
}


def _host(url: str) -> str:
    host = re.sub(r"^https?://", "", url).split("/", 1)[0].lower()
    return host.split("@")[-1].split(":")[0]  # strip any creds / port


def detect_service(url: str) -> str:
    """Classify a URL by streaming service from its host (e.g. 'apple_music')."""
    host = _host(url)
    for needle, name in _SERVICES.items():
        if host == needle or host.endswith("." + needle) or host == needle.split(".", 1)[-1]:
            return name
    # Fall back to a substring check so e.g. music.apple.com.edgekey hosts still map.
    for needle, name in _SERVICES.items():
        if needle in host:
            return name
    return "other"


def is_non_music(url: str) -> bool:
    """True for hosts that never carry songs (gif keyboards, image CDNs, …)."""
    host = _host(url)
    return any(host == h or host.endswith("." + h) for h in _NON_MUSIC_HOSTS)


def _clean(url: str) -> str:
    """Strip trailing punctuation that got swept up by the bare-url regex."""
    while url and url[-1] in _TRAILING:
        url = url[:-1]
    return url


# ─── Record ──────────────────────────────────────────────────────────────────

@dataclass
class LinkRecord:
    url: str
    source: str               # "text" | "embed" | "attachment"
    author: str
    author_id: int
    timestamp: str            # ISO 8601, UTC
    message_id: int
    jump_url: str
    link_text: str = ""       # only set for markdown masked links

    def as_row(self) -> dict:
        return {
            "url": self.url,
            "source": self.source,
            "link_text": self.link_text,
            "author": self.author,
            "timestamp": self.timestamp,
            "jump_url": self.jump_url,
            "message_id": self.message_id,
        }


# ─── Extraction ──────────────────────────────────────────────────────────────

def extract_from_message(message) -> list[LinkRecord]:
    """Pull every link out of a single discord.Message."""
    out: list[LinkRecord] = []
    author = getattr(message.author, "display_name", str(message.author))
    author_id = getattr(message.author, "id", 0)
    ts = message.created_at
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    ts_iso = ts.astimezone(timezone.utc).isoformat()

    def rec(url, source, link_text=""):
        return LinkRecord(
            url=_clean(url),
            source=source,
            author=author,
            author_id=author_id,
            timestamp=ts_iso,
            message_id=message.id,
            jump_url=message.jump_url,
            link_text=link_text,
        )

    content = message.content or ""

    # Masked markdown links first; remember their urls so the bare pass skips them.
    masked_urls: set[str] = set()
    for text, url in _MASKED_RE.findall(content):
        out.append(rec(url, "text", link_text=text.strip()))
        masked_urls.add(_clean(url))

    # Bare urls in the text body.
    for url in _BARE_RE.findall(content):
        if _clean(url) not in masked_urls:
            out.append(rec(url, "text"))

    # Embeds (link previews, rich embeds).
    for embed in message.embeds:
        for candidate in (embed.url,
                          getattr(embed.image, "url", None),
                          getattr(embed.thumbnail, "url", None)):
            if candidate:
                out.append(rec(candidate, "embed", link_text=embed.title or ""))

    # Uploaded attachments.
    for att in message.attachments:
        out.append(rec(att.url, "attachment", link_text=att.filename))

    return out


async def collect_links(channel, limit: int | None = None, newest: bool = False) -> list[LinkRecord]:
    """Walk a channel's history and return all link records, oldest→newest.

    With a `limit`, which messages count depends on `newest`:
    - False (the journal's scan): the channel's FIRST `limit` messages.
    - True (`/links`, the owner, 2026-11-10): the LAST `limit` messages, still listed
      oldest→newest.
    """
    records: list[LinkRecord] = []
    if newest and limit is not None:
        recent = [m async for m in channel.history(limit=limit)]  # newest first
        for message in reversed(recent):
            records.extend(extract_from_message(message))
        return records
    async for message in channel.history(limit=limit, oldest_first=True):
        records.extend(extract_from_message(message))
    return records


# ─── Output ──────────────────────────────────────────────────────────────────

def _stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")


def write_outputs(records: list[LinkRecord], label: str) -> tuple[Path, Path]:
    """Write records to Markdown + CSV under EXPORTS_DIR. Returns (md, csv)."""
    EXPORTS_DIR.mkdir(parents=True, exist_ok=True)
    safe = re.sub(r"[^A-Za-z0-9_-]+", "-", label).strip("-") or "channel"
    base = f"links_{safe}_{_stamp()}"
    md_path = EXPORTS_DIR / f"{base}.md"
    csv_path = EXPORTS_DIR / f"{base}.csv"

    # Markdown — grouped, newest-first, deduped by url (keeping first seen).
    seen: dict[str, LinkRecord] = {}
    for r in records:
        seen.setdefault(r.url, r)
    unique = list(seen.values())

    lines = [
        f"# Links from #{label}",
        "",
        f"_Scanned {datetime.now(timezone.utc).isoformat()} — "
        f"{len(records)} total, {len(unique)} unique._",
        "",
    ]
    for r in unique:
        display = r.link_text or r.url
        lines.append(f"- [{display}]({r.url})")
        lines.append(f"  - {r.author} · {r.timestamp[:10]} · "
                     f"[jump]({r.jump_url}) · `{r.source}`")
    md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    # CSV — every record, no dedupe, for spreadsheet work.
    with csv_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(
            f, fieldnames=["url", "source", "link_text", "author",
                           "timestamp", "jump_url", "message_id"])
        writer.writeheader()
        for r in records:
            writer.writerow(r.as_row())

    return md_path, csv_path


# ─── Per-user output ─────────────────────────────────────────────────────────

def group_by_user(records: list[LinkRecord]) -> "OrderedDict[int, list[LinkRecord]]":
    """Group records by author, deduping each user's repeated urls (earliest kept).

    Records arrive oldest→newest, so the first time we see a (user, url) pair is
    the first time that user sent it — which is what "what they've sent" means.
    Users are ordered by their most recent activity (most active-recently first).
    """
    by_user: "OrderedDict[int, list[LinkRecord]]" = OrderedDict()
    seen: set[tuple[int, str]] = set()
    for r in records:
        key = (r.author_id, r.url)
        if key in seen:
            continue
        seen.add(key)
        by_user.setdefault(r.author_id, []).append(r)
    # Order users by their latest link's timestamp, newest first.
    return OrderedDict(
        sorted(by_user.items(),
               key=lambda kv: kv[1][-1].timestamp, reverse=True))


def write_by_user(records: list[LinkRecord], label: str, fmt: str = "csv") -> Path:
    """Write a per-user breakdown of links to EXPORTS_DIR. Returns the path.

    fmt is one of "csv", "tsv", "xml". CSV/TSV are flat tables sorted so each
    user's links sit together (author, url, …); XML nests links under each user.
    """
    fmt = fmt.lower()
    if fmt not in ("csv", "tsv", "xml"):
        raise ValueError(f"unknown format {fmt!r} (use csv, tsv, or xml)")

    EXPORTS_DIR.mkdir(parents=True, exist_ok=True)
    safe = re.sub(r"[^A-Za-z0-9_-]+", "-", label).strip("-") or "channel"
    path = EXPORTS_DIR / f"by_user_{safe}_{_stamp()}.{fmt}"
    grouped = group_by_user(records)

    if fmt in ("csv", "tsv"):
        delim = "\t" if fmt == "tsv" else ","
        with path.open("w", newline="", encoding="utf-8") as f:
            writer = csv.writer(f, delimiter=delim)
            writer.writerow(["author", "author_id", "link_count",
                             "url", "link_text", "source", "timestamp", "jump_url"])
            for links in grouped.values():
                count = len(links)
                for r in links:
                    writer.writerow([r.author, r.author_id, count, r.url,
                                     r.link_text, r.source, r.timestamp, r.jump_url])
        return path

    # XML — one <user> element per author, links nested inside.
    scanned = datetime.now(timezone.utc).isoformat()
    lines = ['<?xml version="1.0" encoding="UTF-8"?>',
             f'<links channel={quoteattr(label)} scanned={quoteattr(scanned)} '
             f'users="{len(grouped)}" total="{len(records)}">']
    for links in grouped.values():
        head = links[0]
        lines.append(f'  <user name={quoteattr(head.author)} '
                     f'id="{head.author_id}" count="{len(links)}">')
        for r in links:
            lines.append(
                f'    <link url={quoteattr(r.url)} source={quoteattr(r.source)} '
                f'timestamp={quoteattr(r.timestamp)} jump={quoteattr(r.jump_url)}>'
                f'{escape(r.link_text)}</link>')
        lines.append('  </user>')
    lines.append('</links>')
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path
