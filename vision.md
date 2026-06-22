# DeetsOTD — Vision

> Working doc. Captures only what's been decided/provided so far. The harness
> vision itself is intentionally left for the author to fill in.

## Origin

A Discord bot to read all the hyperlinks in a channel.

## Decided

- **Standalone.** Its own project (`~/Documents/GitHub/DeetsOTD`), not wired into
  DeetsCode.
- **Output:** Markdown + CSV.
- **Triggers:** Discord slash command, one-off CLI script, and a device-side
  harness — all on a single shared core.
- A new harness UI will be built from scratch — new ideas, new architecture —
  directed piece by piece.

## Exists today (substrate)

- `linkscan.py` — core: walk channel history, extract links, write outputs.
  Seams: `collect_links(channel, limit)` and `write_outputs(records, label)`.
- `bot.py` — `/links` slash command.
- `scan.py` — one-off CLI.
- `harness.py` — minimal local FastAPI trigger (placeholder; to be replaced/rethought).
- `paths.py`, `config.example.py`, `.env.example`, `requirements.txt`, `README.md`.

## Open — for the author

_(left blank intentionally)_

- The big concept — what the harness is *for*:
- Architecture stance:
- Where state lives:
- UI / aesthetic direction:
- Anything else:
