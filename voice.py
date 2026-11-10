"""
Voice for the bot: join a voice channel and play one Windows app's sound into it.

    /vc play app:<name>   join your voice channel (if needed) and play that app
    /vc stop              stop playing, stay in the channel
    /vc leave             stop and leave

The audio comes from `deets-tap.exe` (DeetsAirplay repo, crates/airplay/src/bin),
which records one app with Windows' per-process loopback and writes 48 kHz /
16-bit / stereo PCM to stdout — the exact shape discord.py's voice sender eats.
Build it once:

    cd ../DeetsAirplay/crates/airplay
    cargo build --release --bin deets-tap

Override its location with DEETS_TAP=<path> in .env.

Who may use it: the bot application's owner (or its team), plus any user ids
in VOICE_USER_IDS (comma-separated, .env). It plays apps from this PC into a
call, so nobody else gets to pick one.

Needs `pip install -r requirements-voice.txt` (PyNaCl, davey). Without them
the rest of the bot runs as before; only /vc says what is missing.

Self-contained on purpose: bot.py calls `register()` and nothing else here
touches /links, the scanners, or storage.
"""

import asyncio
import logging
import os
import subprocess
from pathlib import Path

import discord
from discord import app_commands

log = logging.getLogger("deetsotd.voice")

HERE = Path(__file__).resolve().parent
DEFAULT_TAP = HERE.parent / "DeetsAirplay" / "crates" / "airplay" / "target" / "release" / "deets-tap.exe"
FRAME_BYTES = 3840  # 20 ms of 48 kHz / 16-bit / stereo
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def _tap_path() -> Path:
    return Path(os.environ.get("DEETS_TAP") or DEFAULT_TAP)


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


def register(client: discord.Client, tree: app_commands.CommandTree) -> None:
    owners: set[int] = set()

    async def allowed(user: discord.abc.User) -> bool:
        if not owners:
            info = await client.application_info()
            if info.team:
                owners.update(m.id for m in info.team.members)
            elif info.owner:
                owners.add(info.owner.id)
        return user.id in owners or user.id in _extra_user_ids()

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
                    "`DeetsAirplay/crates/airplay`: `cargo build --release --bin deets-tap`.")
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
        voice = interaction.guild.voice_client
        try:
            if voice is None:
                voice = await channel.connect(self_deaf=True)
            elif voice.channel != channel:
                await voice.move_to(channel)
        except Exception as e:
            log.exception("voice connect failed")
            await interaction.followup.send(f"Couldn't join {channel.mention}: {e}", ephemeral=True)
            return

        if voice.is_playing():
            voice.stop()
        source = TapSource(app)
        # A missing app fails in the helper's first moments; catch that before claiming success.
        await asyncio.sleep(0.5)
        if source.proc.poll() is not None:
            err = source.proc.stderr.read().decode(errors="replace").strip()
            await interaction.followup.send(f"Couldn't capture {app}: {err or 'helper exited'}", ephemeral=True)
            return

        def after(error: Exception | None):
            if error:
                log.error("voice playback ended with %r", error)

        voice.play(source, after=after)
        await interaction.followup.send(f"Playing **{app}** in {channel.mention}.", ephemeral=True)

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
