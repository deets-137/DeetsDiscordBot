"""
One-off CLI scan. Logs in, fetches a single channel's history, writes
Markdown + CSV under exports/, logs out.

Usage:
  python scan.py                 # uses DEFAULT_CHANNEL_ID from config.py
  python scan.py <channel_id>    # scan a specific channel
  python scan.py <channel_id> --limit 500

  # Per-user breakdown of who sent which links (csv | tsv | xml):
  python scan.py <channel_id> --by-user
  python scan.py <channel_id> --by-user --format xml

  # Fold the scan into the persistent archive (storage.db):
  python scan.py <channel_id> --archive

A bot token can read message history over the gateway without staying online, so
this connects, does its work in on_ready, and closes immediately.
"""

import argparse
import asyncio
import os
import sys
import time

import discord
from dotenv import load_dotenv

import enrich
import storage
from linkscan import collect_links, group_by_user, write_by_user, write_outputs

load_dotenv()

try:
    import config
except ModuleNotFoundError:
    print("No config.py — copy config.example.py to config.py first.", file=sys.stderr)
    sys.exit(1)


async def scan_channel(channel_id: int, limit: int | None = None,
                       by_user: bool = False, fmt: str = "csv",
                       archive: bool = False, do_enrich: bool = False,
                       export: str | None = None, notion: bool = False,
                       web: bool = False, web_out: str | None = None):
    """Connect, collect links from one channel, write files. Returns paths+count.

    When by_user is True, also writes a per-user breakdown in fmt (csv/tsv/xml).
    When archive is True, folds the scan into storage.db (the journal). When
    do_enrich is True, also resolves any un-enriched posts against iTunes.
    """
    token = os.environ.get("DISCORD_TOKEN", "")
    if not token:
        raise RuntimeError("DISCORD_TOKEN not set (put it in .env).")

    intents = discord.Intents.default()
    intents.message_content = True

    client = discord.Client(intents=intents)
    result: dict = {}

    @client.event
    async def on_ready():
        try:
            channel = client.get_channel(channel_id) or await client.fetch_channel(channel_id)
            records = await collect_links(channel, limit=limit)
            label = getattr(channel, "name", str(channel_id))
            md, csv = write_outputs(records, label)
            result.update(md=md, csv=csv, count=len(records), label=label)
            if by_user:
                result["by_user"] = write_by_user(records, label, fmt)
                result["per_user"] = {
                    links[0].author: len(links)
                    for links in group_by_user(records).values()
                }
            if archive or do_enrich or export or notion or web:
                result["archive"] = storage.record_scan(channel_id, label, records, limit)
            if do_enrich:
                result["enriched"] = enrich_pending(channel_id)
            if export:
                result["export"] = storage.export_journal(channel_id, export)
            if notion:
                import notion_sync
                result["notion"] = notion_sync.sync(channel_id)
            if web:
                result["web"] = storage.export_web(channel_id, out_path=web_out,
                                                   label=label)
        finally:
            await client.close()

    await client.start(token)
    return result


def enrich_pending(channel_id=None, delay: float = 1.0) -> dict:
    """Resolve every un-enriched post in the journal against iTunes. Returns counts.

    Sleeps `delay` seconds between calls to stay under iTunes' rate limit
    (~20/min). Songs already resolved (shared by an earlier post) are reused
    without a network call, so re-posts cost nothing.
    """
    pending = storage.posts_needing_enrichment(channel_id)
    resolved = 0
    review = 0
    for i, post in enumerate(pending):
        hit = enrich.enrich(post["url"])
        if hit:
            track, match_type = hit
            song_id = storage.upsert_song(track, match_type)
            storage.link_post_to_song(post["post_id"], song_id)
            resolved += 1
            if match_type == "review":
                review += 1
        if delay and i < len(pending) - 1:
            time.sleep(delay)
    return {"checked": len(pending), "resolved": resolved, "review": review}


def main():
    ap = argparse.ArgumentParser(description="Scan a Discord channel for hyperlinks.")
    ap.add_argument("channel_id", nargs="?", type=int, default=config.DEFAULT_CHANNEL_ID)
    ap.add_argument("--limit", type=int, default=None,
                    help="max messages to scan (default: all)")
    ap.add_argument("--by-user", action="store_true",
                    help="also write a per-user breakdown of who sent which links")
    ap.add_argument("--format", choices=["csv", "tsv", "xml"], default="csv",
                    help="format for the --by-user breakdown (default: csv)")
    ap.add_argument("--archive", action="store_true",
                    help="fold the scan into the persistent journal (storage.db)")
    ap.add_argument("--enrich", action="store_true",
                    help="resolve posted songs against iTunes (implies --archive)")
    ap.add_argument("--export", choices=["csv", "tsv", "xlsx"], default=None,
                    help="write the enriched journal from storage.db to "
                         "SOTD_Export_MMDDYYYY.{csv,tsv,xlsx} (implies --archive; "
                         "xlsx gets one tab per month)")
    ap.add_argument("--notion", action="store_true",
                    help="mirror the journal into the Notion database "
                         "(implies --archive; see notion_sync.py for setup)")
    ap.add_argument("--web", action="store_true",
                    help="write a website-ready songs.json for the deets.solutions "
                         "SOTD page (display data only, private Discord ids dropped; "
                         "implies --archive)")
    ap.add_argument("--web-out", metavar="PATH", default=None,
                    help="where --web writes songs.json (default: exports/songs.json). "
                         "Point at the site repo, e.g. "
                         "../deetssolutions/sotd/songs.json")
    args = ap.parse_args()

    if not args.channel_id:
        ap.error("no channel id given and DEFAULT_CHANNEL_ID is 0 in config.py")

    result = asyncio.run(scan_channel(
        args.channel_id, args.limit, by_user=args.by_user, fmt=args.format,
        archive=args.archive, do_enrich=args.enrich, export=args.export,
        notion=args.notion, web=args.web, web_out=args.web_out))
    print(f"#{result['label']}: {result['count']} links")
    print(f"  {result['md']}")
    print(f"  {result['csv']}")
    if args.by_user:
        print(f"  {result['by_user']}")
        print("\nLinks per user (unique urls):")
        for author, count in result["per_user"].items():
            print(f"  {author}: {count}")
    if "archive" in result:
        a = result["archive"]
        print(f"\nArchived to storage.db: {a['new']} new posts, "
              f"{a['known']} already on record (scan #{a['scan_id']}).")
    if "enriched" in result:
        e = result["enriched"]
        print(f"Enriched {e['resolved']}/{e['checked']} posts.")
        if e.get("review"):
            print(f"  {e['review']} need hand-verification — run `python review.py "
                  f"{args.channel_id}`.")
    if "export" in result:
        print(f"Exported journal: {result['export']}")
    if "notion" in result:
        n = result["notion"]
        print(f"Notion sync: {n['created']} created, {n['updated']} updated, "
              f"{n['archived']} archived.")
    if "web" in result:
        print(f"Wrote website JSON: {result['web']}")


if __name__ == "__main__":
    main()
