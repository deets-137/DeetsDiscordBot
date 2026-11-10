---
status: shipped
desk_test: passed 2026-11-10
sources: [bot/dm.py, bot/voice.py]
updated: 2026-11-10
---
# Who may do what

## 1. The host
**The host** is the bot application's owner (or its team members), plus any user ids in
`VOICE_USER_IDS` in `.env` (`voice.is_host`). The host may use every `/vc` and `/dm` command,
any time. Only the host may start, stop or move the stream (`/vc`), because it plays apps from
this PC into a call.

## 2. A guest
Everyone else. A guest may use `/dm` only when **all** of these are true (`dm.check`):
1. The command runs in a server, not a DM.
2. The bot is playing **deetsmusic.exe** into a voice channel there (`voice.tapping`).
3. The guest is **in that voice channel**.
4. DeetsMusic › Settings › **Rooms** allows guests the action (§3), read live through the
   bridge and cached 10 s. Any value other than `everyone`, or an unreadable rule, means host
   only.

Each refusal says why, in a reply only the guest sees ("Join #General to control the music",
"The host has set “Guests may skip” to host only in DeetsMusic").

## 3. Actions and their rule

| Action | Rooms rule | Notes |
|---|---|---|
| np, queue (and paging) | none: any guest in the call | read only |
| pause, resume | `roomGuests.playPause` | |
| skip, back | `roomGuests.skip` | |
| add (and the picker) | `roomGuests.add` | |
| remove, move | `roomGuests.changeQueue` | |
| play (replace what's playing) | host only | not in Rooms |
| volume (the bot's gain) | host only | |
| seek | `roomGuests.seek` | not offered as a command yet |

All five Rooms rules were "Everyone" on 2026-11-10.

## 4. Buttons
A card's buttons run the same check for the person who presses them. The queue's ◀ ▶ count as
"queue". The picker is visible only to the person who asked.

## 5. Never
- Never offer a guest a route outside BRIDGE.md §2.
- Never let a guest act from outside the call, or while the stream is off.
