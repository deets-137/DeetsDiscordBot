---
status: idea
desk_test: none
sources: [bot/app.py, bot/voice.py, bot/dm.py, bot/dm_bridge.py, bot/live.py, journal/linkscan.py, crates/tap/src/main.rs, scripts/parity/capture_inputs.py, scripts/parity/render_expected.py]
updated: 2026-11-10
---
# The Rust port: build plan for agents

> **Status:** idea · 2026-11-10. Not started. The *why* and the stack are in
> [rust-rewrite.md](rust-rewrite.md) (§3 is decided). This doc is the *how*: the shape, the
> behaviour to match, and work packages a coding agent can take one at a time. §1 is the owner's.

## 1. What he said (2026-11-10)
"can you do some more scoping work for the rust rewrite while we're here so we can have sonnet
agents write the code later". The stack: "use pieces of twilight as intended and keep songbird,
handrolling where we can gain more functionality or resource savings." The journal stays Python.

## 2. Rules for every agent
1. **The Python bot is the specification.** Where this doc and `bot/*.py` disagree on behaviour,
   the Python wins; tell the owner. Read the feature docs first: architecture/BRIDGE.md,
   architecture/PERMISSIONS.md, features/DM.md, features/CARDS.md, features/STATUS.md,
   features/VC.md, architecture/VOICE-PIPELINE.md, architecture/LIVE-RELOAD.md.
2. **The parity kit is the test** (§4). A work package is done when its parity tests pass.
3. **Never run on Lil Deets' token.** Use a separate test application and its own token
   (`.env`: `RUST_DISCORD_TOKEN`). Two processes syncing commands on one application overwrite
   each other (architecture/BOT.md §2).
4. **Never hand-roll DAVE, MLS, Opus or transport crypto.** songbird does voice.
5. **No new crate beyond §3's list without the owner's word.** Check a crate's license (MIT or
   Apache-2.0) and that its newest release is at least two weeks old.
6. **Windows first.** Code that only builds on Windows sits behind `cfg(windows)`.
7. **Never edit `bot/` or `journal/`** (the Python bot runs from them live and reloads on save).
   Never edit `../DeetsMusic` or `../DeetsAirplay`.
8. All packages go on one branch, `rust-rewrite`, built in the main session with no agents
   (the owner, 2026-11-10). Commit only when the owner asks; the owner merges.
9. **Never block the async runtime.** Blocking I/O (the capture's reads, file writes, the
   bridge if it uses `ureq`) runs in `tokio::task::spawn_blocking` or its own named thread.
   For the tray app later: a synchronous `#[tauri::command]` runs on the UI thread, so it is
   `async fn` + `spawn_blocking` (DeetsMusic › Conventions; 0.12.0 froze on live this way).
10. **Name every long-lived thread** (`std::thread::Builder::name`) and list it in a thread
    table in the crate's doc comment: what it does, how long it lives (DeetsMusic › threads).
11. **Files we cannot lose half of** (the resume hand-off) are written beside, flushed, then
    renamed over (DeetsMusic's `safe_file::write_atomic`).
12. **Log what acts on its own** with `tracing`: arm, fire, stop (CLAUDE.md › Working style).

## 3. The shape

```
Cargo.toml                       workspace: crates/*
crates/
  tap/            (exists)       deets-tap: becomes lib + bin. The lib adds TapStream (§5 P2)
  ipc/            lildeets-ipc   host ⇄ logic messages (serde) and framing
  rest/           lildeets-rest  hand-rolled Discord REST: our ~20 routes, rate limits, multipart
  cards/          lildeets-cards pure: text formats + Components V2 builders (no I/O)
  bridge/         lildeets-bridge DeetsMusic bridge client
  host/           lildeets-host  BIN: gateway + voice + supervises logic. Rarely changes.
  logic/          lildeets-logic BIN: commands, cards, permissions, watchers. Restarted at will.
tests/parity/                    (exists) the kit: input/, expected/
```

### 3.1 The two processes (why: live reload — architecture/LIVE-RELOAD.md §8)
**lildeets-host** holds everything a restart would cut: the gateway (twilight-gateway 0.18), the
voice connections (songbird driver + the glue), the capture streams, presence. It starts
**lildeets-logic** as a child and talks to it over IPC. When `lildeets-logic.exe` changes on
disk (a rebuild), the host starts the new logic, waits for its `Hello`, then stops the old one.
Commands and cards change with **no gap**; the music never stops.

A change to the host itself is the song-boundary restart, as today: a small launcher
(`lildeets.exe`, or the tray app later) restarts the host on exit code 75; the host saves and
resumes its voice state the same way `bot/live.py` does (`.lildeets-resume.json`, same fields).

**lildeets-logic** handles every interaction. It calls Discord's REST API itself (with the
token passed by the host), and asks the host for everything voice or gateway.

### 3.2 IPC (lildeets-ipc)
- Transport: the host binds `127.0.0.1:0` (the OS picks a free port: never a fixed one) and
  passes `LILDEETS_IPC=127.0.0.1:<port>` and a random 32-byte hex `LILDEETS_IPC_TOKEN` to the
  child. The child's first frame is `Hello { token, version }`; anything else closes the socket.
- Framing: 4-byte little-endian length, then UTF-8 JSON. One enum per direction, `#[serde(tag = "t")]`.

| Host → logic | Meaning |
|---|---|
| `Ready { app_id, bot_user_id, guilds }` | after the gateway's READY (and on every new logic) |
| `Dispatch { event: <raw gateway dispatch JSON> }` | INTERACTION_CREATE (the logic needs nothing else) |
| `VoiceSnapshot { guilds: [{guild, bot_channel, tapping_app, gain, dave_ready, members_in_bot_channel: [user ids]}] }` | on every change, and on request |
| `MemberVoice { request_id, user, guild, channel }` | the answer to `WhereIs` |
| `Done { request_id, error }` | the result of a voice op |

| Logic → host | Meaning |
|---|---|
| `Hello { token, version }` | first frame |
| `WhereIs { request_id, guild, user }` | which voice channel a member is in (host caches voice states) |
| `Play { request_id, guild, channel, app }` · `Tone { … }` · `Stop` · `Leave` · `SetGain { guild, gain }` | voice ops (`voice.start_stream` semantics) |
| `Presence { listening: Option<String> }` | "Listening to …" (gateway op 3) |
| `Snapshot { request_id }` | ask for a `VoiceSnapshot` now |

### 3.3 Crates allowed (rust-rewrite.md §3)
twilight-gateway 0.18, twilight-model 0.18, songbird 0.6 (features `driver`, `rustls`; **not**
`serenity`/`twilight`/`gateway`), tokio, serde + serde_json, reqwest (rustls, multipart) or
ureq for the bridge, tracing, deets-airplay (git rev, as crates/tap pins it), windows (for the
tap only). Anything else: ask.

## 4. The parity kit (tests/parity/)
Built from the Python bot, so the port is checked against what runs today.
- `input/` — fixed inputs: Apple Music **catalog search** results (public), a 24-song queue made
  from them (explicit, clean, unrated, a 20-minute song), now-playing, Rooms rules, theme,
  made-up `/links` messages. Never the owner's real queue: this repo is public.
- `expected/` — what Python produces from them, with the clock frozen at
  `cases.json.frozen_now` (1794340000) and discord.py's random custom_ids replaced by
  `"<custom_id>"`:
  - `commands.json` — every command's registration payload. **The Rust bot registers exactly
    this** (except `dm preview`, which goes once the owner picks a queue layout).
  - `cards/*.json` — the Components V2 `components` arrays (np playing / paused / nothing; queue
    pages 1, 2, 3 and 9 (clamped); queue empty; the picker).
  - `text/*.txt` — suggestion rows per search; help text (host, guest in the call, guest with no
    stream); the "added" lines (later / next / a long song × playing / paused).
  - `cases.json` — `confident`, `tag`, `rating_word`, `column_width`, `_bar`, `mmss`, `_clip`.
  - `links/` — records, the Markdown, the CSV, and the file names. **Both files end lines with
    `\r\n`**, as the Python bot writes them on Windows (the CSV from the csv module, the
    Markdown from text mode). The kit is stored with no line-ending conversion
    (`.gitattributes`), so compare bytes.
- Regenerate: `python scripts/parity/render_expected.py` (offline). Refresh the inputs only with
  `python scripts/parity/capture_inputs.py` (needs DeetsMusic). Commit both with the change.
- **Rust side:** each crate's tests load these files and compare: JSON equal after replacing
  every `custom_id` with `"<custom_id>"`; text equal byte for byte.

## 5. Work packages
Each is one agent session. "Reads" are the files the agent must read first. Dependencies:
P0 → (P1, P2, P3, P4 in parallel) → P5 → P6 → P7 → P8.

### P0 · Workspace and the parity loader
- **Build:** root `Cargo.toml` workspace; move nothing in `crates/tap` yet; an internal test-support
  module (in `crates/cards/tests/support.rs`, copied where needed) that loads `tests/parity/*` and
  normalizes `custom_id`s.
- **Done when:** `cargo test --workspace` runs (and `crates/tap` still builds and prints `--info`).
- **Built 2026-11-10** (branch `rust-rewrite`). Decided inside the package:
  - `crates/tap` is **excluded** from the workspace until P2. A member builds into the root
    `target/`, but the running bot runs `crates/tap/target/release/deets-tap.exe`
    (`bot/voice.py` `DEFAULT_TAP`). P2 moves the tap in, and that path changes with it.
  - The loader is `crates/cards/tests/support/mod.rs`, not `tests/support.rs`: Cargo builds
    every `tests/*.rs` file as its own test binary.
  - `tests/parity_kit.rs` checks the loader: every kit file loads, the `/links` files keep
    `\r\n`, `custom_id`s are normalized at any depth, and a changed card or a `\n` text fails.

### P1 · lildeets-cards (pure; the most parity in the least risk)
- **Reads:** `bot/dm.py` (everything above `register`), features/CARDS.md, features/DM.md §2–3,
  `tests/parity/expected/{cases.json,text/,cards/}`.
- **Build:** pure functions, no I/O: `clip`, `words`, `mmss`, `rating_word`, `tag(track, Option<width>)`,
  `suggestion(track, width)`, `column_width`, `confident(term, songs)`, `bar(progress)`,
  `help_text(rights, host, session)`, `added_line(user, track, next, queue, now_playing, now)`;
  and Components V2 builders with **twilight-model 0.18 types**: `np_card`, `queue_card(page)`,
  `picker_card`. Deterministic custom_ids: `dm:np:<action>`, `dm:queue:<page>`, `dm:pick:<n>:<next 0|1>:<catalogId>`.
- **Mind:** the figure / thin / hair spaces in `_WORD_PAD` and `FIGURE_SPACE` are exact
  characters (U+2007, U+2009, U+200A); `mmss` is `int(round(seconds))` in Python, which rounds
  a `.5` to the even number (banker's rounding: Rust's `f64::round_ties_even`, not `round`), and
  `None` is `"–:––"` (en dashes); `clip` uses "…" (U+2026); the queue card must stay ≤ 40
  components.
- **Done when:** every file in `expected/cases.json`, `expected/text/`, `expected/cards/` is
  reproduced exactly.

### P2 · The tap as a library (crates/tap)
- **Reads:** `crates/tap/src/main.rs`, architecture/VOICE-PIPELINE.md, ops/TAP.md, songbird 0.6.1's
  `input/adapters/raw_adapter.rs` and `RawReader` (raw f32 LE, header `SbirdRaw` + rate + channels).
- **Build:** `lib.rs` with `resolve_exe`, `list_apps`, `meters` (moved from main.rs) and
  `TapStream`: a `std::io::Read` + `symphonia::core::io::MediaSource` (non-seekable) over
  `Capture::start_process_at(pid, 48_000)` that blocks on a 20 ms schedule and yields f32 LE
  stereo (i16 / 32768.0), silence while the app is quiet, priming as `stream()` does. songbird
  wraps it: `RawAdapter::new(tap, 48_000, 2)`. `main.rs` keeps every flag working.
- **Done when:** `deets-tap --info/--list/--meters/--exe` behave as before; a test reads 2 s from
  `TapStream` over a stub ring and gets 2 s of samples, on pace (± 5 %).

### P3 · lildeets-rest (hand-rolled)
- **Reads:** this §, DISCORD-LIMITS.md, Discord's docs for each route.
- **Build:** a client with the bot token, per-route buckets from `X-RateLimit-Bucket` /
  `-Remaining` / `-Reset-After`, global 429 with `retry_after`, and these calls (typed with
  twilight-model where a type exists, else `serde_json::Value`):
  `get_current_application` (owner / team ids) · `put_global_commands(body)` ·
  `interaction_callback(id, token, body)` (types 4, 5, 6, 7, 8 — message, deferred, deferred
  update, update, autocomplete) · `edit_original(app, token, body)` · `delete_original` ·
  `followup(app, token, body)` · `edit_message(channel, message, body)` ·
  `create_dm(user)` · `create_message(channel, body, files)` (multipart, for `/links`) ·
  `channel_messages(channel, before, limit)` (history, oldest-first by paging) ·
  `put_voice_status(channel, status)` (`PUT /channels/{id}/voice-status {"status"}`).
  Components V2 bodies carry `flags: 1 << 15` (IS_COMPONENTS_V2) and **no** `content`.
- **Done when:** unit tests build every request (method, path, headers, body) without a network;
  a rate-limit test drives the bucket logic with recorded headers.

### P4 · lildeets-bridge
- **Reads:** `bot/dm_bridge.py`, architecture/BRIDGE.md.
- **Build:** the same client: ports 47825–47828 (first that answers, cached, retried on failure),
  the token from `%APPDATA%\com.deetsmusic.app\settings.json` (`bridgeToken`, re-read once on
  401), the same routes and caches (search 2 min, rules 10 s, accent 60 s), `THEME_ACCENT`, and
  `BridgeError` with DeetsMusic's own message. **Only the routes in BRIDGE.md §2.**
- **Done when:** tests against a local stub server pass, including the port fail-over and the 401
  re-read.

### P5 · lildeets-ipc + lildeets-host (gateway and voice)
- **Reads:** §3.1–3.2, architecture/VOICE-PIPELINE.md, architecture/LIVE-RELOAD.md, songbird's
  `driver` (`Driver::new(Config)`, `Driver::connect(ConnectionInfo)`), twilight-gateway 0.18.
- **Build:** the gateway (intents: guilds, guild voice states, guild messages, message content);
  a voice-state cache; the songbird glue: send op 4 (Update Voice State, `self_deaf: true`),
  collect our VOICE_STATE_UPDATE (`session_id`) and VOICE_SERVER_UPDATE (`token`, `endpoint`),
  build `ConnectionInfo`, `Driver::connect`; play = `Input::from(RawAdapter::new(TapStream, 48_000, 2))`
  with the track volume = gain; the tone; leave when the bot is alone in its channel; presence
  (op 3, "Listening to"); spawning and replacing lildeets-logic on a rebuild; the IPC server;
  the song-boundary self-restart and `.lildeets-resume.json` (live.py's fields and rules).
- **Done when:** on the **test application**: the host joins a channel, plays DeetsMusic with
  DAVE (the call hears it), leaves when alone, and survives a logic rebuild without a gap. The
  owner confirms by ear.

### P6 · lildeets-logic: commands and permissions
- **Reads:** `bot/app.py`, `bot/voice.py` (register), `bot/dm.py` (register, check),
  architecture/PERMISSIONS.md, `expected/commands.json`.
- **Build:** register `expected/commands.json` (minus `dm preview`) at start; dispatch by
  command name / subcommand / `custom_id`; `check(action)` exactly as PERMISSIONS.md (ask the host
  with `WhereIs` / `VoiceSnapshot`); every `/vc`, `/dm` and `/links` command with the same
  replies, ephemerality and deferrals as Python; autocomplete (`/vc play` from the host's app
  list, `/dm add|play` from the bridge, 2.5 s budget); the picker; `/links` with the DM and its
  fallback (port `extract_from_message` and `write_outputs`; match `expected/links/`).
- **Done when:** every command works on the test application, and the `links/` parity passes.

### P7 · lildeets-logic: watchers
- **Reads:** features/STATUS.md, features/CARDS.md §4, `bot/dm.py` (`StatusWatcher`, `refresh`).
- **Build:** the status watcher (5 s; edit only on change; voice status ≤ 500 chars via REST,
  presence via the host; clear on stop) and the np card watcher (5 s; edit only on song or
  play-state change; 3 h; one per channel; through `edit_message`, not the interaction token).
- **Done when:** the desk tests in STATUS.md §3 and CARDS.md §4 pass on the test application.

### P8 · The launcher and the switch-over
- **Build:** `lildeets.exe` (restart the host on 75), the docs (BOT.md, LIVE-RELOAD.md, SETUP.md
  rewritten for Rust), and a side-by-side week on the test application.
- **The switch:** the owner's call. Then Lil Deets' token moves to the Rust bot and `bot/` goes
  to `docs/archive/` as a record. `journal/` stays.

## 6. Open (to settle in P5, on evidence)
- songbird ↔ twilight: this plan uses songbird's `driver` with our glue on twilight 0.18
  (rust-rewrite.md §3); if the glue misbehaves, fall back to twilight 0.17 with songbird's
  `twilight` feature.
- The queue layout: whatever the owner picks from `/dm preview` (A, B, C, D); P1 re-renders
  the kit after the Python bot adopts it.
