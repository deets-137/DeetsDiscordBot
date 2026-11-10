---
status: built
desk_test: open
sources: [bot/live.py, bot/__main__.py, bot/app.py, bot/dm.py, bot/voice.py]
updated: 2026-11-10
---
# Live reload: code changes go live while the music plays

## 1. The ask (the owner, 2026-11-10)
"Any way to hotswap the code / runtime to keep music playing even as slash commands and cards
get updated? Or to set it up such that it occurs when the song pauses / moves to the next song
so it feels seamless?" Chose **automatic**.

## 2. The two processes
- `python -m bot` is the **launcher** (`bot/__main__.py`). It runs `python -m bot.app` and
  starts it again whenever it exits with **75** (`RESTART_CODE`). Any other exit — a crash, a
  Ctrl+C — ends the launcher too, so a broken build never loops.
- `python -m bot.app` is the bot. It can be run alone (no restarts).

## 3. What happens on a change (`bot/live.py`)
Every 2 s the bot reads the mtimes of `bot/*.py` and `journal/{__init__,paths,linkscan}.py`.
A change is acted on once the files have stayed the same for one more look, **and** every
changed file compiles (else: one error line, and the running code stays).

| Changed | What happens | Gap |
|---|---|---|
| only `dm.py` / `dm_bridge.py` | **Reload in place**: `dm.shutdown()` stops the status watcher and the np card watchers; `/dm` comes off the tree; both modules are reloaded; `dm.register` runs again. A new command or option re-syncs the tree. | none |
| anything else | **Restart at a song boundary** (§4) | 3–5 s, between songs |

Cards posted before a reload keep their old buttons (their callbacks are the old code) until
someone posts a new card.

## 4. The song-boundary restart
1. Playing DeetsMusic: wait until the song changes or someone pauses (checked every second).
   Not playing DeetsMusic: go now.
2. If DeetsMusic is playing, pause it through the bridge.
3. Write `.lildeets-resume.json`: each connected voice channel, its app and gain, and
   whether we paused DeetsMusic. Leave voice; close the client; exit 75.
4. The launcher starts the new code. On ready it reads the hand-off (ignored if older than
   3 min), rejoins each channel, starts the same app (`voice.start_stream`), waits until DAVE
   is ready (up to 6 s) so the first seconds aren't lost, and presses play in DeetsMusic if it
   was the one that paused it.

## 5. Rules for the code
- **Never keep voice state in `dm.py` or `dm_bridge.py`.** They are swapped while the music
  plays. The voice connection, the capture, the gain live in `voice.py`.
- **A background task in `dm.py` goes through `_track`** (or `refreshers`), so `shutdown()`
  can stop it before the new code starts its own.
- **A change that spans several files:** create `.lildeets-hold` at the repo root first;
  changes queue while it exists; delete it to apply them together.
- The resume and hold files are gitignored.

## 6. Tested offline (2026-11-10)
In-place reload of `/dm` on a client that never logged in: a new command group and a fresh
bridge; `/vc` untouched; nothing queued. `voice.py` changed → queued for a song-boundary
restart. A file that doesn't compile → refused. The launcher restarts on 75 and stops on 0 or
1. **Not tested:** the restart and resume in a real call (§7).

## 7. Desk test (open)
1. Start with `python -m bot`; `/vc play app:deetsmusic.exe`.
2. Change a string in `dm.py`: within ~5 s the log says "reloaded /dm in place"; the music
   never stops; `/dm help` shows the change.
3. Change a comment in `voice.py`: the log says it waits for the song to end; at the change
   the bot leaves and rejoins within a few seconds and the next song plays from its start.

## 8. If the bot were Rust
Rust can't reload code in place safely. The same behaviour would need two processes: a
small, rarely changed voice host (gateway + voice) and a logic process (commands, cards)
restarted at will. The boundary in §5 is the same line, so a Rust port keeps it.
