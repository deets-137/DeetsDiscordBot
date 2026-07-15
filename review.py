"""
Hand-verify and correct the songs the enricher got wrong.

Two failure modes, two entry points:

1. Songs the enricher *knew* it wasn't sure about. Some Apple Music posts point at
   *delisted* track ids: the id 404s, and the only fallback — a title search —
   confidently returns the wrong thing (a live cut, a different artist's song) with
   no signal to catch it automatically. enrich.py tags those guesses
   match_type='review'. `review.py review` walks each one, shows the original
   Discord post, and lets you set it right interactively:
     [u] paste the correct Apple Music URL  → re-resolves via the API, tags 'manual'
     [a] accept the current guess as correct → tags 'manual'
     [s] skip (leave flagged for later)   [q] quit

2. Songs that resolved *confidently but wrong* (tagged 'apple'/'search'/'spotify')
   — a Spotify/YouTube post that matched a cover, a live cut, or a wrong-artist
   same-title song. Nothing flagged these, so `review` never shows them. Find one
   by what's on the site and fix it by id, non-interactively:
     python review.py search "<track or artist>"      # → a song_id
     python review.py fix <song_id> --url <correct link>   # re-resolve, tag manual
     python review.py fix <song_id> --accept               # keep it, just tag manual
   The correct link can be Apple Music (most precise), or any Spotify/YouTube/etc.
   URL the enricher can resolve; either way the result is tagged 'manual'.

storage.db stays the source of truth; after fixing, regenerate songs.json
(scan.py --web) and commit it to the site to publish.

    python review.py                       # interactive review (default channel)
    python review.py review -c <channel>   # interactive review of one channel
    python review.py list                  # print what's flagged, no prompts
    python review.py search "<query>"      # find a song to correct by id
    python review.py fix <song_id> --url <link> | --accept
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
    """Resolve a pasted Apple Music URL to an iTunes-shaped track, or None.

    Prefers the authenticated Apple Music API (richer metadata) when credentials
    are configured, but falls back to the free, keyless iTunes id lookup — the same
    order enrich.py uses — so a correction still resolves without an Apple key."""
    tid = enrich._apple_track_id(url)
    if not tid:
        return None
    if enrich._apple_ready():
        song = applemusic.song_by_id(tid)
        if song:
            return applemusic.to_itunes_shape(song)
    return enrich._lookup(tid)


def _resolve_any(url: str):
    """Resolve a hand-supplied 'correct' URL to an iTunes-shaped track.

    Prefer a precise Apple Music id lookup; fall back to the full enricher, so a
    Spotify / YouTube / SoundCloud link the user pastes still works. Returns a
    track dict or None. The caller always tags the result 'manual' — a human
    vouched for this link, whatever service it came from."""
    track = _resolve_apple_url(url)
    if track:
        return track
    res = enrich.enrich(url)
    return res[0] if res else None


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
        print("Regenerate songs.json (scan.py --web) and commit it to the site to publish.")


def cmd_search(query: str, channel_id) -> int:
    """Print songs matching a substring so a wrong one can be picked by id."""
    rows = storage.find_songs(query, channel_id)
    if not rows:
        print(f"No songs match {query!r}.")
        return 0
    print(f"{len(rows)} song(s) matching {query!r}:\n")
    for r in rows:
        print(f"  #{r['song_id']:<5} [{(r['match_type'] or '?'):^6}]  {_fmt_guess(r)}")
        print(f"         {r['post_count']} post(s), first "
              f"{(r['first_posted'] or '')[:10]} · {r['jump_url']}")
        if r["track_view_url"]:
            print(f"         {r['track_view_url']}")
        print()
    print("Fix one with:  python review.py fix <song_id> --url <correct link>")
    return 0


def cmd_fix(song_id: int, url: str | None, accept: bool) -> int:
    """Correct one song by id: re-resolve to a supplied link, or accept as-is.
    Either way it's tagged 'manual' and every post pointing at it comes along."""
    before = storage.song_row(song_id)
    if not before:
        print(f"No song with id {song_id}.", file=sys.stderr)
        return 2
    posts = storage.posts_for_song(song_id)

    if accept:
        storage.set_song_match_type(song_id, "manual")
        print(f"✓ song #{song_id} accepted as-is: {_fmt_guess(before)} (manual)")
        return 0

    track = _resolve_any(url)
    if not track:
        print(f"✗ couldn't resolve that link — check it and retry: {url}",
              file=sys.stderr)
        return 1
    _apply_correction(song_id, posts, track)
    pruned = storage.prune_orphan_songs()
    print(f"✓ song #{song_id} corrected:")
    print(f"    was : {_fmt_guess(before)}")
    print(f"    now : {track.get('trackName') or '?'} — "
          f"{track.get('artistName') or '?'} (manual)")
    print(f"  re-pointed {len(posts)} post(s)"
          + (f", pruned {pruned} orphan song(s)" if pruned else "") + ".")
    print("Regenerate songs.json (scan.py --web) and commit it to the site to publish.")
    return 0


def main() -> None:
    # The output uses ✓/✗/─ etc.; Windows' default cp1252 console can't encode
    # them and would crash mid-command. Force UTF-8 where the stream supports it.
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass

    ap = argparse.ArgumentParser(
        description="Hand-verify and correct mis-resolved SongOTD entries.")
    sub = ap.add_subparsers(dest="cmd")

    def _channel_arg(p):
        p.add_argument("-c", "--channel", type=int, default=config.DEFAULT_CHANNEL_ID,
                       help="limit to one channel (default: config.DEFAULT_CHANNEL_ID)")

    p_review = sub.add_parser("review", help="interactively fix 'review'-flagged songs")
    _channel_arg(p_review)
    p_list = sub.add_parser("list", help="print flagged songs; don't prompt")
    _channel_arg(p_list)
    p_search = sub.add_parser("search", help="find a song by track/artist/album substring")
    p_search.add_argument("query")
    p_search.add_argument("-c", "--channel", type=int, default=None,
                          help="limit to one channel (default: all)")
    p_fix = sub.add_parser("fix", help="correct one song by id")
    p_fix.add_argument("song_id", type=int)
    g = p_fix.add_mutually_exclusive_group(required=True)
    g.add_argument("-u", "--url", help="the correct link (Apple Music, Spotify, YouTube…)")
    g.add_argument("-a", "--accept", action="store_true",
                   help="the current match is right — just tag it 'manual'")

    args = ap.parse_args()
    cmd = args.cmd or "review"          # bare `review.py` → interactive review
    try:
        if cmd == "search":
            sys.exit(cmd_search(args.query, args.channel))
        if cmd == "fix":
            sys.exit(cmd_fix(args.song_id, args.url, args.accept))
        # review / list (and the no-subcommand default)
        channel_id = getattr(args, "channel", config.DEFAULT_CHANNEL_ID) or None
        sys.exit(run(channel_id, list_only=(cmd == "list")))
    finally:
        storage.close()


if __name__ == "__main__":
    main()
