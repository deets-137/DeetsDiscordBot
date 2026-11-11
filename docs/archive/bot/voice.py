"""
Voice for the bot: join a voice channel and play one Windows app's sound into it.

    /vc play app:<name>   join your voice channel (if needed) and play that app
    /vc stop              stop playing, stay in the channel
    /vc leave             stop and leave

The audio comes from `deets-tap.exe` (crates/tap), which records one app with
Windows' per-process loopback and writes 48 kHz / 16-bit / stereo PCM to stdout —
the exact shape discord.py's voice sender eats. Build it once:

    cd crates/tap
    cargo build --release

Override its location with DEETS_TAP=<path> in .env. Before streaming, the bot
checks the helper's `--info` against EXPECTED_TAP (docs/ops/TAP.md §3).

Who may use it: the bot application's owner (or its team), plus any user ids
in VOICE_USER_IDS (comma-separated, .env). It plays apps from this PC into a
call, so nobody else gets to pick one.

Needs `pip install -r requirements-voice.txt` (PyNaCl, davey). Without them
the rest of the bot runs as before; only /vc says what is missing.

Self-contained on purpose: bot/__main__.py calls `register()` and nothing here
touches /links, the journal, or storage.
"""

import asyncio
import json
import logging
import os
import subprocess
from pathlib import Path

import discord
from discord import app_commands

# Under "discord." so client.run()'s log handler prints it next to discord.py's own lines.
log = logging.getLogger("discord.lildeets.voice")

HERE = Path(__file__).resolve().parent
DEFAULT_TAP = HERE.parent / "crates" / "tap" / "target" / "release" / "deets-tap.exe"
FRAME_BYTES = 3840  # 20 ms of 48 kHz / 16-bit / stereo
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def _tap_path() -> Path:
    return Path(os.environ.get("DEETS_TAP") or DEFAULT_TAP)


# The stream this file reads, as deets-tap --info states it (crates/tap/src/main.rs
# CONTRACT). A helper built from older or newer code that disagrees is refused with a
# rebuild hint instead of being played as noise (docs/ops/TAP.md §3).
EXPECTED_TAP = {"contract": 1, "rate": 48_000, "channels": 2, "bits": 16, "frame_bytes": FRAME_BYTES}
_tap_checked: tuple[float, str | None] = (0.0, None)  # (helper mtime, problem)


def tap_contract_problem() -> str | None:
    """None if the helper speaks EXPECTED_TAP; else what's wrong. Rechecked when the exe changes."""
    global _tap_checked
    path = _tap_path()
    try:
        mtime = path.stat().st_mtime
    except OSError:
        return f"deets-tap isn't built yet (looked at `{path}`)."
    if _tap_checked[0] == mtime:
        return _tap_checked[1]
    try:
        out = subprocess.run([str(path), "--info"], capture_output=True, timeout=5, creationflags=NO_WINDOW)
        info = json.loads(out.stdout or b"{}")
    except (OSError, subprocess.TimeoutExpired, ValueError):
        info = {}
    wrong = {k: info.get(k) for k, v in EXPECTED_TAP.items() if info.get(k) != v}
    problem = None
    if wrong:
        problem = (f"deets-tap at `{path}` doesn't match this bot ({wrong}; expected {EXPECTED_TAP}). "
                   "Rebuild it: `cd crates/tap && cargo build --release`.")
    _tap_checked = (mtime, problem)
    return problem


def _extra_user_ids() -> set[int]:
    raw = os.environ.get("VOICE_USER_IDS", "")
    return {int(x) for x in raw.replace(" ", "").split(",") if x.isdigit()}


class TapSource(discord.AudioSource):
    """PCM from a running deets-tap; killing the helper is the cleanup."""

    def __init__(self, app: str):
        self.app = app
        self.proc = subprocess.Popen(
            [str(_tap_path()), "--exe", app],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL,
            creationflags=NO_WINDOW,
        )

    def read(self) -> bytes:
        data = self.proc.stdout.read(FRAME_BYTES)
        return data if len(data) == FRAME_BYTES else b""

    def is_opus(self) -> bool:
        return False

    def cleanup(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
        try:
            err = self.proc.stderr.read().decode(errors="replace").strip()
            self.proc.wait(timeout=2)
        except Exception:
            err = ""
        if err:
            log.info("deets-tap (%s): %s", self.app, err)


class ToneSource(discord.AudioSource):
    """A 440 Hz sine at -12 dBFS for `seconds`: Discord's half of the path with no capture."""

    def __init__(self, seconds: float = 5.0):
        import array
        import math

        rate, amp, per = 48_000, 0.25 * 32767, 960  # 960 frames = 20 ms
        self._frames: list[bytes] = []
        for f in range(int(seconds * 50)):
            block = array.array("h")
            for i in range(per):
                s = int(amp * math.sin(2 * math.pi * 440 * (f * per + i) / rate))
                block.extend((s, s))
            self._frames.append(block.tobytes())

    def read(self) -> bytes:
        return self._frames.pop(0) if self._frames else b""

    def is_opus(self) -> bool:
        return False


def _transport_note(voice: discord.VoiceClient) -> str:
    state = getattr(voice, "_connection", None)
    ver = getattr(state, "dave_protocol_version", "?")
    ready = getattr(state, "can_encrypt", "?")
    return f"voice E2EE (DAVE) version {ver}, ready {ready}, mode {getattr(voice, 'mode', '?')}"


_owners: set[int] = set()


async def is_host(client: discord.Client, user: discord.abc.User) -> bool:
    """The bot application's owner (or team), plus VOICE_USER_IDS: whoever may start /vc."""
    if not _owners:
        info = await client.application_info()
        if info.team:
            _owners.update(m.id for m in info.team.members)
        elif info.owner:
            _owners.add(info.owner.id)
    return user.id in _owners or user.id in _extra_user_ids()


# Output gain per guild (the bot's own volume, applied after the capture), 1.0 = as captured.
_gain: dict[int, float] = {}


def set_gain(guild: discord.Guild, gain: float) -> None:
    _gain[guild.id] = gain
    src = getattr(guild.voice_client, "source", None)
    if isinstance(src, discord.PCMVolumeTransformer):
        src.volume = gain


def gain(guild: discord.Guild) -> float:
    return _gain.get(guild.id, 1.0)


def tapping(guild: discord.Guild) -> str | None:
    """The app this guild's bot is playing into voice right now, or None."""
    voice = guild.voice_client
    if voice is None or not voice.is_connected() or not voice.is_playing():
        return None
    src = getattr(voice, "source", None)
    inner = getattr(src, "original", src)
    return inner.app if isinstance(inner, TapSource) else None


# Opus tuned for music: CELT only ("music" stops the encoder dropping into its speech
# modes, which cost bass and air), no in-band FEC stealing bits for packet loss that a
# wired desk connection doesn't have.
ENCODER = dict(signal_type="music", fec=False, bitrate=128)


async def start_stream(guild: discord.Guild, channel: discord.abc.Connectable, app: str) -> str | None:
    """Join (or move to) `channel` and play `app` into it. None on success, else what went
    wrong, for the user. Shared by /vc play and the resume after a song-boundary restart
    (bot/live.py)."""
    app = app if app.lower().endswith(".exe") else app + ".exe"
    voice = guild.voice_client
    try:
        if voice is None:
            voice = await channel.connect(self_deaf=True)
        elif voice.channel != channel:
            await voice.move_to(channel)
    except Exception as e:
        log.exception("voice connect failed")
        return f"Couldn't join {channel.mention}: {e}"

    if voice.is_playing():
        voice.stop()
    source = TapSource(app)
    # A missing app fails in the helper's first moments; catch that before claiming success.
    await asyncio.sleep(0.5)
    if source.proc.poll() is not None:
        err = source.proc.stderr.read().decode(errors="replace").strip()
        return f"Couldn't capture {app}: {err or 'helper exited'}"

    def after(error: Exception | None):
        if error:
            log.error("voice playback ended with %r", error)

    voice.play(discord.PCMVolumeTransformer(source, volume=gain(guild)), after=after, **ENCODER)
    log.info("playing %s; %s", app, _transport_note(voice))
    return None


def register(client: discord.Client, tree: app_commands.CommandTree) -> None:
    async def allowed(user: discord.abc.User) -> bool:
        return await is_host(client, user)

    async def gate(interaction: discord.Interaction) -> bool:
        if interaction.guild is None:
            await interaction.response.send_message("Voice works in a server, not in DMs.", ephemeral=True)
            return False
        if not await allowed(interaction.user):
            await interaction.response.send_message("Only the bot's owner can use /vc.", ephemeral=True)
            return False
        return True

    def missing_setup() -> str | None:
        if not _tap_path().is_file():
            return (f"deets-tap isn't built yet (looked at `{_tap_path()}`). In "
                    "`crates/tap`: `cargo build --release`.")
        problem = tap_contract_problem()
        if problem:
            return problem
        try:
            import nacl  # noqa: F401
            import davey  # noqa: F401
        except ImportError as e:
            return f"Voice packages missing ({e.name}). Run `pip install -r requirements-voice.txt`."
        return None

    vc_group = app_commands.Group(name="vc", description="Play one of this PC's apps into a voice channel.")

    async def app_choices(interaction: discord.Interaction, current: str) -> list[app_commands.Choice[str]]:
        if not _tap_path().is_file():
            return []
        try:
            proc = await asyncio.create_subprocess_exec(
                str(_tap_path()), "--list",
                stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL,
                creationflags=NO_WINDOW,
            )
            out, _ = await asyncio.wait_for(proc.communicate(), timeout=2)
        except Exception:
            return []
        names = [n.strip() for n in out.decode(errors="replace").splitlines() if n.strip()]
        cur = current.lower()
        return [app_commands.Choice(name=n, value=n) for n in names if cur in n.lower()][:25]

    @vc_group.command(name="play", description="Join your voice channel and play an app's sound.")
    @app_commands.describe(app="The app's exe, e.g. deetsmusic.exe (suggestions are apps making sound now).")
    @app_commands.autocomplete(app=app_choices)
    async def vc_play(interaction: discord.Interaction, app: str):
        if not await gate(interaction):
            return
        problem = missing_setup()
        if problem:
            await interaction.response.send_message(problem, ephemeral=True)
            return
        member = interaction.guild.get_member(interaction.user.id) or interaction.user
        channel = getattr(getattr(member, "voice", None), "channel", None)
        if channel is None:
            await interaction.response.send_message("Join a voice channel first.", ephemeral=True)
            return
        await interaction.response.defer(ephemeral=True, thinking=True)

        app = app if app.lower().endswith(".exe") else app + ".exe"
        problem = await start_stream(interaction.guild, channel, app)
        if problem:
            await interaction.followup.send(problem, ephemeral=True)
            return
        hint = " Everyone here can use `/dm` (see `/dm help`)." if app.lower() == "deetsmusic.exe" else ""
        await interaction.followup.send(f"Playing **{app}** in {channel.mention}.{hint}", ephemeral=True)

    @vc_group.command(name="tone", description="Play a 5 s test tone (checks Discord's side, no app capture).")
    async def vc_tone(interaction: discord.Interaction):
        if not await gate(interaction):
            return
        member = interaction.guild.get_member(interaction.user.id) or interaction.user
        channel = getattr(getattr(member, "voice", None), "channel", None)
        voice = interaction.guild.voice_client
        if voice is None and channel is None:
            await interaction.response.send_message("Join a voice channel first.", ephemeral=True)
            return
        await interaction.response.defer(ephemeral=True, thinking=True)
        if voice is None:
            voice = await channel.connect(self_deaf=True)
        if voice.is_playing():
            voice.stop()
        voice.play(ToneSource(), **ENCODER)
        log.info("tone; %s", _transport_note(voice))
        await interaction.followup.send(f"Test tone for 5 s. ({_transport_note(voice)})", ephemeral=True)

    @vc_group.command(name="stop", description="Stop playing (stay in the channel).")
    async def vc_stop(interaction: discord.Interaction):
        if not await gate(interaction):
            return
        voice = interaction.guild.voice_client
        if voice and voice.is_playing():
            voice.stop()
            await interaction.response.send_message("Stopped.", ephemeral=True)
        else:
            await interaction.response.send_message("Nothing is playing.", ephemeral=True)

    @vc_group.command(name="leave", description="Stop and leave the voice channel.")
    async def vc_leave(interaction: discord.Interaction):
        if not await gate(interaction):
            return
        voice = interaction.guild.voice_client
        if voice is None:
            await interaction.response.send_message("Not in a voice channel.", ephemeral=True)
            return
        await voice.disconnect()
        await interaction.response.send_message("Left.", ephemeral=True)

    tree.add_command(vc_group)

    @client.event
    async def on_voice_state_update(member: discord.Member, before: discord.VoiceState, after: discord.VoiceState):
        # Leave once the bot is alone, so a forgotten stream doesn't keep the helper running.
        voice = member.guild.voice_client
        if voice is None or before.channel is None or before.channel != voice.channel:
            return
        if not any(not m.bot for m in voice.channel.members):
            await voice.disconnect()
