"""
/dm — control DeetsMusic from Discord while the host plays it into voice.

    /dm help                 what each command does, and what you may do right now
    /dm np                   now-playing card with ⏮ ⏯ ⏭ and a Queue button
    /dm pause | resume | skip | back
    /dm add song [next]      search as you type; a confident match is queued straight away,
                             otherwise you get a picker of the top 5
    /dm queue [page]         Up Next, 10 to a page, ◀ ▶ to page
    /dm remove n | move n to Up Next edits (row numbers as /dm queue shows them)
    /dm play song            replace what's playing (host only)
    /dm volume percent       the bot's own output gain (host only)

Who may do what: the host (whoever may run /vc) always. Anyone else only while the
host is playing deetsmusic.exe into a voice channel with /vc play, only from inside
that channel, and only what DeetsMusic › Settings › Rooms lets a guest do
(roomGuests.playPause / skip / add / changeQueue: "everyone" or host only), read live.
Library, playlists, settings, Diary and Song of the Day are never reachable from here.

Talks to DeetsMusic only through its bridge (dm_bridge.py); no DeetsMusic code changes.
Cards are Discord's Components V2 (discord.ui.LayoutView), at most 40 components each.
"""

import asyncio
import logging
import re
import time

import discord
from discord import app_commands, ui

from bot import voice
from bot.dm_bridge import Bridge, BridgeError, artwork, mmss

log = logging.getLogger("discord.lildeets.dm")
bridge = Bridge()

APP = "deetsmusic.exe"
PAGE = 10
# The np card follows the song: check the bridge every 5 s, edit only on a change, for 3 h.
WATCH_EVERY, WATCH_FOR = 5, 3 * 3600

# action → the Rooms rule that lets a guest do it (None: host only; "read": any guest in the call).
RULE = {
    "np": "read",
    "queue": "read",
    "pause": "roomGuests.playPause",
    "resume": "roomGuests.playPause",
    "skip": "roomGuests.skip",
    "back": "roomGuests.skip",
    "add": "roomGuests.add",
    "remove": "roomGuests.changeQueue",
    "move": "roomGuests.changeQueue",
    "play": None,
    "volume": None,
}
RULE_LABEL = {
    "roomGuests.playPause": "Guests may start and stop",
    "roomGuests.skip": "Guests may skip",
    "roomGuests.add": "Guests may add songs",
    "roomGuests.changeQueue": "Guests may change Up Next",
}


def _clip(s: str, n: int) -> str:
    return s if len(s) <= n else s[: n - 1] + "…"


def _words(s: str) -> list[str]:
    return re.findall(r"[a-z0-9]+", s.lower())


# Apple's contentRating: "explicit", "clean" (the edited version of an explicit song), or
# absent (nothing to edit; Apple doesn't call that clean, so neither do we).
_RATING_WORD = {"explicit": "Explicit", "clean": "Clean"}
_RATING_MARK = {"explicit": "🅴", "clean": "🅲"}
_NO_MARK = "▫️"  # same width as the letters, so titles line up in the suggestion list


def _secs(t: dict) -> float:
    return (t.get("durationMs") or 0) / 1000


def tag(t: dict, width: int | None = None) -> str:
    """`MM:SS | Explicit` in a code span. As a column (queue page, picker: pass the page's
    `column_width`) every row is the same width in the monospace span: the length is
    right-aligned to the longest on the page and the rating padded to "Explicit". On its
    own (the add line, the now-playing row) an unrated song is just `MM:SS`."""
    word = _RATING_WORD.get(t.get("contentRating") or "", "")
    if width is None:
        return f"`{mmss(_secs(t))}{' | ' + word if word else ''}`"
    return f"`{mmss(_secs(t)):>{width}} | {word:<8}`"


FIGURE_SPACE = " "  # exactly one digit wide, in any font that has it


def suggestion(t: dict, width: int = 4) -> str:
    """One autocomplete row. Discord shows these as plain text in a proportional font, cut off
    on the right, so the column goes on the left: length, then 🅴 / 🅲 / ▫️, then the song.
    Every row of one query gets the same column width: a shorter length (3:04 next to 12:31)
    is padded with figure spaces, which are one digit wide, and an unrated song gets the
    ▫️ emoji, which is as wide as the lettered ones."""
    mark = _RATING_MARK.get(t.get("contentRating") or "", _NO_MARK)
    length = mmss(_secs(t))
    length = FIGURE_SPACE * (width - len(length)) + length
    return _clip(f"{length} {mark} {t.get('title')} — {t.get('artistName')} · {t.get('albumName', '')}", 100)


def column_width(tracks: list[dict]) -> int:
    """Characters in the longest `M:SS` / `MM:SS` / `MMM:SS` of a query's rows."""
    return max((len(mmss(_secs(t))) for t in tracks), default=4)


def _line(t: dict) -> str:
    return f"{t.get('title', '?')} — {t.get('artistName', '?')}"


def _notices(reply: dict) -> str:
    notes = [n.get("text", "") for n in reply.get("notices") or [] if n.get("text")]
    return "".join(f"\n-# ⚠ {n}" for n in notes)


def confident(term: str, songs: list[dict]) -> bool:
    """Top result is a sure thing: every typed word is in its title/artist/album, and the
    runner-up doesn't also match every word (so 'pink' alone still asks)."""
    if not songs:
        return False
    want = set(_words(term))
    if not want:
        return False

    def hay(t: dict) -> set[str]:
        return set(_words(f"{t.get('title', '')} {t.get('artistName', '')} {t.get('albumName', '')}"))

    top = hay(songs[0])
    if not want <= top and not all(any(h.startswith(w) for h in top) for w in want):
        return False
    return len(songs) == 1 or not want <= hay(songs[1])


async def check(client: discord.Client, interaction: discord.Interaction, action: str) -> str | None:
    """None when allowed; otherwise the reason, for an ephemeral reply."""
    if interaction.guild is None:
        return "/dm works in a server."
    if await voice.is_host(client, interaction.user):
        return None
    rule = RULE[action]
    if rule is None:
        return "Only the host can do that."
    if voice.tapping(interaction.guild) != APP:
        return "/dm opens up to everyone while the host is playing DeetsMusic in voice."
    vc = interaction.guild.voice_client
    member = interaction.guild.get_member(interaction.user.id)
    if not member or not member.voice or member.voice.channel != vc.channel:
        return f"Join {vc.channel.mention} to control the music."
    if rule == "read":
        return None
    if (await bridge.rules()).get(rule) != "everyone":
        return f"The host has set “{RULE_LABEL[rule]}” to host only in DeetsMusic."
    return None


# --- cards -----------------------------------------------------------------------


def _bar(progress: float, cells: int = 18) -> str:
    k = max(0, min(cells - 1, int(round(progress * (cells - 1)))))
    return "━" * k + "●" + "─" * (cells - 1 - k)


def _section(text: str, art: str | None) -> ui.Item:
    return ui.Section(ui.TextDisplay(text), accessory=ui.Thumbnail(art)) if art else ui.TextDisplay(text)


async def np_view(client: discord.Client, guild: discord.Guild | None) -> ui.LayoutView:
    np, q, accent = await asyncio.gather(bridge.now_playing(), bridge.queue(), bridge.accent())
    view = ui.LayoutView(timeout=WATCH_FOR)
    if not np.get("active") or not np.get("title"):
        view.add_item(ui.Container(ui.TextDisplay("Nothing is playing in DeetsMusic."), accent_colour=accent))
        return view
    where = ""
    vc = guild.voice_client if guild else None
    if vc and voice.tapping(guild) == APP:
        where = f" in {vc.channel.mention}"
    state = "Now playing" if np.get("playing") else "Paused"
    up = (q.get("upcoming") or [None])[0]
    # The bar is drawn once per song (the card is edited only when the song changes); the
    # countdown is a Discord timestamp, which every client ticks itself with no edits.
    dur, at = np.get("duration") or 0, np.get("currentTime") or 0
    if np.get("playing") and dur:
        ends = f" · ends <t:{int(time.time() + dur - at)}:R>"
    else:
        ends = f" · paused at {mmss(at)}" if dur else ""
    text = (
        f"-# {state}{where}\n"
        f"### {np.get('title')}\n"
        f"{np.get('artist', '')} · {np.get('album', '')}\n"
        f"`{mmss(at)}` {_bar(np.get('progress') or 0)} `{mmss(dur)}`\n"
        f"-# {mmss(dur)}{ends}"
        + (f" · up next: {_line(up)}" if up else "")
    )
    buttons = ui.ActionRow(
        _NpButton("back", "⏮"),
        _NpButton("pause" if np.get("playing") else "resume", "⏸" if np.get("playing") else "▶"),
        _NpButton("skip", "⏭"),
        _NpButton("queue", "Queue"),
    )
    for b in buttons.children:
        b.client = client
    view.add_item(ui.Container(_section(text, artwork(np, 300)), buttons, accent_colour=accent))
    return view


class _NpButton(ui.Button):
    client: discord.Client

    def __init__(self, action: str, label: str):
        super().__init__(label=label, style=discord.ButtonStyle.secondary)
        self.action = action

    async def callback(self, interaction: discord.Interaction):
        reason = await check(self.client, interaction, self.action)
        if reason:
            await interaction.response.send_message(reason, ephemeral=True)
            return
        try:
            if self.action == "queue":
                await interaction.response.send_message(view=await queue_view(self.client, 1), ephemeral=True)
                return
            await interaction.response.defer()
            kind = {"back": "previous", "pause": "pause", "resume": "play", "skip": "next"}[self.action]
            await bridge.command(kind)
            await interaction.edit_original_response(view=await np_view(self.client, interaction.guild))
        except BridgeError as e:
            await interaction.followup.send(str(e), ephemeral=True)


async def queue_view(client: discord.Client, page: int) -> ui.LayoutView:
    q, accent = await asyncio.gather(bridge.queue(), bridge.accent())
    upcoming = q.get("upcoming") or []
    pages = max(1, -(-len(upcoming) // PAGE))
    page = max(1, min(page, pages))
    total_s = sum((t.get("durationMs") or 0) for t in upcoming) // 1000
    h, m = divmod(total_s // 60, 60)
    length = f"{h} h {m} m" if h else f"{m} m"

    cur = q.get("current")
    head = (
        f"-# Now playing\n**{cur.get('title')}** · {cur.get('artistName', '')} {tag(cur)}" if cur else "-# Nothing playing"
    ) + f"\n-# Up Next · {len(upcoming)} songs · {length} · page {page} of {pages}"
    parts: list[ui.Item] = [_section(head, artwork(cur, 300)), ui.Separator()]
    start = (page - 1) * PAGE
    width = column_width(upcoming[start : start + PAGE])
    for n, t in enumerate(upcoming[start : start + PAGE], start=start + 1):
        text = f"`{n:>2}` {tag(t, width)} **{_clip(t.get('title', '?'), 80)}** · {_clip(t.get('artistName', ''), 60)}\n-# {_clip(t.get('albumName', ''), 60)}"
        parts.append(_section(text, artwork(t, 120)))
    if not upcoming:
        parts.append(ui.TextDisplay("Up Next is empty. Add a song with `/dm add`."))
    prev, nxt = _PageButton(page - 1, "◀"), _PageButton(page + 1, "▶")
    prev.disabled, nxt.disabled = page <= 1, page >= pages
    for b in (prev, nxt):
        b.client = client
    parts.append(ui.ActionRow(prev, nxt))
    view = ui.LayoutView(timeout=900)
    view.add_item(ui.Container(*parts, accent_colour=accent))
    return view


class _PageButton(ui.Button):
    client: discord.Client

    def __init__(self, page: int, label: str):
        super().__init__(label=label, style=discord.ButtonStyle.secondary)
        self.page = page

    async def callback(self, interaction: discord.Interaction):
        reason = await check(self.client, interaction, "queue")
        if reason:
            await interaction.response.send_message(reason, ephemeral=True)
            return
        try:
            await interaction.response.edit_message(view=await queue_view(self.client, self.page))
        except BridgeError as e:
            await interaction.followup.send(str(e), ephemeral=True)


def picker_view(client: discord.Client, term: str, songs: list[dict], next_up: bool, accent: int) -> ui.LayoutView:
    parts: list[ui.Item] = [ui.TextDisplay(f"Which one? “{_clip(term, 80)}”")]
    row = ui.ActionRow()
    width = column_width(songs[:5])
    for n, t in enumerate(songs[:5], start=1):
        parts.append(_section(f"`{n}` {tag(t, width)} **{_clip(t.get('title', '?'), 80)}** · {t.get('artistName', '')}\n-# {t.get('albumName', '')}", artwork(t, 120)))
        b = _PickButton(t, next_up, str(n))
        b.client = client
        row.add_item(b)
    parts.append(row)
    view = ui.LayoutView(timeout=300)
    view.add_item(ui.Container(*parts, accent_colour=accent))
    return view


class _PickButton(ui.Button):
    client: discord.Client

    def __init__(self, track: dict, next_up: bool, label: str):
        super().__init__(label=label, style=discord.ButtonStyle.secondary)
        self.track, self.next_up = track, next_up

    async def callback(self, interaction: discord.Interaction):
        reason = await check(self.client, interaction, "add")
        if reason:
            await interaction.response.send_message(reason, ephemeral=True)
            return
        await interaction.response.defer()
        try:
            reply = await bridge.enqueue(f"song:{self.track['catalogId']}", self.next_up)
        except BridgeError as e:
            await interaction.followup.send(str(e), ephemeral=True)
            return
        done = ui.LayoutView()
        done.add_item(ui.TextDisplay(f"Picked {_line(self.track)}."))
        await interaction.edit_original_response(view=done)
        line = await _added(interaction.user, [self.track], self.next_up, f"song:{self.track['catalogId']}")
        await interaction.followup.send(line + _notices(reply))


async def _added(user: discord.abc.User, tracks: list[dict], next_up: bool, song_id: str) -> str:
    """The channel line for an add, with where it landed in Up Next and when it plays."""
    what = _line(tracks[0]) if tracks else "that"
    line = f"➕ {user.display_name} added **{what}**{' ' + tag(tracks[0]) if tracks else ''}"
    try:
        q, np = await asyncio.gather(bridge.queue(), bridge.now_playing())
    except BridgeError:
        return line + "."
    cid = song_id.removeprefix("song:")
    up = q.get("upcoming") or []
    rows = [n for n, t in enumerate(up, 1) if str(t.get("catalogId")) == cid]
    if not rows:
        return line + "."
    n = rows[0] if next_up else rows[-1]
    ahead = max(0.0, (np.get("duration") or 0) - (np.get("currentTime") or 0))
    ahead += sum((t.get("durationMs") or 0) for t in up[: n - 1]) / 1000
    if np.get("playing"):
        when = f"plays <t:{int(time.time() + ahead)}:R>"
    else:
        when = f"{mmss(ahead)} of music ahead of it"
    return f"{line} · #{n} of {len(up)} in Up Next · {when}."


def help_text(rights: dict[str, bool], host: bool, session: bool) -> str:
    def mark(action: str) -> str:
        return "✅" if rights.get(action) else "🔒"

    who = "You're the host: everything is open to you." if host else (
        "The host is playing DeetsMusic here. ✅ = you can, 🔒 = host only right now (DeetsMusic › Settings › Rooms)."
        if session else "Guests can use /dm while the host plays DeetsMusic into voice with `/vc play`."
    )
    return (
        "### Lil Deets · DeetsMusic\n"
        f"-# {who}\n"
        f"{mark('np')} `/dm np` — what's playing, with ⏮ ⏯ ⏭\n"
        f"{mark('queue')} `/dm queue` — Up Next, 10 a page\n"
        f"{mark('pause')} `/dm pause` · `/dm resume`\n"
        f"{mark('skip')} `/dm skip` · `/dm back`\n"
        f"{mark('add')} `/dm add` — search as you type; `next: True` plays it next\n"
        f"{mark('remove')} `/dm remove` · `/dm move` — edit Up Next by row number\n"
        f"{mark('play')} `/dm play` — replace what's playing\n"
        f"{mark('volume')} `/dm volume` — the bot's volume in the call\n"
        "-# The host starts and stops the stream with `/vc play` · `/vc stop` · `/vc leave`."
    )


# --- commands --------------------------------------------------------------------


def register(client: discord.Client, tree: app_commands.CommandTree) -> None:
    group = app_commands.Group(name="dm", description="Control DeetsMusic from Discord.")
    refreshers: dict[int, asyncio.Task] = {}

    async def gate(interaction: discord.Interaction, action: str) -> bool:
        reason = await check(client, interaction, action)
        if reason:
            await interaction.response.send_message(reason, ephemeral=True)
            return False
        return True

    async def run(interaction: discord.Interaction, action: str, work, *, ephemeral: bool = False):
        """Gate, defer (bridge calls can take seconds), do the work, report BridgeErrors."""
        if not await gate(interaction, action):
            return
        await interaction.response.defer(ephemeral=ephemeral, thinking=True)
        try:
            await work()
        except BridgeError as e:
            await interaction.followup.send(str(e), ephemeral=True)

    async def song_choices(interaction: discord.Interaction, current: str) -> list[app_commands.Choice[str]]:
        if len(current.strip()) < 2 or await check(client, interaction, "add"):
            return []
        try:
            songs = await asyncio.wait_for(bridge.search_songs(current, timeout=2.5), timeout=2.6)
        except (BridgeError, asyncio.TimeoutError):
            return []
        rows = [t for t in songs[:25] if t.get("catalogId")]
        width = column_width(rows)
        return [app_commands.Choice(name=suggestion(t, width), value=f"song:{t['catalogId']}") for t in rows]

    async def resolve(song: str) -> tuple[str | None, list[dict]]:
        """(song id to use, candidates). A picked suggestion is an id already."""
        if song.startswith("song:"):
            return song, []
        songs = await bridge.search_songs(song)
        if confident(song, songs):
            return f"song:{songs[0]['catalogId']}", []
        return None, songs[:5]

    @group.command(name="help", description="What /dm can do, and what you can do right now.")
    async def dm_help(interaction: discord.Interaction):
        host = interaction.guild is not None and await voice.is_host(client, interaction.user)
        rights = {a: (await check(client, interaction, a)) is None for a in RULE}
        session = interaction.guild is not None and voice.tapping(interaction.guild) == APP
        try:
            accent = await bridge.accent()
        except BridgeError:
            accent = None
        view = ui.LayoutView()
        view.add_item(ui.Container(ui.TextDisplay(help_text(rights, host, session)), accent_colour=accent))
        await interaction.response.send_message(view=view, ephemeral=True)

    @group.command(name="np", description="What's playing, with controls.")
    async def dm_np(interaction: discord.Interaction):
        async def work():
            await interaction.followup.send(view=await np_view(client, interaction.guild))
            old = refreshers.pop(interaction.channel_id, None)
            if old:
                old.cancel()
            refreshers[interaction.channel_id] = asyncio.create_task(refresh(interaction))

        await run(interaction, "np", work)

    async def refresh(interaction: discord.Interaction):
        """Keep the card on the current song: poll the local bridge (free), and edit the
        message only when the song or play/pause changes — about once a song, far under
        Discord's edit limits. The countdown in the card ticks client-side on its own.
        Edits go through the message (bot token), not the interaction, whose token dies
        after 15 minutes. Stops when a newer /dm np takes over the channel or after WATCH_FOR."""
        try:
            message = await interaction.original_response()
            last = None
            for _ in range(WATCH_FOR // WATCH_EVERY):
                await asyncio.sleep(WATCH_EVERY)
                try:
                    np = await bridge.now_playing()
                except BridgeError:
                    continue
                key = (np.get("catalogId") or np.get("title"), bool(np.get("playing")))
                if last is None:
                    last = key
                    continue
                if key != last:
                    last = key
                    await message.edit(view=await np_view(client, interaction.guild))
        except (BridgeError, discord.HTTPException, asyncio.CancelledError):
            pass

    def simple(name: str, kind: str, done: str, description: str):
        @group.command(name=name, description=description)
        async def cmd(interaction: discord.Interaction):
            async def work():
                reply = await bridge.command(kind)
                await interaction.followup.send(f"{done} — {interaction.user.display_name}" + _notices(reply))

            await run(interaction, name, work)

        return cmd

    simple("pause", "pause", "⏸ Paused", "Pause DeetsMusic.")
    simple("resume", "play", "▶ Resumed", "Resume DeetsMusic.")
    simple("skip", "next", "⏭ Skipped", "Skip to the next song.")
    simple("back", "previous", "⏮ Back", "Go back a song.")

    @group.command(name="add", description="Add a song to Up Next.")
    @app_commands.describe(song="Start typing: suggestions appear. Enter on your own text takes a sure match or asks.",
                           next="Play it next instead of last.")
    @app_commands.autocomplete(song=song_choices)
    async def dm_add(interaction: discord.Interaction, song: str, next: bool = False):
        async def work():
            song_id, candidates = await resolve(song)
            if song_id:
                reply = await bridge.enqueue(song_id, next)
                # The deferred reply is ephemeral (a picker may follow); drop it so the
                # "added" line goes to the channel as a fresh, public message.
                await interaction.delete_original_response()
                line = await _added(interaction.user, reply.get("tracks") or [], next, song_id)
                await interaction.followup.send(line + _notices(reply))
            elif candidates:
                await interaction.followup.send(view=picker_view(client, song, candidates, next, await bridge.accent()), ephemeral=True)
            else:
                await interaction.followup.send(f"No songs found for “{_clip(song, 80)}”.", ephemeral=True)

        # Ephemeral thinking: the picker is for the asker only; the "added" line goes to the channel.
        await run(interaction, "add", work, ephemeral=True)

    @group.command(name="queue", description="Up Next, 10 songs a page.")
    @app_commands.describe(page="Which page (default 1).")
    async def dm_queue(interaction: discord.Interaction, page: app_commands.Range[int, 1, 999] = 1):
        async def work():
            await interaction.followup.send(view=await queue_view(client, page))

        await run(interaction, "queue", work)

    @group.command(name="remove", description="Remove a song from Up Next by its row number.")
    @app_commands.describe(n="Row number from /dm queue.")
    async def dm_remove(interaction: discord.Interaction, n: app_commands.Range[int, 1, 9999]):
        async def work():
            q = await bridge.queue()
            up = q.get("upcoming") or []
            if n > len(up):
                await interaction.followup.send(f"Up Next has {len(up)} songs.", ephemeral=True)
                return
            gone = up[n - 1]
            await bridge.queue_edit("remove", n)
            await interaction.followup.send(f"➖ {interaction.user.display_name} removed **{_line(gone)}** (row {n}).")

        await run(interaction, "remove", work)

    @group.command(name="move", description="Move a song in Up Next.")
    @app_commands.describe(n="Row to move.", to="Row it should end up at (1 = plays next).")
    async def dm_move(interaction: discord.Interaction, n: app_commands.Range[int, 1, 9999], to: app_commands.Range[int, 1, 9999]):
        async def work():
            q = await bridge.queue()
            up = q.get("upcoming") or []
            if n > len(up) or to > len(up):
                await interaction.followup.send(f"Up Next has {len(up)} songs.", ephemeral=True)
                return
            moved = up[n - 1]
            await bridge.queue_edit("move", n, to)
            await interaction.followup.send(f"↕ {interaction.user.display_name} moved **{_line(moved)}** to row {to}.")

        await run(interaction, "move", work)

    @group.command(name="play", description="Play a song now, replacing what's playing (host).")
    @app_commands.describe(song="Start typing: suggestions appear.")
    @app_commands.autocomplete(song=song_choices)
    async def dm_play(interaction: discord.Interaction, song: str):
        async def work():
            song_id, candidates = await resolve(song)
            if not song_id:
                hint = "; pick one of the suggestions" if candidates else ""
                await interaction.followup.send(f"Not sure which song you mean{hint}.", ephemeral=True)
                return
            reply = await bridge.play(song_id)
            tracks = reply.get("tracks") or []
            await interaction.followup.send(f"▶ {interaction.user.display_name} played **{_line(tracks[0]) if tracks else 'it'}**." + _notices(reply))

        await run(interaction, "play", work)

    @group.command(name="volume", description="The bot's volume in the call (host).")
    @app_commands.describe(percent="0–200; 100 = as DeetsMusic plays it.")
    async def dm_volume(interaction: discord.Interaction, percent: app_commands.Range[int, 0, 200]):
        if not await gate(interaction, "volume"):
            return
        voice.set_gain(interaction.guild, percent / 100)
        await interaction.response.send_message(f"🔊 Volume {percent}%.", ephemeral=True)

    tree.add_command(group)
    StatusWatcher(client).install()


class StatusWatcher:
    """While the bot plays DeetsMusic into a voice channel, show the song in two places:

    - the voice channel's status (the line under its name in the channel list), for
      everyone in the server: "🎵 Title — Artist", or "⏸ …" while paused;
    - the bot's own profile status: "Listening to Title — Artist".

    Polls the local bridge every 5 s (free) and calls Discord only when the line changes,
    about once a song. Clears both when the stream stops, the bot leaves, or DeetsMusic
    quits. Setting a channel status needs the bot's "Set Voice Channel Status" permission
    there; without it the channel line is skipped and the profile status still updates."""

    EVERY = 5
    STATUS_MAX = 500  # Discord's limit for a voice channel status

    def __init__(self, client: discord.Client):
        self.client = client
        self.channel_line: dict[int, str] = {}  # voice channel id → status we set
        self.presence_line: str | None = None
        self.warned: set[int] = set()

    def install(self) -> None:
        # Start once the client's loop runs, without taking over bot.py's on_ready.
        previous = self.client.setup_hook

        async def setup_hook():
            await previous()
            asyncio.create_task(self.run())

        self.client.setup_hook = setup_hook

    async def run(self) -> None:
        await self.client.wait_until_ready()
        while not self.client.is_closed():
            try:
                await self.tick()
            except Exception:
                log.exception("status watcher")
            await asyncio.sleep(self.EVERY)

    async def tick(self) -> None:
        live = {g.voice_client.channel.id: g.voice_client.channel for g in self.client.guilds if voice.tapping(g) == APP}
        line = None
        if live:
            try:
                np = await bridge.now_playing()
            except BridgeError:
                np = {}
            if np.get("active") and np.get("title"):
                song = f"{np['title']} — {np.get('artist', '')}"
                line = ("🎵 " if np.get("playing") else "⏸ ") + song
                await self.set_presence(song if np.get("playing") else None)
            else:
                await self.set_presence(None)
        else:
            await self.set_presence(None)

        for cid, channel in live.items():
            if self.channel_line.get(cid) != line:
                await self.set_channel(channel, line)
        for cid in [c for c in self.channel_line if c not in live]:
            channel = self.client.get_channel(cid)
            if channel is not None:
                await self.set_channel(channel, None)
            self.channel_line.pop(cid, None)

    async def set_channel(self, channel: discord.VoiceChannel, line: str | None) -> None:
        try:
            await channel.edit(status=_clip(line, self.STATUS_MAX) if line else None)
            self.channel_line[channel.id] = line or ""
        except discord.Forbidden:
            if channel.id not in self.warned:
                self.warned.add(channel.id)
                log.warning("no Set Voice Channel Status permission in #%s; skipping its status", channel.name)
            self.channel_line[channel.id] = line or ""  # don't retry every 5 s
        except discord.HTTPException as e:
            log.warning("voice channel status: %s", e)

    async def set_presence(self, song: str | None) -> None:
        if song == self.presence_line:
            return
        self.presence_line = song
        activity = discord.Activity(type=discord.ActivityType.listening, name=_clip(song, 128)) if song else None
        await self.client.change_presence(activity=activity)
