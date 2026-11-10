---
status: sop
desk_test: none
sources: [requirements.txt, requirements-voice.txt, .env.example, config.example.py, crates/tap/Cargo.toml]
updated: 2026-11-10
---
# Setup on a fresh machine

Windows 11 (deets-tap uses Windows audio APIs). Run everything from the repo root.

## 1. Python
Python 3.12.
```
pip install -r requirements.txt            # the journal and the bot
pip install -r requirements-voice.txt      # /vc only: PyNaCl, davey (pinned <0.2)
```
discord.py 2.7.1 includes its own Opus library on Windows.

## 2. Private files (all gitignored)
- `cp .env.example .env` — `DISCORD_TOKEN`; optional `TMDB_API_KEY`, Notion, Apple Music keys
  (`APPLE_PRIVATE_KEY_PATH` points into `secrets/`); optional `VOICE_USER_IDS`, `DEETS_TAP`.
- `cp config.example.py config.py` — `DEFAULT_CHANNEL_ID`, `ALLOWED_CHANNEL_IDS`, harness host
  and port.
- `secrets/AuthKey_*.p8` — the Apple Music key, if used.
- `storage.db` — the journals. Copy it with SQLite's backup API (architecture/STORAGE.md §5).

## 3. deets-tap
Needs Rust (stable) and `../DeetsAirplay` beside this repo while the crate is a path
dependency (TAP.md §3).
```
cd crates/tap && cargo build --release
target/release/deets-tap.exe --info
```

## 4. The Discord developer portal
- Bot → Privileged Gateway Intents → **Message Content** on.
- Invite with scopes `bot` and `applications.commands`, and permissions: View Channels, Send
  Messages, Read Message History, Embed Links, Attach Files, **Connect**, **Speak**, **Set
  Voice Channel Status**.

## 5. DeetsMusic
For `/dm`: DeetsMusic running, with Settings › Connections › Agent control on (it gates the
bridge). For `/vc` to be heard: DeetsMusic's volume high in the Windows mixer, and its output
routed to a device you don't listen to if you want the PC quiet (VOICE-PIPELINE.md §1).

## 6. Run
`python -m bot`. "Logged in as … slash commands synced." means it is up. New commands may need
Ctrl+R in the Discord client.
