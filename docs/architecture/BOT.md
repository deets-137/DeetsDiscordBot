---
status: shipped
desk_test: passed 2026-11-10
sources: [bot/__main__.py, bot/__init__.py, journal/__init__.py, journal/paths.py]
updated: 2026-11-10
---
# The bot process

## 1. One process
`python -m bot`, from the repo root, runs `bot/__main__.py`:
1. Loads `.env` from the repo root (`journal.paths.PROJECT_ROOT`) and imports `config.py`.
2. Makes one `discord.Client` with the default intents plus **Message Content** (for `/links`).
   The default intents include **voice states**, which `/vc` and `/dm` need.
3. Builds one `app_commands.CommandTree`, then lets each module add its group:
   `voice.register(client, tree)` (`/vc`) and `dm.register(client, tree)` (`/dm`).
   `/links` is defined in `__main__.py` itself.
4. `on_ready` syncs the tree **globally** and prints "slash commands synced".

A module plugs in with `register(client, tree)`. It may add a command group, a
`client.event` the others don't use (`voice.py` takes `on_voice_state_update`), or a
background task by wrapping `client.setup_hook` (`dm.StatusWatcher.install`). Never replace
`on_ready` from a module.

## 2. One command list per application
Discord keeps **one global command list per application**. A `tree.sync()` replaces it with
that process's tree. So:
- **Only this process syncs.** The journal's `scan` logs in with the same token to read the
  channel, then logs out; it never syncs. Keep it so.
- The harness bot (`Vibe Coding Era/harness/discord_bot.py`) shared the token and synced its
  own commands per guild. It was retired on 2026-11-10 and its guild commands were cleared
  (HANDOFF.md › Retired). If it ever runs again, its commands come back in those guilds.
- Global commands can take a while to show in a Discord client: **Ctrl+R** reloads it.

## 3. Packages and imports
- `bot/` imports `journal/` (for `linkscan`); `journal/` never imports `bot/`.
- Both are packages with relative imports inside. `journal/__init__.py` puts the repo root on
  `sys.path`, so `import config` works from any working directory; `bot/__init__.py` imports
  `journal` first for the same reason.
- Private files stay at the root: `.env`, `config.py`, `storage.db`, `exports/`, `secrets/`.

## 4. Logging
discord.py's `client.run()` installs a stderr handler on the `discord` logger only. The bot's
own loggers sit under it so their lines print: `discord.lildeets.voice`,
`discord.lildeets.dm`. A logger outside `discord.*` prints nothing.

## 5. The token
`DISCORD_TOKEN` in `.env`. The application is "Lil Deets". The same token is used by
`journal.scan`. Reset it in the developer portal → update `.env` here (and in
`../DeetsOTD/.env` until the cut-over).

## 6. Servers (2026-11-10)
The bot is in four servers (friends' servers: names stay out of this public repo). It has
Connect, Speak and Set Voice Channel Status in all four.
