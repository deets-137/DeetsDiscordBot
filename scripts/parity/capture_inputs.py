"""
Capture the parity kit's INPUTS once, from DeetsMusic's bridge (docs/ideas/rust-port-plan.md §4).

Everything comes from **Apple Music catalog searches** (public data), never from the owner's
queue, library or history: this repo is public. The "queue" and "now playing" are built from
search results so every case the cards handle is present: explicit, clean and unrated songs,
a song of 10 minutes or more, long titles, 23 songs in Up Next (3 pages).

    python scripts/parity/capture_inputs.py      # needs DeetsMusic running; rewrites tests/parity/input/

Re-run only to refresh the kit; then re-run render_expected.py and commit both.
"""

import asyncio
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from bot.dm_bridge import Bridge  # noqa: E402

OUT = ROOT / "tests" / "parity" / "input"
QUERIES = {
    "humble": "humble",
    "pink_white": "pink white frank ocean",
    "dna": "kendrick lamar dna",
    "long": "rush 2112",
    "mix": "phoebe bridgers kyoto",
}
KEEP = ("catalogId", "title", "artistName", "albumName", "durationMs", "contentRating", "artwork")


def slim(t: dict) -> dict:
    """Only the fields the bot reads (docs/architecture/BRIDGE.md §3)."""
    out = {k: t[k] for k in KEEP if k in t}
    if "artwork" in out:
        out["artwork"] = {"urlTemplate": out["artwork"].get("urlTemplate")}
    return out


async def main() -> None:
    b = Bridge()
    OUT.mkdir(parents=True, exist_ok=True)
    results = {}
    for name, term in QUERIES.items():
        songs = [slim(t) for t in await b.search_songs(term)]
        results[name] = songs
        (OUT / f"search_{name}.json").write_text(json.dumps({"term": term, "songs": songs}, indent=1, ensure_ascii=False), encoding="utf-8")

    # A queue from public search results: current + 23 upcoming, de-duplicated.
    pool, seen = [], set()
    for name in ("pink_white", "dna", "humble", "mix", "long"):
        for t in results[name]:
            if t.get("catalogId") and t["catalogId"] not in seen:
                seen.add(t["catalogId"])
                pool.append(t)
    long_ones = [t for t in pool if (t.get("durationMs") or 0) >= 600_000]
    if not long_ones:
        raise SystemExit("no song of 10+ minutes in the searches; change QUERIES['long']")
    cleans = [t for t in pool if t.get("contentRating") == "clean"]
    if not cleans:
        raise SystemExit("no clean-rated song in the searches; add a query that has one")
    special = (long_ones[0], cleans[0])
    current, upcoming = pool[0], [t for t in pool[1:] if t not in special][:21]
    upcoming.insert(2, cleans[0])     # a clean song on page 1
    upcoming.insert(5, long_ones[0])  # a 10+ minute song on page 1
    queue = {"current": current, "upcoming": upcoming, "history": []}
    (OUT / "queue.json").write_text(json.dumps(queue, indent=1, ensure_ascii=False), encoding="utf-8")

    dur = (current.get("durationMs") or 0) / 1000
    tmpl = (current.get("artwork") or {}).get("urlTemplate")
    np = {"active": True, "playing": True, "title": current["title"], "artist": current["artistName"],
          "album": current["albumName"], "artworkTemplate": tmpl,
          "artworkUrl": tmpl.replace("{w}", "480").replace("{h}", "480") if tmpl else None,
          "catalogId": current["catalogId"], "currentTime": 58.0, "duration": dur,
          "progress": 58.0 / dur if dur else 0, "live": False}
    (OUT / "now_playing.json").write_text(json.dumps(np, indent=1, ensure_ascii=False), encoding="utf-8")

    rules = {"settings": [{"key": k, "value": "everyone"} for k in
                          ("roomGuests.playPause", "roomGuests.skip", "roomGuests.seek", "roomGuests.add", "roomGuests.changeQueue")]}
    (OUT / "settings_rooms.json").write_text(json.dumps(rules, indent=1), encoding="utf-8")
    (OUT / "settings_theme.json").write_text(json.dumps({"settings": [{"key": "theme", "value": "moonlight"}]}, indent=1), encoding="utf-8")
    print(f"wrote {len(list(OUT.glob('*.json')))} input files to {OUT}")


if __name__ == "__main__":
    asyncio.run(main())
