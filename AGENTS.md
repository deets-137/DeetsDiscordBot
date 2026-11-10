# DeetsDiscordBot — project guide for Codex

Lil Deets (a discord.py bot), the Song-of-the-Day and film journals behind deets.solutions,
and deets-tap (a Rust capture helper).

**The full project guide is [`CLAUDE.md`](CLAUDE.md). Read it first; every rule in it applies
to you.** This file repeats only the rules that cost the most when missed.

- **He decides every fork.** Bring him the choice before you build it, your recommendation
  first.
- **Never edit `../DeetsMusic` or `../DeetsOTD`.** The bot reaches DeetsMusic only through its
  bridge.
- **Never run a second bot process that syncs slash commands** on this token.
- **Never raise davey to 0.2+** with discord.py 2.7.x: the call goes silent.
- **Never commit `.env`, `config.py`, `storage.db`, `exports/` or `secrets/`.** The repo is
  public.
- **Never add a dependency without asking.**
