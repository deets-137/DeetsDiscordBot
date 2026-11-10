# DeetsDiscordBot

**Status:** in regular use · Python (discord.py) + a small Rust helper · Windows

Everything Discord for the Deets projects:

| Part | What it does |
|---|---|
| **Lil Deets** (`bot/`) | A Discord bot. `/vc play` streams one Windows app — usually [DeetsMusic](https://github.com/deets-137/DeetsMusicWindows) — into a voice channel. `/dm` lets the people in that call see what's playing, page through the queue, and add, skip or reorder songs, within the host's DeetsMusic Rooms rules. `/links` reads a channel's links. |
| **The journals** (`journal/`) | A Song-of-the-Day channel and a Letterboxd diary, kept in a local SQLite file and exported as the JSON that [deets.solutions](https://deets.solutions) shows on its Song of the Day and Movies pages. |
| **deets-tap** (`crates/tap`) | Captures one app's sound with Windows' per-process loopback and writes 48 kHz PCM for the bot. Built on [DeetsAirplay](https://github.com/deets-137/DeetsAirplay)'s capture crate. |

This repo continues [DeetsOTD](https://github.com/deets-137/DeetsOTD), with its full history.

## Run

```bash
pip install -r requirements.txt -r requirements-voice.txt
cp config.example.py config.py     # channel ids
cp .env.example .env               # DISCORD_TOKEN and the optional API keys
cd crates/tap && cargo build --release && cd ../..

python -m bot                      # the bot
python -m journal.scan <channel_id> --enrich --web --web-out <path>/songs.json
python -m journal.letterboxd_web rss --web -o <path>/movies.json
```

Setup in full, including the Discord portal and invite permissions: [docs/ops/SETUP.md](docs/ops/SETUP.md).
The commands: [docs/features/VC.md](docs/features/VC.md), [docs/features/DM.md](docs/features/DM.md).
The journal by hand: [docs/guide/USAGE.md](docs/guide/USAGE.md).

## Privacy

`storage.db`, `exports/`, `.env`, `config.py` and `secrets/` are gitignored. The website
JSON carries display fields only — never Discord message ids, user ids or jump links.
The bot talks to DeetsMusic only on `127.0.0.1`, with the token from DeetsMusic's own settings.

## License

MIT — see [LICENSE](LICENSE).
