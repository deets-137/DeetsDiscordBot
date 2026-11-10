"""
A small async client for DeetsMusic's loopback bridge — the same HTTP surface its
`deetsmusic` CLI and MCP use (DeetsMusic docs/integrations/AGENT.md §3). Read-only
toward DeetsMusic's code: nothing here changes the app, it only calls its routes.

- Address: 127.0.0.1, the first of ports 47825–47828 that answers.
- Auth: `Authorization: Bearer <bridgeToken>` from
  %APPDATA%\\com.deetsmusic.app\\settings.json, re-read when a call gets 401.
- Every call runs in a worker thread (urllib), so the bot's event loop never blocks.
"""

import asyncio
import json
import os
import time
import urllib.error
import urllib.request
from pathlib import Path

SETTINGS = Path(os.path.expandvars(r"%APPDATA%\com.deetsmusic.app\settings.json"))
PORTS = range(47825, 47829)

# Accent colour per DeetsMusic theme: the theme's own "go" light from
# src/styles/themes.css (moonlight's is near-white, so its title blue instead).
THEME_ACCENT = {
    "lilac": 0x2FB089,
    "green": 0x4C9A46,
    "sepia": 0x6B8E4E,
    "moonlight": 0xDFECFC,
    "black-yellow": 0xFFD400,
    "black-red": 0xFF4D45,
}
DEFAULT_ACCENT = 0xD4537E

# DeetsMusic Settings › Rooms: what a guest may do (value "everyone" or host only).
RULE_KEYS = ("roomGuests.playPause", "roomGuests.skip", "roomGuests.seek", "roomGuests.add", "roomGuests.changeQueue")


class BridgeError(Exception):
    """A failure worth showing the user as-is."""


class Bridge:
    def __init__(self):
        self._port: int | None = None
        self._token: str | None = None
        self._rules: tuple[float, dict[str, str]] = (0.0, {})
        self._accent: tuple[float, int] = (0.0, DEFAULT_ACCENT)
        self._search_cache: dict[str, tuple[float, list[dict]]] = {}

    # --- transport -------------------------------------------------------------

    def _read_token(self) -> str:
        try:
            token = json.loads(SETTINGS.read_text(encoding="utf-8")).get("bridgeToken")
        except (OSError, ValueError):
            token = None
        if not token:
            raise BridgeError("DeetsMusic isn't set up on this PC (no bridge token).")
        return token

    def _request(self, port: int, path: str, body, timeout: float):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(
            f"http://127.0.0.1:{port}{path}",
            data=data,
            headers={"Authorization": f"Bearer {self._token}", "Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read() or b"{}")

    def _call_sync(self, path: str, body=None, timeout: float = 20.0):
        if self._token is None:
            self._token = self._read_token()
        ports = [self._port] if self._port else list(PORTS)
        for attempt in range(2):
            for port in ports:
                try:
                    out = self._request(port, path, body, timeout)
                    self._port = port
                    return out
                except urllib.error.HTTPError as e:
                    if e.code == 401 and attempt == 0:
                        self._token = self._read_token()
                        break
                    try:
                        msg = json.loads(e.read() or b"{}").get("error")
                    except ValueError:
                        msg = None
                    raise BridgeError(msg or f"DeetsMusic said {e.code}.")
                except (urllib.error.URLError, ConnectionError, TimeoutError, OSError):
                    continue
            else:
                if self._port and attempt == 0:
                    self._port, ports = None, list(PORTS)  # the app restarted on another port
                    continue
                raise BridgeError("DeetsMusic isn't running.")
        raise BridgeError("DeetsMusic isn't running.")

    async def call(self, path: str, body=None, timeout: float = 20.0):
        return await asyncio.to_thread(self._call_sync, path, body, timeout)

    # --- reads -----------------------------------------------------------------

    async def now_playing(self) -> dict:
        return await self.call("/now-playing")

    async def queue(self) -> dict:
        return await self.call("/queue")

    async def search_songs(self, term: str, timeout: float = 20.0) -> list[dict]:
        key = term.strip().lower()
        hit = self._search_cache.get(key)
        if hit and time.monotonic() - hit[0] < 120:
            return hit[1]
        out = await self.call("/search", {"term": term, "types": ["songs"]}, timeout=timeout)
        songs = out.get("songs") or []
        self._search_cache[key] = (time.monotonic(), songs)
        if len(self._search_cache) > 200:
            self._search_cache.pop(next(iter(self._search_cache)))
        return songs

    async def rules(self) -> dict[str, str]:
        """Rooms guest rules, cached 10 s. Unknown or unreadable → host only."""
        at, rules = self._rules
        if time.monotonic() - at < 10 and rules:
            return rules
        try:
            out = await self.call("/settings?section=rooms", timeout=5)
            rules = {r["key"]: r.get("value") for r in out.get("settings", []) if r.get("key") in RULE_KEYS}
        except BridgeError:
            rules = {}
        self._rules = (time.monotonic(), rules)
        return rules

    async def accent(self) -> int:
        at, colour = self._accent
        if time.monotonic() - at < 60:
            return colour
        try:
            out = await self.call("/settings", timeout=5)
            theme = next((r.get("value") for r in out.get("settings", []) if r.get("key") == "theme"), None)
            colour = THEME_ACCENT.get(theme or "", DEFAULT_ACCENT)
        except BridgeError:
            pass
        self._accent = (time.monotonic(), colour)
        return colour

    # --- writes ----------------------------------------------------------------

    async def command(self, kind: str, value=None) -> dict:
        body = {"kind": kind}
        if value is not None:
            body["value"] = value
        return await self.call("/command", body)

    async def enqueue(self, song_id: str, next_up: bool) -> dict:
        return await self.call("/queue", {"id": song_id, "mode": "next" if next_up else "later"})

    async def play(self, song_id: str) -> dict:
        return await self.call("/play", {"id": song_id})

    async def queue_edit(self, action: str, index: int, to: int | None = None) -> dict:
        body = {"action": action, "index": index}
        if to is not None:
            body["to"] = to
        return await self.call("/queue/edit", body)


def artwork(track: dict | None, size: int = 300) -> str | None:
    """A sized artwork URL from a Track (`artwork.urlTemplate`) or now-playing (`artworkTemplate`)."""
    if not track:
        return None
    tmpl = (track.get("artwork") or {}).get("urlTemplate") or track.get("artworkTemplate") or track.get("artworkUrl")
    if not tmpl:
        return None
    return tmpl.replace("{w}", str(size)).replace("{h}", str(size))


def mmss(seconds: float | None) -> str:
    if seconds is None:
        return "–:––"
    s = int(round(seconds))
    return f"{s // 60}:{s % 60:02d}"
