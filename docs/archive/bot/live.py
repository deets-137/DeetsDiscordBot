"""
Code changes go live by themselves, and the music keeps playing (the owner, 2026-11-10:
"automatic"). docs/architecture/LIVE-RELOAD.md is the design.

Every 2 s the bot looks at its own source files. When some have changed and stayed unchanged
for one more look (an editor may write in steps), and every changed file compiles:

- **Only `bot/dm.py` / `bot/dm_bridge.py` changed → reload in place.** /dm's background tasks
  stop, its commands come off the tree, the two modules are reloaded and /dm registers again.
  The voice connection and the capture live in voice.py and are not touched: no gap at all.
  If the commands' shape changed (a new command or option), the tree is synced again.
  Cards already posted keep their old buttons until someone posts a new one.
- **Anything else changed (voice.py, app.py, this file, journal/ code the bot imports) →
  restart at a song boundary.** While the bot plays DeetsMusic, it waits for the song to
  change (or for someone to pause), pauses DeetsMusic through the bridge, writes where it
  was to RESUME_FILE, leaves voice, and exits with RESTART_CODE. The launcher
  (bot/__main__.py) starts the new code, which rejoins the same channel, starts the same
  app, waits for voice encryption to be ready, and resumes DeetsMusic. A gap of a few
  seconds between songs; nothing is skipped. Not playing DeetsMusic: it restarts at once.

A file that doesn't compile is never loaded: one log line, and the running code stays.
"""

from __future__ import annotations

import asyncio
import importlib
import json
import logging
import sys
import time
from pathlib import Path

import discord
from discord import app_commands

from journal.paths import PROJECT_ROOT

log = logging.getLogger("discord.lildeets.live")

RESTART_CODE = 75  # EX_TEMPFAIL: "run me again"
RESUME_FILE = PROJECT_ROOT / ".lildeets-resume.json"
RESUME_MAX_AGE = 180  # seconds; an older hand-off is stale and ignored
# While this file exists, changes queue up and nothing goes live: create it before a change
# that spans several files, delete it when the change is complete.
HOLD_FILE = PROJECT_ROOT / ".lildeets-hold"
EVERY = 2
APP = "deetsmusic.exe"

# Swappable in place: they hold no voice state (bot/dm.py says so at its _tasks).
LIVE = {"dm.py", "dm_bridge.py"}


def watched_files() -> list[Path]:
    """The bot's own code: bot/*.py and the journal modules it imports."""
    files = sorted((PROJECT_ROOT / "bot").glob("*.py"))
    files += [PROJECT_ROOT / "journal" / n for n in ("__init__.py", "paths.py", "linkscan.py")]
    return [f for f in files if f.exists()]


def _mtimes() -> dict[Path, float]:
    out = {}
    for f in watched_files():
        try:
            out[f] = f.stat().st_mtime
        except OSError:
            pass
    return out


def _compiles(path: Path) -> str | None:
    try:
        compile(path.read_text(encoding="utf-8"), str(path), "exec")
        return None
    except (SyntaxError, ValueError, OSError) as e:
        return f"{path.name}: {e}"


class Live:
    def __init__(self, client: discord.Client, tree: app_commands.CommandTree):
        self.client = client
        self.tree = tree
        self.seen = _mtimes()
        self.restart_requested = False
        self.restart_reason: set[str] = set()

    # --- start-up ---------------------------------------------------------------

    def install(self) -> None:
        previous = self.client.setup_hook

        async def setup_hook():
            await previous()
            asyncio.create_task(self.run())

        self.client.setup_hook = setup_hook

    async def run(self) -> None:
        await self.client.wait_until_ready()
        try:
            await self.resume()
        except Exception:
            log.exception("live: resume after restart failed")
        pending: set[Path] = set()
        while not self.client.is_closed():
            await asyncio.sleep(EVERY)
            now = _mtimes()
            fresh = {f for f, m in now.items() if self.seen.get(f) != m}
            self.seen = now
            if fresh:
                pending |= fresh  # still being written: wait one more look
                continue
            if HOLD_FILE.exists():
                continue  # a multi-file change is in progress; keep queuing
            if pending:
                changed, pending = pending, set()
                try:
                    await self.apply(changed)
                except Exception:
                    log.exception("live: applying a change failed")
            if self.restart_reason and not self.restart_requested:
                await self.maybe_restart()

    # --- a change ---------------------------------------------------------------

    async def apply(self, changed: set[Path]) -> None:
        names = sorted(f.name if f.parent.name == "bot" else f"journal/{f.name}" for f in changed)
        broken = [p for p in (_compiles(f) for f in changed) if p]
        if broken:
            log.error("live: not loading %s — it doesn't compile: %s", ", ".join(names), "; ".join(broken))
            return
        in_place = all(f.parent.name == "bot" and f.name in LIVE for f in changed)
        if in_place and not self.restart_reason:
            await self.reload_dm(names)
        else:
            self.restart_reason.update(names)
            log.info("live: %s changed; restarting at the next song boundary", ", ".join(names))

    async def reload_dm(self, names: list[str]) -> None:
        import bot.dm as dm
        import bot.dm_bridge as dm_bridge

        old = self.tree.get_command("dm")
        before = json.dumps(old.to_dict(self.tree), sort_keys=True) if old else ""
        dm.shutdown()
        self.tree.remove_command("dm")
        try:
            importlib.reload(dm_bridge)
            importlib.reload(dm)
            dm.register(self.client, self.tree)
        except Exception:
            log.exception("live: reloading /dm failed; restarting at the next song boundary instead")
            self.restart_reason.update(names)
            return
        new = self.tree.get_command("dm")
        after = json.dumps(new.to_dict(self.tree), sort_keys=True) if new else ""
        synced = ""
        if after != before:
            await self.tree.sync()
            synced = "; commands changed, synced"
        log.info("live: reloaded /dm in place (%s)%s; the music didn't stop", ", ".join(names), synced)

    # --- the song-boundary restart ---------------------------------------------

    async def maybe_restart(self) -> None:
        from bot import voice
        from bot.dm import bridge
        from bot.dm_bridge import BridgeError

        playing_dm = [g for g in self.client.guilds if voice.tapping(g) == APP]
        if playing_dm:
            try:
                start = await bridge.now_playing()
            except BridgeError:
                start = {}
            song = start.get("catalogId") or start.get("title")
            if start.get("playing") and song:
                log.info("live: waiting for “%s” to end", start.get("title"))
                while True:  # check each second: the change is caught within a second of it
                    await asyncio.sleep(1)
                    try:
                        np = await bridge.now_playing()
                    except BridgeError:
                        break
                    if not np.get("playing") or (np.get("catalogId") or np.get("title")) != song:
                        break
        paused_by_us = False
        if playing_dm:
            try:
                np = await bridge.now_playing()
                if np.get("playing"):
                    await bridge.command("pause")
                    paused_by_us = True
            except BridgeError:
                pass
        await self.restart(paused_by_us)

    async def restart(self, paused_by_us: bool) -> None:
        from bot import voice

        streams = []
        for g in self.client.guilds:
            vc = g.voice_client
            if vc is not None and vc.is_connected():
                streams.append({"guild": g.id, "channel": vc.channel.id, "app": voice.tapping(g), "gain": voice.gain(g)})
        RESUME_FILE.write_text(json.dumps({"at": time.time(), "paused_by_us": paused_by_us, "streams": streams,
                                           "reason": sorted(self.restart_reason)}), encoding="utf-8")
        log.info("live: restarting for %s (%d voice channel(s) to rejoin%s)", ", ".join(sorted(self.restart_reason)),
                 len(streams), ", DeetsMusic paused" if paused_by_us else "")
        self.restart_requested = True
        for g in self.client.guilds:
            if g.voice_client is not None:
                await g.voice_client.disconnect(force=True)
        await self.client.close()

    async def resume(self) -> None:
        if not RESUME_FILE.exists():
            return
        try:
            state = json.loads(RESUME_FILE.read_text(encoding="utf-8"))
        finally:
            RESUME_FILE.unlink(missing_ok=True)
        if time.time() - state.get("at", 0) > RESUME_MAX_AGE:
            log.info("live: ignoring a stale restart hand-off")
            return
        from bot import voice
        from bot.dm import bridge

        for s in state.get("streams", []):
            guild = self.client.get_guild(s["guild"])
            channel = self.client.get_channel(s["channel"])
            if guild is None or channel is None:
                continue
            voice.set_gain(guild, s.get("gain", 1.0))
            if s.get("app"):
                problem = await voice.start_stream(guild, channel, s["app"])
                if problem:
                    log.error("live: couldn't resume %s: %s", s["app"], problem)
            else:
                await channel.connect(self_deaf=True)
            await self._encryption_ready(guild)
        if state.get("paused_by_us"):
            await bridge.command("play")
        log.info("live: back after the restart (%s)%s", ", ".join(state.get("reason", [])),
                 "; DeetsMusic resumed" if state.get("paused_by_us") else "")

    @staticmethod
    async def _encryption_ready(guild: discord.Guild, timeout: float = 6.0) -> None:
        """Wait until the call can decrypt what we send (DAVE ready), so the first seconds
        of the song after the restart aren't lost to the handshake."""
        vc = guild.voice_client
        state = getattr(vc, "_connection", None)
        end = time.monotonic() + timeout
        while state is not None and time.monotonic() < end:
            if getattr(state, "dave_protocol_version", 0) == 0 or getattr(state, "can_encrypt", True):
                return
            await asyncio.sleep(0.2)
