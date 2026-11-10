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
- **The running bot is still DeetsOTD's** (`python bot.py` in `../DeetsOTD`, on its branch
  `lil-deets-voice`). Next restart: run it from here, `python -m bot`. Stop the old one first:
  two bot processes would both answer and both sync.
- **The nightly job still runs from `../DeetsOTD`.** This repo passed the side-by-side test;
  the switch is ops/NIGHTLY.md §4.
- **Built and used today:** `/vc` (play an app into a call), `/dm` (12 commands, native cards,
  Rooms rules), the Explicit/Clean column, the np card that edits on change, the add line
  with its row in Up Next. **Built, not yet seen in Discord:** the channel status and
  "Listening to" (features/STATUS.md), the column's width with a long song (features/CARDS.md
  §3), the 48 kHz capture's sound in a call.

## Open now
1. **Restart the bot from here** (`python -m bot`); desk-test STATUS.md and CARDS.md §3.
2. **The cut-over** (ops/NIGHTLY.md §4): re-copy `storage.db` after a 6 PM run, switch the
   DeetsSolutions script, watch one real run, then archive DeetsOTD.
3. **Merge DeetsOTD's `lil-deets-voice` branch** into its main at the archive step.
   (DeetsAirplay's was merged and pushed 2026-11-10: master `4185ed6`, which `crates/tap` pins.)
4. Later, by his word: in-call extras, the Activity, image cards, the tray app, a Rust bot.

## Retired (2026-11-10)
The harness Discord bot (`../Vibe Coding Era/harness/discord_bot.py`) shared this token. Its
10 guild commands (`/reset` … `/emergency`) were cleared from the three servers that had them.
If it ever runs again, they come back. The harness itself was not touched.

## The desk
- DeetsMusic plays to the Dell monitor's speakers at 100% in the Windows mixer, so the PC is
  quiet and the call hears full level (VOICE-PIPELINE.md §1).
- `../DeetsOTD`'s working tree is on `lil-deets-voice`; the nightly job runs that checkout.
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
