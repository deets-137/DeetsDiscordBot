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
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

from paths import EXPORTS_DIR

# ─── URL patterns ────────────────────────────────────────────────────────────

# Markdown masked link: [visible text](https://url) — captured first so we can
# keep the link text and avoid double-counting the bare url inside it.
_MASKED_RE = re.compile(r"\[([^\]]+)\]\((https?://[^\s)]+)\)")

# Bare URLs. Trailing punctuation that's almost never part of a url is trimmed
# afterwards in _clean(). Angle-bracket <url> (Discord's "no embed" form) works
# because the leading < / trailing > are not in the allowed char class.
_BARE_RE = re.compile(r"https?://[^\s<>()\[\]]+")

_TRAILING = ".,;:!?\"'"


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


async def collect_links(channel, limit: int | None = None) -> list[LinkRecord]:
    """Walk a channel's history (oldest→newest) and return all link records."""
    records: list[LinkRecord] = []
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
