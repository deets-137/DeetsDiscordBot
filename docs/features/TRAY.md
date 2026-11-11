---
status: project
desk_test: none
sources: []
updated: 2026-11-10
---
# A tray app that runs the bot

> **Status:** project · 2026-11-10. Not built. §1 is the owner's ask. §2–§4 are Claude's
> proposal from the talk of 2026-11-10; §5 holds the forks for when this starts.

## 1. The ask (the owner, 2026-11-10)
"Pretend I wanted to make an app in the tray that can start up the discord bot with one click
for release." A Rust rewrite "for efficiency" is for later, "not a priority for now".

## 2. Proposed: what it does
- A Tauri v2 tray app, the same shell as DeetsAirplay and DeetsRGB (Press skin, the shared
  theme tokens copied from DeetsRGB).
- **Its only job is the bot's life:** start, stop, restart; status (connected, which voice
  channel, what plays); the last log lines; launch at sign-in.
- **The bot stays its own process.** A crash in one never takes down the other.
- The panel shows per-app levels like `deets-tap --meters` (§3), so a quiet stream is visible
  before anyone in the call says so.

## 3. Proposed: the repo shape
```
crates/         the bot (Rust: launcher, host, logic)
journal/        the nightly pipeline (unchanged)
crates/tap/     deets-tap as a library and a program: the tray links the library for meters
src-tauri/      the tray app (Rust): supervises `lildeets.exe`, links `crates/tap` for meters
src/            the tray panel (vanilla TS)
```
- **The tray talks to the bot over a local status channel**, a small localhost port with a
  token (the pattern of DeetsMusic's bridge), never by parsing the bot's console.
- **The token** moves out of `.env` into Windows Credential Manager for the release build.

## 4. Proposed: shipping Python
> **Part:** project · 2026-11-10 — a fork for when this starts.

1. **The official embeddable Python, bundled as a resource** (recommended): no antivirus false
   alarms; easy to patch.
2. **A PyInstaller one-file exe:** simpler to ship; often flagged by antivirus.
3. **Rewrite the bot in Rust** (serenity/songbird): one binary, the most efficient. Before it
   is chosen, confirm a Rust voice library supports DAVE (VOICE-PIPELINE.md §4).

## 5. Open forks
- §4: how Python ships.
- Whether the tray also owns the nightly schedule (today: Task Scheduler runs a
  DeetsSolutions script; ops/NIGHTLY.md).
