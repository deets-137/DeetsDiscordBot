# DeetsOTD

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

> Personal, single-user. The harness binds to localhost with no auth — don't
> expose it.
