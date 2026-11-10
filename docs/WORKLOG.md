---
status: sop
desk_test: none
sources: []
updated: 2026-11-10
---
# DeetsDiscordBot — Work log

One dated `##` per sitting, newest first. An old entry is never corrected. DeetsOTD's earlier
history is in `git log` and docs/archive/.

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
