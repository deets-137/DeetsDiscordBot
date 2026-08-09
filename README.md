# DeetsOTD

**Status:** working and in regular use · **Platform:** Python — Discord bot, CLI, and a local web harness

Reads every hyperlink out of a Discord channel and writes them to Markdown + CSV.

Three ways to trigger the same core (`linkscan.py`):

| Entry point | Use it for |
|-------------|-----------|
| `bot.py`    | `/links` slash command inside Discord; DMs you the files |
| `scan.py`   | one-off run from the terminal |
| `harness.py`| device-side local web/API trigger (browser form or `curl`) |

Output lands in `exports/` as `links_<channel>_<timestamp>.md` (deduped, grouped,
with author/date/jump links) and `.csv` (every record, for spreadsheets).

## Setup

```bash
pip install -r requirements.txt
cp config.example.py config.py        # set DEFAULT_CHANNEL_ID / ALLOWED_CHANNEL_IDS
cp .env.example .env                  # paste your DISCORD_TOKEN
```

In the [Discord developer portal](https://discord.com/developers/applications):
your app → **Bot** → enable **Message Content Intent**. Invite the bot to your
server with the **Read Message History** permission.

Channel IDs: enable Developer Mode in Discord (Settings → Advanced), then
right-click a channel → **Copy ID**.

## Run

```bash
python scan.py 123456789012345678          # one-off, specific channel
python scan.py 123456789012345678 --limit 500

python bot.py                              # then use /links in Discord

python harness.py                          # open http://127.0.0.1:8077
```

### Per-user breakdown

To see *what each user has sent* in a channel — every user's links grouped
together, with repeats per user collapsed — add `--by-user`:

```bash
python scan.py 123456789012345678 --by-user                 # CSV (default)
python scan.py 123456789012345678 --by-user --format tsv    # TSV
python scan.py 123456789012345678 --by-user --format xml    # XML
```

This writes `exports/by_user_<channel>_<timestamp>.<ext>` and prints a
links-per-user tally. CSV/TSV are flat tables sorted so each user's links sit
together (`author, author_id, link_count, url, …`); XML nests `<link>` elements
under one `<user>` element each. Users are ordered by most recent activity.

### The journal + SongOTD archive

Treat the channel as a Song-of-the-Day journal that accumulates over time in a
local SQLite DB (`storage.db`, gitignored):

```bash
python scan.py <channel_id> --archive   # fold posts into the journal
python scan.py <channel_id> --enrich    # + resolve each song via iTunes
```

`--archive` records one **post** per song link a user typed (display name,
**Discord user ID**, the message's date/time, URL, and detected **service** —
`apple_music` / `spotify` / `youtube` / …). It's idempotent on
`(message_id, url)`, so re-scanning never double-counts.

`--enrich` (implies `--archive`) resolves each post against the free iTunes
APIs into a **`songs`** catalog — title, artist, album, genre, release date,
artwork, 30s preview, store link. Apple Music links resolve by their track id
(`direct`); Spotify/YouTube/etc. resolve via oEmbed title → iTunes search
(`search`). Posts that don't resolve (e.g. a non-music link) stay in the journal
unenriched.

Schema lives in [storage.py](storage.py); enrichment in [enrich.py](enrich.py).

### Website export (songs.json)

Feed the [Deets.Solutions](https://deets.solutions) Song-of-the-Day page a static,
display-only JSON straight from the journal:

```bash
python scan.py <channel_id> --enrich --web \
    --web-out ../deetssolutions/sotd/songs.json
```

`--web` (implies `--archive`) writes `{generated_at, channel, count, songs[]}`. Each
song carries only display fields — track/artist/album, genre, artwork, 30s preview,
Apple Music link, `duration_sec`, dates, poster **name** — and **drops the private
Discord ids** (message id, author id, jump link) so the file is safe on a public
site. Numbers come out as real numbers (or `null`) so the page can sort/filter
client-side. `--web-out` sets the path (default `exports/songs.json`); point it at
the site repo and commit the result.

Regenerating from an existing `storage.db` needs no Discord/iTunes:

```bash
python -c "import storage; storage.export_web(<channel_id>, out_path='../deetssolutions/sotd/songs.json')"
```

> Personal, single-user. The harness binds to localhost with no auth — don't
> expose it.

## License

MIT — see [LICENSE](LICENSE).
