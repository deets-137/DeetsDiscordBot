# DeetsDiscordBot — project guide for Claude

Everything Discord for the Deets family, in one repo:
- **Lil Deets, the bot** (`bot/`, Python, discord.py): `/vc` plays one Windows app (usually
  DeetsMusic) into a voice channel; `/dm` lets the people in that call control DeetsMusic;
  `/links` reads a channel's links.
- **The journals** (`journal/`, Python): the Song-of-the-Day channel and the Letterboxd film
  diary, kept in `storage.db` and exported as `songs.json` / `movies.json` for deets.solutions
  (`../DeetsSolutions`) by a nightly job.
- **deets-tap** (`crates/tap`, Rust): the capture helper `/vc` runs, built on DeetsAirplay's
  `deets-airplay` crate.

Started 2026-11-10 as a clone of `../DeetsOTD` (all its history). The nightly job ran from here
and passed the same day, and DeetsOTD was archived (docs/ops/NIGHTLY.md §4).

## Start here
- **`docs/HANDOFF.md`** — cold start: state of play, **Open now**, gotchas. Corrected in place.
- **`docs/WORKLOG.md`** — one dated `##` per sitting, newest first. Never correct an old entry.
- **`docs/DESIGN.md`** — the overview: what the bot is for, the owner's decisions, the shape.
- **`docs/LESSONS.md`** — what the owner tends to want. It shapes the option you RECOMMEND.
  It never closes a fork. Read `../DeetsMusic/docs/LESSONS.md` and
  `../DeetsMusic/docs/TASTE.md` with it (TASTE.md is local only: never quote it in a doc or a
  commit).
- **`docs/DOCS-ORG.md`** — how the docs are organized.

## The docs
**State lives in each doc, never in this file.** Every doc starts with front matter
(DOCS-ORG.md §2); a part with its own state carries a part marker. A new doc gets front matter
and one line below.

**`docs/architecture/`** — the engine
- `BOT.md` — the launcher and the bot process, the one command tree, intents, the token
  shared with the journal, how modules plug in (`register(client, tree)`), logging (`bot/app.py`).
- `LIVE-RELOAD.md` — code goes live by itself: `/dm` reloads in place, the rest restarts at a
  song boundary (`bot/live.py`). Never keep voice state in `dm.py`.
- `VOICE-PIPELINE.md` — app → per-process loopback → 48 kHz PCM → Opus (music mode) → DAVE →
  Discord; why davey is pinned below 0.2 (`bot/voice.py`, `crates/tap`).
- `BRIDGE.md` — DeetsMusic's loopback bridge as the bot uses it: ports, token, routes, caches.
  Read-only toward DeetsMusic's code (`bot/dm_bridge.py`).
- `PERMISSIONS.md` — host versus guest; DeetsMusic's Rooms guest rules decide what a guest may
  do (`bot/dm.py` `check`).
- `STORAGE.md` — `storage.db`: the song tables and the film tables, their only writers, the
  idempotent keys (`journal/storage.py`, `journal/letterboxd_store.py`).

**`docs/features/`** — one surface or one feature
- `VC.md` — `/vc play | tone | stop | leave`; who may use it; leave-when-alone.
- `DM.md` — every `/dm` command, the confidence rule for `/dm add`, the queue line on an add.
- `CARDS.md` — the Components V2 cards: layout, the 40-component limit, the accent, the column,
  the edit-on-change rule. Never edit a card on a timer.
- `STATUS.md` — the voice channel status and the bot's "Listening to" (`StatusWatcher`).
- `LINKS.md` — `/links`: a channel's links as Markdown + CSV in a DM.
- `JOURNAL.md` — the Song-of-the-Day pipeline: scan → archive → enrich → review → songs.json.
- `FILMS.md` — the Letterboxd film journal: import, RSS, TMDB posters → movies.json.
- `TRAY.md` — a tray app that starts the bot with one click. Project: not built.

**`docs/ops/`** — build, run, diagnose
- `SETUP.md` — a fresh machine: Python packages, `.env`, `config.py`, the Discord portal, the
  invite permissions, building deets-tap.
- `TAP.md` — deets-tap: build, flags, the `--info` contract, the drift rules with DeetsAirplay.
- `NIGHTLY.md` — the 6 PM job that publishes songs.json / movies.json, and the cut-over from
  DeetsOTD (its checklist).
- `DEBUGGING.md` — no sound in the call, a quiet stream, commands missing, the bridge down.
- `DISCORD-LIMITS.md` — the Discord limits the bot designs around.

**`docs/guide/`** — `USAGE.md`: running the journal by hand (scan, review, export, Notion).
**`docs/integrations/`** — `APPLE-MUSIC.md`: the Apple Music API in `enrich.py` and `review.py`.

**`docs/ideas/`** — not built; never tell a user the bot does these. §1 is what he said.
- `in-call-extras.md` — the card in the call's own chat, vote-to-skip.
- `activity.md` — a Discord Activity: DeetsMusic's Rooms look inside the call.
- `image-cards.md` — rendered PNG cards in DeetsMusic's theme, next to the native ones.
- `auto-route.md` — `/vc play` moves the app to a silent output by itself.
- `friends-tapping.md` — why friends do not tap their own apps through this bot.
- `rust-rewrite.md` — the scope of a Rust port: part by part, measured costs, benefits, order.
- `rust-port-plan.md` — the build plan for agents: the two-process shape, IPC, work packages
  P0–P8, the parity kit in `tests/parity/` (`scripts/parity/`) that each package must pass.

**`docs/archive/`** — DeetsOTD's June vision and handoff, kept as a record.

## Never
- **Never edit `../DeetsMusic` from this project.** The bot reaches DeetsMusic only through
  its bridge (BRIDGE.md). Never expose a library, playlist, settings, Diary or Song of the Day
  write to Discord.
- **Never edit or run `../DeetsOTD`.** It is archived (2026-11-10); its `storage.db` is the
  frozen backup from the move. The live database is this repo's (NIGHTLY.md §4).
- **Never run a second bot process on this token that syncs slash commands.** Discord keeps
  one global command list per application; the last process to sync wins (BOT.md §2). The
  journal's `scan` logs in briefly and never syncs; keep it so.
- **Never let a guest do what DeetsMusic's Rooms rules don't allow**, and never let a guest act
  outside the bot's voice channel or while the host is not playing DeetsMusic (PERMISSIONS.md).
- **Never raise davey to 0.2 or above with discord.py 2.7.x.** The DAVE session never gets
  ready and the call hears nothing (VOICE-PIPELINE.md §4).
- **Never keep voice state in `bot/dm.py` or `bot/dm_bridge.py`.** They are reloaded while the
  music plays (LIVE-RELOAD.md §5). Before a change across several bot files, create
  `.lildeets-hold`; delete it when done.
- **Never edit a Discord message on a timer.** Edit on a change only (CARDS.md §4).
- **Never change deets-tap's stdout format without bumping `CONTRACT`** in
  `crates/tap/src/main.rs` and `EXPECTED_TAP` in `bot/voice.py` together (TAP.md §3).
- **Never commit `.env`, `config.py`, `storage.db`, `exports/` or `secrets/`.** The repo is
  public. The bridge token lives in DeetsMusic's settings file and is never copied here.
- **Never write songs.json or movies.json with private Discord ids** (message id, author id,
  jump link). `storage.export_web` drops them; keep it so.
- **Never add a dependency without asking.**

## How to verify your work
- **The user runs the bot and tests in Discord.** Do not start a second bot process to test:
  it would answer interactions too and re-sync commands. The running bot picks up a saved
  change by itself (LIVE-RELOAD.md): a `/dm` change at once, anything else at the next song.
- Cheap checks that ARE worth running:
  - `python -c "import bot.voice, bot.dm, journal.scan, journal.review, journal.letterboxd_web"`
  - `cd crates/tap && cargo build --release`, then `target/release/deets-tap.exe --info`
  - build a card offline: `asyncio.run(bot.dm.queue_view(client, 1))` and count components
  - the bridge: `bot.dm_bridge.Bridge().now_playing()` against the running DeetsMusic
- A journal change: export from a test copy of `storage.db` and compare with the published
  JSON, minus `generated_at` (NIGHTLY.md §3 has the script).
- Hand over with the desk test written into the feature's doc.

## Working style (the user directs the architecture)
These follow DeetsMusic's CLAUDE.md › Working style (2026-09 to 2026-10), keeping what applies
to a bot; its UI rules (tokens, motion, hints, toasts, settings keys, the Compass, the CSP,
the shots runner, publishing) do not apply here.
- **Design on paper first.** Surface the real forks, confirm, then build. Follow
  `~/.claude/CLAUDE.md` › Bringing forks: open with the count, recommendation first.
- **He decides every real fork** (DeetsMusic, 2026-09-18). A default, a wording, a reply line, a
  card layout, a command shape, a permission: bring the fork before you build it. One option
  left: build it, and say so in one line. Reason: on 2026-09-18 a badge he never saw shipped to
  DeetsMusic's users. LESSONS.md and TASTE.md shape the option you RECOMMEND; they are never
  permission to skip the question.
- **The end user first** (here: the people in the call), then maximum security that still
  feels great. Hard or complex work is not a cost. Prefer the most user-friendly and
  user-empowering option.
- Inside a fork he HAS chosen, build the full, polished thing without asking again; "he
  decides" never means "build less". At hand-off, list what you decided inside his choice.
- **One load-bearing feature in flight** (DeetsMusic, 2026-09-17). Hand it over, he desk-tests
  it, then the next starts. Clear the open desk tests (HANDOFF.md › Open now) before new work:
  untested features stacked in one tree hide each other's mistakes.
- **The pattern first** (DeetsMusic, 2026-10-26). Before you design a command, card, reply or
  button, find how the bot already does that kind of thing (or how DeetsMusic does it, for
  anything its Rooms, cards or wording already decided), build on it, and name the precedent.
- **Log what acts on its own.** Anything that runs without a command (the status watcher, the
  np card watcher, live reload, the nightly job) logs when it arms, fires and stops, under
  `discord.lildeets.*`.
- **Stewardship.** Keep calls to the bridge, Apple and Discord few: edit on change, cache what
  repeats, and ask the cost before a design that polls or fetches a lot.
- **A file we cannot lose half of** (the resume hand-off, settings, tokens, `storage.db` copies)
  is written beside, flushed, then renamed over the old one (DeetsMusic's `safe_file`).
- **Agents.** Build and explore in the main context. The Rust port is built here too, with no
  agents, on the `rust-rewrite` branch (his word, 2026-11-10; docs/ideas/rust-port-plan.md).
  One standing use of a Sonnet agent (`model: "sonnet"`): after a feature is built, a pass
  asking how a user might break it from a UX or workflow view (DeetsMusic, 2026-10-17). Read every diff an agent makes
  and run the checks yourself: an agent's report is not proof.
- **Commit only when he asks.** Co-author trailer: name the model that did the work (today
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`); update this line when it changes.
- **Several Deets apps run on this PC at once.** Never hardcode a port with strict failure.

## How to explain things to me
- Write in ASD-STE100 (Simplified Technical English): short sentences, active voice, one idea
  per sentence, plain approved words, no metaphor.
- Describe a thing by what he sees in Discord, not by an internal name. Name the exact
  control ("the ⏭ button on the now-playing card", "the voice channel's status line").
  Define the terms once, at the start.
- Test each option against the code BEFORE you show it. Discard the options the code already
  rules out. Show the real forks only.
- A list, finding or count he asked for goes in the chat text, in full, before the end-of-turn
  box.

## Run
```
pip install -r requirements.txt -r requirements-voice.txt
cd crates/tap && cargo build --release && cd ../..
python -m bot                                   # the launcher + bot (from the repo root)
python -m journal.scan <channel> --enrich --web --web-out ../DeetsSolutions/sotd/songs.json
python -m journal.letterboxd_web rss --web -o ../DeetsSolutions/movies/movies.json
python -m journal.review list                   # songs flagged for hand-checking
crates/tap/target/release/deets-tap.exe --meters   # every app's level and mixer volume
```
