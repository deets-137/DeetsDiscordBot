---
status: foundation
desk_test: none
sources: [bot/__main__.py, bot/voice.py, bot/dm.py, journal/scan.py, crates/tap/src/main.rs]
updated: 2026-11-10
---
# DeetsDiscordBot — design

The overview a new reader meets first. The details are in the area docs; this file says
what the parts are for and which decisions shaped them.

## 1. What it is for (the owner, 2026-11-10)
- **The bot's main jobs are playing music and reading channels**, as deets.solutions needs.
  The AI harness over Discord (`Vibe Coding Era/harness/discord_bot.py`) is not part of it:
  it was retired the same day (HANDOFF.md › Retired).
- **Friends in a call hear the host's DeetsMusic and can operate it**: see what plays, queue,
  skip, reorder. The host starts the stream; the guests act within his Rooms rules.

## 2. The parts

```
Discord ──── Lil Deets (python -m bot) ──────────────────────────────────────────┐
              ├─ /links ── journal.linkscan                                       │
              ├─ /vc ───── deets-tap.exe (crates/tap) ── per-process loopback ── DeetsMusic's WebView2
              └─ /dm ───── bot.dm_bridge ── http://127.0.0.1:47825 (DeetsMusic's bridge)
Task Scheduler 6 PM ── DeetsSolutions/scripts/nightly-sotd.ps1
              ├─ python -m journal.scan … --web ──── storage.db ── songs.json
              └─ python -m journal.letterboxd_web rss --web ── storage.db ── movies.json
```

| Part | Doc |
|---|---|
| The bot process, its command tree, its modules | architecture/BOT.md |
| The audio path into a call | architecture/VOICE-PIPELINE.md, ops/TAP.md |
| Talking to DeetsMusic | architecture/BRIDGE.md |
| Who may do what | architecture/PERMISSIONS.md |
| The journals' data | architecture/STORAGE.md, features/JOURNAL.md, features/FILMS.md |
| The nightly publish | ops/NIGHTLY.md |

## 3. The owner's decisions

| Date | Decision |
|---|---|
| 2026-11-10 | The bot taps DeetsMusic (or any app) on this PC and plays it into a call. No change to DeetsMusic's code; the song pulls and journal scripts stay unaffected. |
| 2026-11-10 | The bot lives in the DeetsOTD bot process (now `python -m bot`), not the harness bot. |
| 2026-11-10 | `/dm` permissions follow DeetsMusic's Rooms guest rules. |
| 2026-11-10 | Native Components V2 cards first; rendered image cards to be tried after (ideas/image-cards.md). |
| 2026-11-10 | The np card edits only when the song or play state changes, not to move the scrubber. |
| 2026-11-10 | An add says where in Up Next the song landed. A length and Explicit/Clean column in suggestions and cards, the same width for every row of one list. |
| 2026-11-10 | Voice channel status and "Listening to" now; the card in the call's chat, vote-to-skip and the Activity later. |
| 2026-11-10 | Friends don't tap their own apps through this bot (ideas/friends-tapping.md). |
| 2026-11-10 | One repo for everything Discord: DeetsOTD moves here whole, as a new public repo. DeetsOTD stays untouched until the migration is confirmed, then becomes an archive. |
| 2026-11-10 | deets-tap moves into this repo as `crates/tap`, with steps against drift (ops/TAP.md §3). |
| 2026-11-10 | A tray app to start the bot with one click is the release shape; a Rust rewrite is for later, not a priority (features/TRAY.md). |
| 2026-11-10 | Live reload is automatic (architecture/LIVE-RELOAD.md). |
| 2026-11-10 | If the bot moves to Rust, **the journal stays Python** (ideas/rust-rewrite.md). |

## 4. Shape rules
- **`bot/` imports `journal/`; `journal/` never imports `bot/`.** The nightly job never loads
  Discord voice code.
- **One process owns the slash commands** (BOT.md §2).
- **DeetsMusic is a service on 127.0.0.1**, reached only through its bridge; nothing here
  reads its files except the bridge token in its settings.
- **Everything private stays at the repo root and is gitignored.** The repo is public.
