---
status: sop
desk_test: none
sources: []
updated: 2026-11-10
---
# DeetsDiscordBot — Work log

One dated `##` per sitting, newest first. An old entry is never corrected. DeetsOTD's earlier
history is in `git log` and docs/archive/.

## 2026-11-10 (night) — the switch to the Rust bot; the song-boundary restart

- **His words.** "commit and push everything please"; "build the song-boundary restart, then
  go ahead with 1, 3, and 4" (the restart, twilight, the command sync; not the docs rewrite
  or the week); "I'll switch to the rust bot entirely now, no need to spend a week testing
  since no one uses this live for now".
- **Committed and pushed** `rust-rewrite` (`fa3c6b8`, P1–P8).
- **Built:** the host's song-boundary restart (the launcher runs the host as a copy; the
  host watches its exe, waits for the song to end, pauses, writes the resume file
  atomically, exits 75; the new host rejoins and plays after 2 s); `.lildeets-hold` for the
  host and the logic swap; the host clears the command-sync record at start; a 400's
  `errors` now shows in the log.
- **twilight:** no advisories; 0.18.0 is the newest and stays pinned (rust-port-plan.md P8).
- **Desk test, Moshi's:** `/vc play`, the status line and "Listening to" work; two logic
  swaps while playing, no gap. `/dm queue` failed: duplicated ◀ ▶ custom_ids on a clamped
  page. Fixed and swapped in live; not yet retried by him.
- **Left open:** the rest of the desk test (HANDOFF item 0) with the restart; the docs
  rewrite; `bot/` to the archive; the merge into main. The host logs a WARN when it stops an
  old logic (os error 10054); it is expected and should be quiet (a host change: it goes
  live through the restart, so do it with him listening).

## 2026-11-10 (evening) — the Rust port, P0–P8, on branch `rust-rewrite`

- **His words.** "let's open a new branch 'rust-rewrite' and get started, no agents";
  "continue with the build as far as you can"; "Build as far as you can".
- **Built** (all on `rust-rewrite`; the plan is ideas/rust-port-plan.md, each package
  marked there): the workspace and parity loader (P0); the text formats and the cards
  (P1); `TapStream` in `crates/tap` (P2); hand-rolled typed REST (P3); the bridge client
  (P4); `lildeets-ipc` and `lildeets-host` (P5); `lildeets-logic` with every command,
  button and autocomplete (P6) and the two watchers (P7); the `lildeets.exe` launcher (P8).
  82 workspace tests + 3 in `crates/tap`; clippy clean.
- **The voice spike passed by ear** (General in Moshi's Minecraft Monsters): twilight 0.18
  with our glue + songbird + DAVE, "clean". Two silent runs first: songbird ships no PCM
  decoder (fixed with symphonia's `pcm`), then DeetsMusic was on AirPlay (no Windows audio
  session to capture; now in DEBUGGING.md §1.3).
- **Smoke test of the whole bot** (Python bot stopped): connect as Lil Deets, logic Ready,
  command sync, a rebuilt logic swapped in ~35 ms. Two supervisor bugs found and fixed.
  **No command was tried in Discord**; the bot was stopped again.
- **His decisions:** layout C for the queue (built in the Python bot too; `/dm preview`
  deleted); `/links limit:N` reads the newest N in both bots (the journal's scan keeps the
  first N); Lil Deets' own token for the Rust side (a copy in `Documents/Deets' Secrets/`);
  twilight 0.18 allowed before it is two weeks old; test the Rust logic with the Python bot
  stopped.
- **Also:** the `deetsmusic-dev` MCP server is the CLI's debug build and never pairs with
  the installed app; a `deetsmusic` server on the release CLI was added (user config). The
  CLI's misleading "isn't running" message is for a DeetsMusic session to evaluate.
- **Left open:** the Discord desk tests of the Rust bot; the host's song-boundary restart
  and resume-file writing; the docs rewrite; twilight's advisories before the switch.
  Nothing on the branch since P0 is committed.

## 2026-11-10 — Lil Deets joins voice; /dm; the repo is made

- **His words.** "Can we wire Lil Deets (the disc bot) to be able to join vc and output audio?
  It can tap from DeetsMusic / any specific windows app of mine." Rules: current song pulls and
  OTD scripts unaffected; no DeetsMusic code touched.
- **The capture.** No driver (DeetsAudioDriver is tabled for a certificate): DeetsAirplay's
  per-process loopback, as a helper `deets-tap` that writes PCM to stdout. Later moved to
  48 kHz directly (`Capture::start_process_at`, added to the crate).
- **Three causes of silence, found by measurement:** DeetsMusic at 17% in the Windows mixer
  (peak 91), then davey 0.2.0 (DAVE never ready; pinned <0.2), with a wrong-process scare on
  the way (DeetsMusic's CLI/MCP share its exe name). Tools made for it: `--meters`, `/vc tone`
  with a DAVE readout.
- **He routed DeetsMusic to the Dell monitor at 100%** so only the call hears it.
- **/dm** on DeetsMusic's bridge, permissions from its Rooms guest rules, native Components V2
  cards in the theme's accent (mockups first; image cards to be tried later). Then: the np
  card edits only on a change; an add says its row and "plays in"; an Explicit/Clean column;
  the voice channel status and "Listening to"; Opus in music mode without FEC.
- **Decisions:** everything Discord in one new public repo, DeetsOTD untouched until the
  migration is confirmed; deets-tap into `crates/tap` with drift steps; the harness bot
  retired; a tray app is the release shape; a Rust rewrite later.
- **The move.** Cloned DeetsOTD (36 commits), `git mv` into `bot/` and `journal/`, relative
  imports, root-relative paths, `python -m` entry points; deets-tap moved with an `--info`
  contract the bot checks. Side-by-side: songs.json and movies.json identical from both repos'
  data; a live scan and RSS run clean from here. Harness guild commands cleared. Docs written.
- **Not verified:** the status line and the column width in Discord; the 48 kHz capture by ear;
  a nightly run from here.
