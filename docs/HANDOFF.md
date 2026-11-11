---
status: sop
desk_test: none
sources: []
updated: 2026-11-10
---
# DeetsDiscordBot — Handoff

> Cold start: read this first. This file holds what is still true. The record of each sitting
> is [WORKLOG.md](WORKLOG.md).

## State of play (2026-11-10)
- **This repo was made today** from DeetsOTD (all its history) and reorganized into `bot/`,
  `journal/` and `crates/tap`. The code is the same code that runs on the desk, moved.
- **The bot runs from here only**, `python -m bot`. At 20:26 on 2026-11-10 no bot process was
  running. Never start one from `../DeetsOTD`: two bot processes would both answer and both sync.
- **The nightly job runs from this repo since 2026-11-10**; the first run, at 6 PM that day,
  passed. **DeetsOTD is archived** (GitHub read-only, main `81bbf2c`). Its local `storage.db`
  is frozen as the backup (ops/NIGHTLY.md §4).
- **Built and used today:** `/vc` (play an app into a call), `/dm` (12 commands, native cards,
  Rooms rules), the Explicit/Clean column, the np card that edits on change, the add line
  with its row in Up Next. **Built, not yet seen in Discord:** the channel status and
  "Listening to" (features/STATUS.md), the column's width with a long song (features/CARDS.md
  §3), the 48 kHz capture's sound in a call.

## Open now
1. **One last manual restart** (`python -m bot`, at a natural pause): from then on the bot
   takes code changes by itself (architecture/LIVE-RELOAD.md). Then desk-test LIVE-RELOAD.md §7,
   STATUS.md, the suggestion words (CARDS.md §3), and pick a queue layout with `/dm preview`.
2. **The cut-over is done** (ops/NIGHTLY.md §4, 2026-11-10). The 6 PM run passed (485 songs,
   323 films, DeetsSolutions `df170f1`). DeetsOTD is archived. The references moved too:
   DeetsSolutions `2542d4d` (its project card is now "DeetsDiscordBot") and DeetsSQL `61bf0fc`
   (its default path). The installed DeetsSQL opens the new path only after a rebuild.
2a. **Write the resume hand-off atomically** (`bot/live.py` `restart`, the
   `RESUME_FILE.write_text` line): beside, flush, rename over (CLAUDE.md › Working style). It is
   a `live.py` change, so the running bot restarts at the next song: do it as the LIVE-RELOAD.md
   §7 desk test, with him listening.
3. Later, by his word: in-call extras, the Activity, image cards, the tray app, a Rust bot (ideas/rust-rewrite.md; the journal stays Python).

## Retired (2026-11-10)
The harness Discord bot (`../Vibe Coding Era/harness/discord_bot.py`) shared this token. Its
10 guild commands (`/reset` … `/emergency`) were cleared from the three servers that had them.
If it ever runs again, they come back. The harness itself was not touched.

## The desk
- DeetsMusic plays to the Dell monitor's speakers at 100% in the Windows mixer, so the PC is
  quiet and the call hears full level (VOICE-PIPELINE.md §1).
- `../DeetsOTD` is on main (`81bbf2c`, archived). Nothing runs from it. Keep the folder until
  he says.
- Python 3.12, discord.py 2.7.1, davey 0.1.7, PyNaCl.

## Gotchas
- **davey must stay below 0.2** with discord.py 2.7.x (VOICE-PIPELINE.md §4).
- **The capture hears the app after its mixer slider.** A quiet stream is the slider first.
- **`deetsmusic.exe` is several processes** (the app, plus its CLI and MCP from Claude
  sessions). The helper picks the top-most and its WebView2 child; `--list` shows who has sound.
- **The exe is locked while the bot streams.** Rename it aside to swap (ops/TAP.md §1).
- **Global commands lag in the client.** Ctrl+R.
- Notion: `notion_sync` uses an internal integration that must be connected to the DeetsOTD
  page, or it 403s (guide/USAGE.md §4).
