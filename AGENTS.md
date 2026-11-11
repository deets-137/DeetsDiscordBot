# DeetsDiscordBot — project guide for Codex

Lil Deets (a Rust Discord bot: launcher, host, logic), the Song-of-the-Day and film journals
behind deets.solutions (Python), and deets-tap (the Rust capture). The first, Python bot is
archived in `docs/archive/bot/`.

**The full project guide is [`CLAUDE.md`](CLAUDE.md). Read it first; every rule in it applies
to you.** This file repeats only the rules that cost the most when missed.

- **He decides every fork.** Bring him the choice before you build it, your recommendation
  first.
- **Never edit `../DeetsMusic` or `../DeetsOTD`.** The bot reaches DeetsMusic only through its
  bridge.
- **Never run a second bot process that syncs slash commands** on this token.
- **Never keep voice state in the logic** (`crates/logic`): it is swapped while the music
  plays. Calls, captures, gain and presence live in the host.
- **Never commit `.env`, `config.py`, `storage.db`, `exports/` or `secrets/`.** The repo is
  public.
- **Never add a dependency without asking.**
