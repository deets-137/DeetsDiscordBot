"""
Hand-verify the songs the enricher couldn't safely resolve.

Some Apple Music posts point at *delisted* track ids: the id 404s, and the only
fallback — a title search — confidently returns the wrong thing (a live cut, a
different artist's song) with no signal to catch it automatically. enrich.py tags
those guesses match_type='review' instead of trusting them. This walks each one,
shows you the original Discord post, and lets you set it right by hand.

Per song you can:
  [u] paste the correct Apple Music URL  → re-resolves via the API, tags 'manual'
  [a] accept the current guess as correct → tags 'manual'
  [s] skip (leave flagged for later)
  [q] quit

storage.db stays the source of truth; after fixing, re-run notion_sync.py to push
the corrections to Notion.

    python review.py [channel_id]
    python review.py --list      # just print what's flagged, no prompts
"""

from __future__ import annotations

import argparse
import sys

import applemusic
import config
import enrich
import storage


def _fmt_guess(row) -> str:
    bits = [row["track_name"] or "?", "—", row["artist_name"] or "?"]
    if row["collection_name"]:
        bits.append(f"[{row['collection_name']}]")
    return " ".join(bits)


def _group_by_song(rows) -> "dict[int, list]":
    grouped: "dict[int, list]" = {}
    for r in rows:
        grouped.setdefault(r["song_id"], []).append(r)
    return grouped


def _resolve_apple_url(url: str):
    """Resolve a pasted Apple Music URL to an iTunes-shaped track, or None."""
    tid = enrich._apple_track_id(url)
    if not tid:
        return None
    song = applemusic.song_by_id(tid)
    return applemusic.to_itunes_shape(song) if song else None


def _apply_correction(song_id, posts, track) -> None:
    """Point every post at the corrected track and tag it hand-verified."""
    new_id = storage.upsert_song(track, "manual")
    for p in posts:
        storage.link_post_to_song(p["post_id"], new_id)


def run(channel_id, list_only: bool) -> int:
    rows = storage.songs_needing_review(channel_id)
    groups = _group_by_song(rows)
    if not groups:
        print("Nothing flagged for review — every song is resolved or confirmed.")
        return 0

    print(f"{len(groups)} song(s) flagged for hand-verification.\n")
    if list_only:
        for sid, posts in groups.items():
            p = posts[0]
            print(f"  • {_fmt_guess(p)}")
            print(f"    posted by {p['author']} on {p['posted_at'][:10]} "
                  f"via {p['service']}")
            print(f"    source : {p['url']}")
            print(f"    discord: {p['jump_url']}\n")
        return 0

    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        print("Not a TTY — run interactively, or use --list to view.", file=sys.stderr)
        return 2

    fixed = 0
    for n, (sid, posts) in enumerate(groups.items(), 1):
        p = posts[0]
        print(f"── {n}/{len(groups)} ───────────────────────────────────────────")
        print(f"  posted by {p['author']} on {p['posted_at'][:10]} via {p['service']}")
        print(f"  source : {p['url']}")
        print(f"  discord: {p['jump_url']}")
        if len(posts) > 1:
            print(f"  (also posted {len(posts) - 1}x more — all will be updated)")
        print(f"  guess  : {_fmt_guess(p)}")
        if p["track_view_url"]:
            print(f"           {p['track_view_url']}")

        while True:
            try:
                choice = input("  [u]rl / [a]ccept / [s]kip / [q]uit > ").strip().lower()
            except (EOFError, KeyboardInterrupt):
                print("\nStopped.")
                choice = "q"

            if choice in ("q", "quit"):
                _finish(fixed)
                return 0
            if choice in ("s", "skip", ""):
                print()
                break
            if choice in ("a", "accept"):
                storage.set_song_match_type(sid, "manual")
                fixed += 1
                print("  ✓ accepted (manual)\n")
                break
            if choice in ("u", "url"):
                url = input("  paste correct Apple Music URL > ").strip()
                if not url:
                    continue
                track = _resolve_apple_url(url)
                if not track:
                    print("  ✗ couldn't resolve that URL — check the link and retry.")
                    continue
                _apply_correction(sid, posts, track)
                fixed += 1
                print(f"  ✓ {track['trackName']} — {track['artistName']} (manual)\n")
                break
            print("  ? choose u, a, s, or q.")

    _finish(fixed)
    return 0


def _finish(fixed: int) -> None:
    pruned = storage.prune_orphan_songs()
    print(f"Done. {fixed} verified" + (f", {pruned} orphan song(s) pruned." if pruned
                                        else "."))
    if fixed:
        print("Re-run `python notion_sync.py <channel_id>` to push corrections to Notion.")


def main() -> None:
    ap = argparse.ArgumentParser(description="Hand-verify 'review'-flagged songs.")
    ap.add_argument("channel_id", nargs="?", type=int,
                    default=config.DEFAULT_CHANNEL_ID,
                    help="limit to one channel (default: config.DEFAULT_CHANNEL_ID)")
    ap.add_argument("--list", action="store_true",
                    help="just print what's flagged; don't prompt")
    args = ap.parse_args()
    channel_id = args.channel_id or None
    try:
        sys.exit(run(channel_id, args.list))
    finally:
        storage.close()


if __name__ == "__main__":
    main()
