---
status: foundation
desk_test: passed 2026-11-10
sources: [crates/tap/src/main.rs, crates/tap/Cargo.toml, bot/voice.py]
updated: 2026-11-10
---
# deets-tap

The capture helper `/vc` runs. Rust, Windows only. Moved here from DeetsAirplay
(`crates/airplay/src/bin/deets-tap.rs`) on 2026-11-10; its history is in DeetsAirplay's log.

## 1. Build
```
cd crates/tap
cargo build --release          # → target/release/deets-tap.exe, where bot/voice.py looks
```
The bot never rebuilds it. While the bot streams, the exe is locked: build to another folder
(`--target-dir target/next`) and swap after the bot stops, or rename the running exe aside
(Windows allows a rename of a running exe) and copy the new one in.

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
1. **The stream contract.** `CONTRACT` in `src/main.rs` and `EXPECTED_TAP` in `bot/voice.py`
   describe the same stream. `/vc play` runs `--info` (again whenever the exe changes) and
   refuses a mismatch with the rebuild command. **Change the format → bump both, together.**
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
