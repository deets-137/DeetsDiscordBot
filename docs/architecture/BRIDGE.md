---
status: shipped
desk_test: passed 2026-11-10
sources: [crates/bridge/src/lib.rs]
updated: 2026-11-10
---
# DeetsMusic's bridge, as the bot uses it

DeetsMusic serves a small HTTP API on loopback for its CLI, its MCP server and its browser
extension. The bot is one more client. The source of truth is DeetsMusic's
`docs/integrations/AGENT.md` §3; this doc lists only what the bot calls.

## 1. Finding it
- Address `127.0.0.1`, the first of ports **47825–47828** that answers. The port is cached; on a
  failure the client tries all four again (DeetsMusic may have restarted on another).
- Auth: `Authorization: Bearer <bridgeToken>`, read from
  `%APPDATA%\com.deetsmusic.app\settings.json` (`bridgeToken`). Re-read once on a 401. **The
  token is never copied into this repo.**
- Every call runs in a worker thread (`asyncio.to_thread` + urllib), so the event loop never
  blocks. Errors become a `BridgeError` with DeetsMusic's own message, shown to the user.

## 2. Routes used

| Call | Route | Used by |
|---|---|---|
| `now_playing()` | `GET /now-playing` | np card, status watcher, the add line's "plays in" |
| `queue()` | `GET /queue` → `{current, upcoming, history}` | queue card, add line, remove/move |
| `search_songs(term)` | `POST /search {term, types:["songs"]}` | `/dm add` and `/dm play` suggestions |
| `command(kind)` | `POST /command {kind}`: `play` `pause` `next` `previous` | pause, resume, skip, back, np buttons, the Activity |
| `command("seek", value)` | `POST /command {kind: "seek", value: 0..1}` (a fraction of the song) | the Activity's scrub bar (added 2026-11-11) |
| `appearance()` | `GET /settings` (rows `theme`, `skin`) | the Activity's look, cached 60 s with the accent |
| `enqueue(id, next)` | `POST /queue {id, mode: next \| later}` | `/dm add`, the picker |
| `play(id, keep_queue)` | `POST /play {id[, keepQueue: true]}`: without `keepQueue` DeetsMusic replaces Up Next; with it the song goes in at the top and the rest stays | `/dm play` (the host: replace; a guest: keep), the Activity's Play Now (keep) |
| `queue_edit(action, n, to)` | `POST /queue/edit {action: remove \| move, index, to}` | `/dm remove`, `/dm move` |
| `rules()` | `GET /settings?section=rooms` | permissions, cached 10 s |
| `accent()` | `GET /settings` (row `theme`) | the cards' accent colour, cached 60 s |

**Never called from Discord:** library, playlist, folder, settings writes, picks, diary,
rules, history, query, update. A guest must never reach those.

## 3. Shapes worth knowing
- A **Track** (queue, search): `catalogId`, `title`, `artistName`, `albumName`, `durationMs`,
  `contentRating` (`explicit` | `clean` | absent), `artwork.urlTemplate` (`{w}x{h}`).
- **Now playing** has `title`, `artist`, `album`, `artworkTemplate`, `currentTime`,
  `duration`, `progress`, `playing`, `active` — but **no `contentRating`**; take it from
  `queue().current`.
- Ids go back as `song:<catalogId>`.
- A write's reply may carry `notices: [{kind, text}]` (DeetsMusic's warn/error toasts); the bot
  appends them as `-# ⚠ …` lines.
- `search_songs` caches 2 minutes per term (autocomplete asks on every keystroke).
- `queue().history` is **oldest first** (DeetsMusic's `queue.ts` pushes each played song). The
  Activity shows it newest first.
- `/queue/edit` rows are 1-based; `move` takes the song out, then puts it in at row `to`.

## 4. Theme accents
`THEME_ACCENT` maps DeetsMusic's theme id to its "go" light from `src/styles/themes.css`
(moonlight: its title blue, since its "go" is near white). A new theme in DeetsMusic needs a
row here, or the cards fall back to `#D4537E`.
