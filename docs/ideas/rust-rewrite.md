---
status: idea
desk_test: none
sources: [bot/app.py, bot/voice.py, bot/dm.py, bot/dm_bridge.py, bot/live.py, crates/tap/src/main.rs]
updated: 2026-11-10
---
# A Rust rewrite

## 1. What he said (2026-11-10)
"Eventually we can talk about rewriting in Rust for efficiency but not a priority for now."
Then: "Can you scope out the rust rewrite across the board please? Just scoping for now, what
benefits might it provide?" Then: "explore what Discord's Rust libraries offer for feature
parity to current live. Journal can remain in python."

## 2. Measured today (the Python bot, streaming DeetsMusic into a call)
| Process | RAM | CPU (one core) |
|---|---|---|
| python (the bot) | 57 MB | 2.5 % |
| deets-tap (the capture) | 11 MB | 1.7 % |

Code: bot 1,690 lines of Python; journal 3,131 lines of Python; deets-tap 282 lines of Rust.

## 3. The Rust libraries (checked 2026-11-10, in their published source)
Each library's released source was downloaded from crates.io and searched for every feature
the bot uses today, so this table is not from documentation.

| Library | Newest release | Notes |
|---|---|---|
| **serenity** | 0.12.5 (2025-12-20) | The gateway, HTTP and models most bots use. **Components V2 is on its `next` branch only** (PR #3123, merged 2025-06-29): no released version has it. |
| **poise** | 0.7.1 (2026-10-12) | The command framework on serenity (`^0.12.5`): slash commands, groups, autocomplete. |
| **twilight** (model, http, gateway) | 0.18.0 (2026-11-01) | Lower level, modular. **Full Components V2** (container, section, thumbnail, text display, separator, media gallery, labels…); fixes the empty-`content_type` bug of 0.17.1 (issue #2591). |
| **songbird** | 0.6.1 (2026-10-10) | Voice. **DAVE since 0.6.0** through `davey ^0.1.2` (the line the Python bot pins). Integrates with serenity `^0.12` and with **twilight only up to 0.17**; its `driver` feature works with any gateway if we pass it the voice events ourselves. |

### Feature parity with the bot as it runs today

| Feature (where it is today) | serenity 0.12.5 + poise + songbird | twilight 0.18 + songbird (driver only) |
|---|---|---|
| Slash command groups, options, ranges, booleans (`/vc`, `/dm`) | ✅ poise | ✅ twilight-model (no framework: our own routing) |
| Autocomplete, 25 choices (`/vc play`, `/dm add`) | ✅ poise | ✅ |
| Defer, ephemeral, follow-up, edit / delete the original | ✅ | ✅ |
| **Cards: container + accent, section + thumbnail, text display, separator** (CARDS.md) | ❌ not released → send the card as raw JSON through serenity's HTTP client (it takes any serializable body) | ✅ typed |
| Buttons, string select (picker, ◀ ▶, layout D) | ✅ (inside raw-JSON cards: by `custom_id`) | ✅ |
| Component callbacks | by `custom_id` (no view objects as in discord.py) | by `custom_id` |
| Editing a card with the bot token (np watcher) | ✅ | ✅ |
| **Voice channel status** (STATUS.md) | ✅ `edit_voice_status` (`EditChannel`) | ❌ no route → one raw `PUT /channels/{id}/voice-status` |
| Presence "Listening to" | ✅ | ✅ |
| Application info (host check), voice state events (leave when alone) | ✅ | ✅ |
| DM a user with files (`/links`) | ✅ | ✅ |
| Join self-deafened, move, leave | ✅ songbird, first-class with serenity | ✅ songbird driver + our glue for the voice server/state events |
| **DAVE** | ✅ songbird 0.6 | ✅ songbird 0.6 |
| Raw 48 kHz PCM in, no helper process (deets-tap) | ✅ a `MediaSource` fed by `deets-airplay`'s capture, wrapped in `RawAdapter` | ✅ same |
| Opus for music, 128 kbps, no FEC (`voice.ENCODER`) | ✅ songbird's mixer: `CodingMode::Audio`, 128 kbps default, `set_bitrate` | ✅ same |
| Volume (`/dm volume`) | ✅ track volume | ✅ |
| Bridge to DeetsMusic (dm_bridge.py) | reqwest / ureq | reqwest / ureq |
| Live reload (live.py) | two processes (§4) | two processes (§4) |

**Nothing is missing outright.** Each stack has one gap that becomes a small hand-written
HTTP call: serenity lacks the cards' types, twilight lacks the voice-status route. Twilight
also needs the voice glue that songbird gives serenity for free.

### The stack (the owner, 2026-11-10)
None of these crates is Discord's: Discord publishes no bot library in any language. Its one
official piece in this area is `libdave`, the C++ reference for DAVE; `davey` is a community
Rust implementation of it.

**Decided:** use **twilight's pieces as they are meant to be used**, **keep songbird** for
voice, and **hand-roll where it gains functionality or saves resources.**

| Layer | Choice | Why |
|---|---|---|
| Models (payloads, Components V2) | `twilight-model` | full Components V2, typed |
| Gateway (WebSocket, heartbeat, resume) | `twilight-gateway` | reconnect and resume are where hand-rolled bugs hide |
| REST (~20 routes we use) | **hand-rolled**, typed with `twilight-model` (or `twilight-http` where it already fits) | exactly our routes; the voice-status route `twilight-http` lacks; one rate-limit layer |
| Commands, autocomplete, component routing | **hand-rolled** dispatch by command name and `custom_id` | no framework needed for ~20 commands; same shapes as today |
| Voice (voice gateway, UDP, RTP, transport crypto, Opus, DAVE) | **songbird** 0.6 (`davey`, `opus2`) | real-time networking and cryptography: never hand-rolled. Its mixer's defaults already match `voice.ENCODER` |
| songbird ↔ twilight | songbird's `driver` feature + our glue (pass it the voice state / server events), since its `twilight` feature stops at twilight 0.17 — **or** twilight 0.17 with that feature | the spike picks: the glue keeps twilight current; 0.17 is less code |
| Capture | `deets-airplay` linked in; a `MediaSource` into songbird's `RawAdapter` | no helper process, no pipe, no `--info` contract |
| Bridge to DeetsMusic | a small HTTP client (`ureq` or `reqwest`) | as today |

**Never hand-roll:** DAVE / MLS (use `davey` or `libdave`), the Opus codec (libopus).

The spike (§7 step 1) settles the one open row (the songbird ↔ twilight glue) on a real call.

## 4. Part by part

| Part | In Rust | Gain | Size |
|---|---|---|---|
| **Voice path** (voice.py + deets-tap) | capture → Opus → DAVE in one process, on a real-time thread; no stdout pipe, no `--info` contract | lowest latency (the pipe and the 40 ms hold-back go); no Python garbage-collector or GIL pauses in the audio; one process instead of two | small–medium |
| **/dm + cards** (dm.py, dm_bridge.py) | the same commands and layouts | typed bridge replies; otherwise the same | medium (the largest bot part) |
| **/vc, /links, status** | the same | — | small |
| **Live reload** (live.py) | two processes: a voice host (gateway + voice, rarely changed) and a logic process (commands, cards) it restarts at will; the song-boundary restart stays for the host | the same behaviour; restarts start in milliseconds (the 3–5 s gap is mostly the voice rejoin, so it barely shrinks) | medium: the split is new work |
| **The tray app** (features/TRAY.md) | the bot is a library inside the Tauri app, or one sidecar exe | **no Python to ship**: the release fork in TRAY.md §4 disappears; one installer like DeetsAirplay's; the panel reads the bot's state directly | small once the bot is Rust |
| **The journal** (journal/) | **stays Python** (the owner, 2026-11-10) | — | — |

## 5. Benefits, in order of weight
1. **Release:** one signed exe in the tray app, no Python runtime, no pip, no antivirus
   question about a bundled interpreter. The same stack as DeetsAirplay, DeetsRGB, DeetsMusic.
2. **The audio path:** capture, encode and send in one real-time process. No pipe between two
   processes, no interpreter pauses. Fewer ways for the stream to stutter under load.
3. **One audio codebase:** deets-tap's capture and the bot share a crate; the drift rules in
   ops/TAP.md §3 shrink to a rev pin.
4. **Resources:** expected roughly 15–30 MB and ~1–2 % of a core for everything, against
   68 MB and ~4 % today. **Real but small in absolute terms** on this desk.
5. **Types:** the bridge's replies and Discord's payloads checked at compile time.

## 6. Costs and risks
- **Live reload gets harder** (§4): Python reloads a module in place; Rust needs the
  two-process split.
- **The journal stays Python** (the owner, 2026-11-10). The nightly job calls it as a command;
  the Rust bot's `/links` needs only a channel-history read, which both stacks have.
- **Library gaps** (§3): one hand-written HTTP call either way.
- **Two bots for a while:** the port should run beside the Python bot on a test application
  (its own token), never on Lil Deets' token, until it matches (architecture/BOT.md §2).

> The build plan, with work packages for agents and the parity kit: [rust-port-plan.md](rust-port-plan.md).

## 7. Claude's proposal for the order (not decided; the journal part is his decision)
1. A spike on a test application (its own token), on the stack in §3: twilight gateway +
   models, hand-rolled REST, songbird; `deets-airplay`'s capture straight into songbird;
   `/vc play` with DAVE in a real call; one Components V2 card with a thumbnail; the voice
   channel status. It settles the songbird ↔ twilight row on evidence.
2. The bot (voice, /vc, /dm, status, /links) in Rust, with the voice-host / logic split.
3. The tray app hosts it (features/TRAY.md).
4. **The journal stays Python** (the owner, 2026-11-10).

## Sources
- crates.io: [serenity](https://crates.io/crates/serenity), [poise](https://crates.io/crates/poise),
  [twilight-model](https://crates.io/crates/twilight-model), [songbird](https://crates.io/crates/songbird)
  (versions, dependency ranges, and the source searched for §3)
- [serenity PR #3123: Components V2](https://github.com/serenity-rs/serenity/pull/3123) (merged into `next`)
- [twilight issue #2591](https://github.com/twilight-rs/twilight/issues/2591) (fixed in 0.18.0's source)
- [twilight-components](https://github.com/Kelwing/twilight-components) (Components V2 macros on twilight 0.17)
- [Discord's component reference](https://discord.com/developers/docs/components/reference)
- [songbird releases](https://github.com/serenity-rs/songbird/releases) (v0.6.0: DAVE)
- [songbird PR #314](https://github.com/serenity-rs/songbird/pull/314) (DAVE receive fix)
- [songbird on lib.rs](https://lib.rs/crates/songbird) · [crates.io](https://crates.io/crates/songbird)
- [Resonance](https://github.com/jvz-devx/resonance) and [nexcord](https://github.com/nexverse404/nexcord): Serenity + Songbird bots with DAVE
