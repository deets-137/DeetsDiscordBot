---
status: sop
desk_test: none
sources: [Cargo.toml, crates/host/Cargo.toml, requirements.txt, .env.example, config.example.py, crates/tap/Cargo.toml]
updated: 2026-11-10
---
# Setup on a fresh machine

Windows 11 (the capture uses Windows audio APIs). Run everything from the repo root.

## 1. Rust (the bot)
Rust stable, with the MSVC toolchain (Visual Studio 2022 Build Tools, "Desktop development
with C++").

## 2. CMake (for libopus)
songbird builds libopus with CMake. The copy inside the VS 2022 Build Tools works; point
`CMAKE` at it for every cargo build:
```
$env:CMAKE = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
```

## 3. Build the bot
```
cargo build --workspace --release
cargo test --workspace          # the parity kit and the unit tests
```
The exes land in `target\release\`: `lildeets.exe`, `lildeets-host.exe`,
`lildeets-logic.exe`. A build while the bot runs goes live by itself (LIVE-RELOAD.md).

## 4. Private files (all gitignored)
- `cp .env.example .env`: `DISCORD_TOKEN`; optional `VOICE_USER_IDS` (who besides the
  application's owner may start `/vc`); for the journal, optional `TMDB_API_KEY`, Notion and
  Apple Music keys (`APPLE_PRIVATE_KEY_PATH` points into `secrets/`).
- `cp config.example.py config.py`: `DEFAULT_CHANNEL_ID` (the journal), `ALLOWED_CHANNEL_IDS`
  (limits `/links`).
- `secrets/AuthKey_*.p8`: the Apple Music key, if used.
- `storage.db`: the journals. Copy it with SQLite's backup API (architecture/STORAGE.md §5).

## 5. Python (the journal only)
Python 3.12.
```
pip install -r requirements.txt
```

## 6. deets-tap (a diagnostic tool)
The bot captures through `crates/tap` as a library. The `deets-tap.exe` tool is for finding
an app's sound by hand (`--meters`, `--list`, `--info`; ops/TAP.md). Cargo fetches
DeetsAirplay's crate from GitHub at the pinned rev.
```
cd crates/tap && cargo build --release
target/release/deets-tap.exe --meters
```

## 7. The Discord developer portal
- Bot → Privileged Gateway Intents → **Message Content** on (`/links` reads message text).
- Invite with scopes `bot` and `applications.commands`, and permissions: View Channels, Send
  Messages, Read Message History, Embed Links, Attach Files, **Connect**, **Speak**, **Set
  Voice Channel Status**.

## 8. DeetsMusic
For `/dm`: DeetsMusic running, with Settings › Connections › Agent control on (it gates the
bridge). For `/vc` to be heard: DeetsMusic plays to a Windows output (not AirPlay: no Windows
audio session, so the capture hears silence), its volume high in the Windows mixer, and its
output routed to a device you don't listen to if you want the PC quiet (VOICE-PIPELINE.md §1).

## 9. Node (the Activity only)
`activity/` is TypeScript and Vite (features/ACTIVITY.md). It needs Node 22 or newer (built
on 24.18).
```
cd activity
npm install
npm run mock        # the UI on fake data; Vite prints the address (5273, or the next free port)
npm run check       # types, then the copied look against ../DeetsMusic
```
- `npm run look:sync` copies DeetsMusic's look into `activity/deetsmusic-ui/look/`. It needs
  the DeetsMusic repo beside this one. The copy is committed, so a machine without DeetsMusic
  still builds; there `look:check` prints that it cannot check and passes.
- `npm install` may print that esbuild's install script was blocked. Vite still runs.

### 9.1 On a Mac
- **Works:** everything in `activity/` (the UI, the mock, later the relay Worker), and the
  docs. Clone, `npm install`, `npm run mock`.
- **Does not work:** the bot. The capture is Windows audio (`crates/tap`), DeetsMusic is a
  Windows app, and its bridge is on this PC's loopback. A `cargo build` of the workspace on
  macOS was not tried; expect `crates/tap` and `crates/host` to fail. So steps 3 to 5 of
  ACTIVITY.md §10 can be written on a Mac, and are tested on the PC.
- `.env`, `config.py` and `storage.db` are not in the repo and stay on the PC.

## 10. Run
`target\release\lildeets.exe`. The log shows `gateway: ready`, `logic: ready` and `commands:
synced` (or `unchanged since the last sync`). New commands may need Ctrl+R in the Discord
client. Ctrl+C stops it.
