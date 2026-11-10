"""
DeetsOTD Discord bot — slash command to scan the current channel for hyperlinks.

Setup:
  pip install -r requirements.txt
  cp config.example.py config.py   # edit ALLOWED_CHANNEL_IDS if you want
  set DISCORD_TOKEN in .env
  python bot.py

In Discord:
  /links              scan this channel's full history
  /links limit:500    scan only the most recent 500 messages

Requires the "Message Content Intent" toggle in the Discord developer portal
(Bot → Privileged Gateway Intents) so it can read historical message text.
The bot DMs you the Markdown + CSV files so the channel stays uncluttered.
"""

import os

import discord
from discord import app_commands
from dotenv import load_dotenv

from linkscan import collect_links, write_outputs

load_dotenv()

try:
    import config
except ModuleNotFoundError:
    raise SystemExit("No config.py — copy config.example.py to config.py first.")

intents = discord.Intents.default()
intents.message_content = True

client = discord.Client(intents=intents)
tree = app_commands.CommandTree(client)

# /vc: play one of this PC's apps into a voice channel (voice.py, self-contained).
import voice  # noqa: E402
import dm  # noqa: E402

voice.register(client, tree)
# /dm: control DeetsMusic from Discord through its local bridge (dm.py, dm_bridge.py).
dm.register(client, tree)


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
    records = await collect_links(channel, limit=limit)
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


if __name__ == "__main__":
    main()
