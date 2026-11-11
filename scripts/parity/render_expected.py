"""
Render the parity kit's EXPECTED outputs: what the Python bot produces for the committed inputs
(docs/ideas/rust-port-plan.md §4). The Rust port must produce the same, case by case.

    python scripts/parity/render_expected.py     # offline: no Discord, no DeetsMusic

It runs the real functions in bot/dm.py against a fake bridge that serves tests/parity/input/,
with the clock frozen at FROZEN_NOW, and writes tests/parity/expected/:

  commands.json            every slash command's registration payload (/links, /vc, /dm)
  cards/*.json             Components V2 payloads (np, queue pages, picker); custom_ids → "<custom_id>"
  text/*.txt               suggestion rows, help text, the "added" lines, one case per file
  cases.json               confident(), tag(), rating_word(), column_width() tables

Re-run after any change to bot/dm.py's output, and commit the result with the change.
"""

import asyncio
import json
import sys
import types
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
# The Python bot is a record now (docs/archive/bot/, 2026-11-10); import it from there.
sys.path.insert(0, str(ROOT / "docs" / "archive"))

import discord  # noqa: E402

from bot import dm  # noqa: E402
from bot.dm_bridge import Bridge  # noqa: E402

IN = ROOT / "tests" / "parity" / "input"
OUT = ROOT / "tests" / "parity" / "expected"
FROZEN_NOW = 1_794_340_000  # a fixed unix time, so <t:…:R> stamps are stable


def load(name: str):
    return json.loads((IN / name).read_text(encoding="utf-8"))


class FakeBridge(Bridge):
    """Serves the input files for the routes the bot reads; refuses everything else."""

    def __init__(self, now_playing=None, queue=None):
        super().__init__()
        self.np = now_playing or load("now_playing.json")
        self.q = queue or load("queue.json")
        self.searches = {d["term"].lower(): d["songs"] for d in (load(p.name) for p in IN.glob("search_*.json"))}

    async def call(self, path, body=None, timeout=20.0):
        if path == "/now-playing":
            return self.np
        if path == "/queue":
            return self.q
        if path == "/search":
            return {"songs": self.searches.get(body["term"].strip().lower(), [])}
        if path == "/settings?section=rooms":
            return load("settings_rooms.json")
        if path == "/settings":
            return load("settings_theme.json")
        raise AssertionError(f"the parity kit has no input for {path}")


def scrub(obj):
    """Replace discord.py's random custom_ids so the output is stable."""
    if isinstance(obj, dict):
        return {k: ("<custom_id>" if k == "custom_id" else scrub(v)) for k, v in obj.items()}
    if isinstance(obj, list):
        return [scrub(v) for v in obj]
    return obj


def write(rel: str, data) -> None:
    p = OUT / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(data, str):
        p.write_text(data + "\n", encoding="utf-8", newline="\n")
    else:
        p.write_text(json.dumps(data, indent=1, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8", newline="\n")


class User:
    display_name = "Deets"


async def main() -> None:
    dm.time = types.SimpleNamespace(time=lambda: FROZEN_NOW)
    client = discord.Client(intents=discord.Intents.default())

    # Commands, exactly as registered.
    import bot.app as app
    write("commands.json", sorted((c.to_dict(app.tree) for c in app.tree.get_commands()), key=lambda c: c["name"]))

    # Cards.
    dm.bridge = FakeBridge()
    write("cards/np_playing.json", scrub((await dm.np_view(client, None)).to_components()))
    paused = dict(load("now_playing.json"), playing=False)
    dm.bridge = FakeBridge(now_playing=paused)
    write("cards/np_paused.json", scrub((await dm.np_view(client, None)).to_components()))
    dm.bridge = FakeBridge(now_playing={"active": False})
    write("cards/np_nothing.json", scrub((await dm.np_view(client, None)).to_components()))
    dm.bridge = FakeBridge()
    for page in (1, 2, 3, 9):
        write(f"cards/queue_page{page}.json", scrub((await dm.queue_view(client, page)).to_components()))
    dm.bridge = FakeBridge(queue={"current": None, "upcoming": [], "history": []})
    write("cards/queue_empty.json", scrub((await dm.queue_view(client, 1)).to_components()))
    dm.bridge = FakeBridge()
    humble = load("search_humble.json")["songs"]
    for next_up in (False, True):
        write(f"cards/picker_humble_next{int(next_up)}.json",
              scrub(dm.picker_view(client, "humble", humble[:5], next_up, 0xDFECFC).to_components()))

    # Suggestion rows, per query (the column width is per query).
    for p in sorted(IN.glob("search_*.json")):
        d = load(p.name)
        rows = [t for t in d["songs"][:25] if t.get("catalogId")]
        w = dm.column_width(rows)
        write(f"text/suggestions_{p.stem.removeprefix('search_')}.txt", "\n".join(dm.suggestion(t, w) for t in rows))

    # Help text: the three situations.
    actions = list(dm.RULE)
    write("text/help_host.txt", dm.help_text({a: True for a in actions}, host=True, session=True))
    guest = {a: dm.RULE[a] is not None for a in actions}
    write("text/help_guest_in_call.txt", dm.help_text(guest, host=False, session=True))
    write("text/help_guest_no_stream.txt", dm.help_text({a: False for a in actions}, host=False, session=False))

    # The "added" lines: later / next, playing / paused.
    q = load("queue.json")
    for t, next_up, name in ((q["upcoming"][3], False, "later"), (q["upcoming"][0], True, "next"), (q["upcoming"][5], False, "long")):
        dm.bridge = FakeBridge()
        write(f"text/added_{name}_playing.txt", await dm._added(User, [t], next_up, f"song:{t['catalogId']}"))
        dm.bridge = FakeBridge(now_playing=paused)
        write(f"text/added_{name}_paused.txt", await dm._added(User, [t], next_up, f"song:{t['catalogId']}"))

    # Tables.
    up = q["upcoming"]
    terms = ["pink white frank", "pink", "humble", "humble kendrick", "hum", "dna kendrick", "kendrick lamar dna",
             "rush 2112", "2112", "kyoto phoebe", "zzzz", ""]
    searches = {d["term"]: d["songs"] for d in (load(p.name) for p in IN.glob("search_*.json"))}
    pools = {"humble": searches["humble"], "pink white frank ocean": searches["pink white frank ocean"],
             "kendrick lamar dna": searches["kendrick lamar dna"], "rush 2112": searches["rush 2112"],
             "phoebe bridgers kyoto": searches["phoebe bridgers kyoto"]}
    write("cases.json", {
        "frozen_now": FROZEN_NOW,
        "confident": [{"term": term, "results_of": pool, "confident": dm.confident(term, songs)}
                      for pool, songs in pools.items() for term in terms],
        "tag_column": [{"catalogId": t["catalogId"], "width": dm.column_width(up[:10]), "tag": dm.tag(t, dm.column_width(up[:10]))} for t in up[:10]],
        "tag_alone": [{"catalogId": t["catalogId"], "tag": dm.tag(t)} for t in up],
        "rating_word": [{"contentRating": r, "word": dm.rating_word({"contentRating": r} if r else {})} for r in ("explicit", "clean", None)],
        "column_width": [{"durations_ms": [t["durationMs"] for t in up[i:i + 10]], "width": dm.column_width(up[i:i + 10])} for i in (0, 10, 20)],
        "bar": [{"progress": p, "bar": dm._bar(p)} for p in (0, 0.01, 0.25, 0.5, 0.99, 1)],
        "mmss": [{"seconds": s, "text": dm.mmss(s)} for s in (0, 9.4, 59.6, 60, 185.2, 1232, 6000, None)],
        "clip": [{"text": "x" * n, "n": 10, "out": dm._clip("x" * n, 10)} for n in (9, 10, 11)],
    })
    render_links()
    print(f"wrote {sum(1 for _ in OUT.rglob('*') if _.is_file())} expected files to {OUT}")


def render_links() -> None:
    """/links: made-up messages (input/links_messages.json) → records, Markdown and CSV."""
    import datetime as dt
    import tempfile

    from journal import linkscan

    msgs = load("links_messages.json")

    def to_message(m):
        ns = types.SimpleNamespace
        return ns(id=m["id"], content=m["content"], jump_url=m["jump_url"],
                  created_at=dt.datetime.fromisoformat(m["created_at"]),
                  author=ns(display_name=m["author"], id=m["author_id"]),
                  embeds=[ns(url=e.get("url"), title=e.get("title"), image=ns(url=e.get("image")),
                             thumbnail=ns(url=e.get("thumbnail"))) for e in m.get("embeds", [])],
                  attachments=[ns(url=a["url"], filename=a["filename"]) for a in m.get("attachments", [])])

    records = [r for m in msgs for r in linkscan.extract_from_message(to_message(m))]
    write("links/records.json", [r.as_row() | {"author_id": r.author_id} for r in records])

    frozen = dt.datetime.fromtimestamp(FROZEN_NOW, dt.timezone.utc)

    class FrozenDatetime(dt.datetime):
        @classmethod
        def now(cls, tz=None):
            return frozen if tz else frozen.replace(tzinfo=None)

    saved = (linkscan.datetime, linkscan.EXPORTS_DIR)
    with tempfile.TemporaryDirectory() as tmp:
        linkscan.datetime, linkscan.EXPORTS_DIR = FrozenDatetime, Path(tmp)
        try:
            md, csv_path = linkscan.write_outputs(records, "song-of-the-day")
            write("links/output_name.txt", f"{md.name}\n{csv_path.name}")
            # Byte for byte: Python's csv module ends rows with \r\n, and the port must too.
            (OUT / "links").mkdir(parents=True, exist_ok=True)
            (OUT / "links" / "output.md").write_bytes(md.read_bytes())
            (OUT / "links" / "output.csv").write_bytes(csv_path.read_bytes())
        finally:
            linkscan.datetime, linkscan.EXPORTS_DIR = saved


if __name__ == "__main__":
    asyncio.run(main())
