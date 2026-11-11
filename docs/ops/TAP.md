---
status: foundation
desk_test: passed 2026-11-10
sources: [crates/tap/src/lib.rs, crates/tap/src/main.rs, crates/tap/Cargo.toml]
updated: 2026-11-10
---
# deets-tap

The capture `/vc` uses. Rust, Windows only. Two faces: **a library** (`TapStream`, `crates/tap/src/lib.rs`) that the bot's host links in, and **the `deets-tap.exe` tool** for looking at apps' sound by hand (the archived Python bot ran it as a helper). Moved here from DeetsAirplay
(`crates/airplay/src/bin/deets-tap.rs`) on 2026-11-10; its history is in DeetsAirplay's log.

## 1. Build
```
cd crates/tap
cargo build --release          # → target/release/deets-tap.exe, the tool
```
The library builds with the bot (`cargo build --workspace --release` at the root; the host
depends on `crates/tap` by path, though `crates/tap` is outside the workspace). The Rust bot
never runs the exe, so it is never locked by the bot.

## 2. Flags
| Flag | Prints / does |
|---|---|
| `--info` | The contract as one JSON line: `{"name","version","contract","rate","channels","bits","frame_bytes"}` |
| `--list` | Apps with an audio session on the default output now, one per line (WebView2 children named after their host) |
| `--meters` | Every active output device, each session's app, its peak over 1 s and its mixer volume |
| `--exe NAME` | Stream that app: 48 kHz / 16-bit / stereo LE on stdout in 3840-byte blocks |
| `--pid N` | Stream that process tree |

Exit 2 with a message on stderr for a bad argument or an app that isn't running.

## 3. Against drift (the owner, 2026-11-10)
Two things can drift: the helper against the bot, and the helper against DeetsAirplay's crate.
1. **The stream contract** (the archived Python bot only). `CONTRACT` in `src/main.rs` and
   `EXPECTED_TAP` in `docs/archive/bot/voice.py` describe the exe's stdout stream; the Python
   `/vc play` checked `--info` against it. The Rust bot reads `TapStream` directly, so the
   compiler is its contract. Change the stdout format → still bump `CONTRACT`.
2. **The crate.** deets-tap calls `Capture::start_process_at`, `mixer::children_named` and
   `mixer::running_exes`. DeetsAirplay's `CLAUDE.md` names this repo as a consumer of those.
3. **Pinning.** `deets-airplay` is pinned by **git rev** (DeetsAirplay master `4185ed6`,
   2026-11-10, the first rev with `start_process_at`), with `Cargo.lock` committed: DeetsMusic's
   pattern. To try a local DeetsAirplay change before it is pushed, add a gitignored
   `crates/tap/.cargo/config.toml` with
   `[patch."https://github.com/deets-137/DeetsAirplay"] deets-airplay = { path = "../../../DeetsAirplay/crates/airplay" }`,
   and delete it before committing `Cargo.lock`.
4. **The rev check.** `powershell -File scripts/check-airplay.ps1 [-Ref <commit>]` reads the
   pin, fetches DeetsAirplay, lists the commits after the pin that touch `capture.rs` or
   `mixer.rs`, and prints the three signatures deets-tap calls at the newest commit (exit 2 if
   one is missing). Run it before bumping the rev; then bump, build, `--info`, commit the lock.

## 4. Measured (2026-11-10)
- 48 kHz capture of DeetsMusic: 198 blocks in 4.01 s (on pace), 195 with sound, peak 12430.
- At DeetsMusic 17% in the mixer: peak 91 — near silence in the call.
