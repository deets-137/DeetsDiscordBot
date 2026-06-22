"""
One-off CLI scan. Logs in, fetches a single channel's history, writes
Markdown + CSV under exports/, logs out.

Usage:
  python scan.py                 # uses DEFAULT_CHANNEL_ID from config.py
  python scan.py <channel_id>    # scan a specific channel
  python scan.py <channel_id> --limit 500

A bot token can read message history over the gateway without staying online, so
this connects, does its work in on_ready, and closes immediately.
"""

import argparse
import asyncio
import os
import sys

import discord
from dotenv import load_dotenv

from linkscan import collect_links, write_outputs

load_dotenv()

try:
    import config
except ModuleNotFoundError:
    print("No config.py — copy config.example.py to config.py first.", file=sys.stderr)
    sys.exit(1)


async def scan_channel(channel_id: int, limit: int | None = None):
    """Connect, collect links from one channel, write files. Returns paths+count."""
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
        finally:
            await client.close()

    await client.start(token)
    return result


def main():
    ap = argparse.ArgumentParser(description="Scan a Discord channel for hyperlinks.")
    ap.add_argument("channel_id", nargs="?", type=int, default=config.DEFAULT_CHANNEL_ID)
    ap.add_argument("--limit", type=int, default=None,
                    help="max messages to scan (default: all)")
    args = ap.parse_args()

    if not args.channel_id:
        ap.error("no channel id given and DEFAULT_CHANNEL_ID is 0 in config.py")

    result = asyncio.run(scan_channel(args.channel_id, args.limit))
    print(f"#{result['label']}: {result['count']} links")
    print(f"  {result['md']}")
    print(f"  {result['csv']}")


if __name__ == "__main__":
    main()
