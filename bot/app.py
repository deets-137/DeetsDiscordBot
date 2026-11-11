"""
Lil Deets — the Discord bot. One process owns every slash command of the application
(Discord keeps one global command list per app, and the last process to sync wins).

Start it with `python -m bot` (the launcher in bot/__main__.py), which runs this module and
restarts it when bot/live.py asks. `python -m bot.app` runs it once, with no restarts.

Setup (docs/ops/SETUP.md):
  pip install -r requirements.txt -r requirements-voice.txt
  cp config.example.py config.py
  put DISCORD_TOKEN in .env
  cd crates/tap && cargo build --release      # the /vc capture helper
  python -m bot                               # from the repo root

Commands:
  /links [limit]   scan this channel's history for hyperlinks; DMs you Markdown + CSV
  /vc …            play one Windows app into a voice channel (bot/voice.py)
  /dm …            control DeetsMusic from Discord (bot/dm.py)

Needs the "Message Content Intent" (Bot → Privileged Gateway Intents) for /links.
"""

import os

import discord
from discord import app_commands
from dotenv import load_dotenv

from journal.linkscan import collect_links, write_outputs
from journal.paths import PROJECT_ROOT

load_dotenv(PROJECT_ROOT / ".env")

try:
    import config
except ModuleNotFoundError:
    raise SystemExit("No config.py — copy config.example.py to config.py first.")

intents = discord.Intents.default()
intents.message_content = True

client = discord.Client(intents=intents)
tree = app_commands.CommandTree(client)

# /vc: play one of this PC's apps into a voice channel (voice.py, self-contained).
from bot import voice  # noqa: E402
from bot import dm  # noqa: E402

from bot import live  # noqa: E402

voice.register(client, tree)
# /dm: control DeetsMusic from Discord through its local bridge (dm.py, dm_bridge.py).
dm.register(client, tree)
# Code changes go live by themselves: /dm reloads in place, the rest restarts between songs.
watcher = live.Live(client, tree)
watcher.install()


@client.event
async def on_ready():
    await tree.sync()
    print(f"Logged in as {client.user} — slash commands synced.")


@tree.command(name="links", description="Scan this channel's history for hyperlinks.")
@app_commands.describe(limit="Max messages to scan (leave blank for all).")
async def links(interaction: discord.Interaction, limit: int | None = None):
    allowed = config.ALLOWED_CHANNEL_IDS
    if allowed and interaction.channel_id not in allowed:
        await interaction.response.send_message(
            "Not allowed in this channel.", ephemeral=True)
        return

    # History scans can take a while — defer so we don't hit the 3s ack window.
    await interaction.response.defer(ephemeral=True, thinking=True)

    channel = interaction.channel
    records = await collect_links(channel, limit=limit, newest=True)
    label = getattr(channel, "name", str(interaction.channel_id))
    md, csv = write_outputs(records, label)

    unique = len({r.url for r in records})
    summary = f"Found **{len(records)}** links ({unique} unique) in #{label}."

    files = [discord.File(md), discord.File(csv)]
    # Try DM first; fall back to an ephemeral reply if DMs are closed.
    try:
        await interaction.user.send(summary, files=files)
        await interaction.followup.send(summary + " Sent to your DMs.", ephemeral=True)
    except discord.Forbidden:
        await interaction.followup.send(
            summary, files=[discord.File(md), discord.File(csv)], ephemeral=True)


def main():
    token = os.environ.get("DISCORD_TOKEN", "")
    if not token:
        raise SystemExit("DISCORD_TOKEN not set (put it in .env).")
    client.run(token)
    if watcher.restart_requested:
        raise SystemExit(live.RESTART_CODE)  # the launcher starts the new code


if __name__ == "__main__":
    main()
