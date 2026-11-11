---
status: shipped
desk_test: passed 2026-11-10
sources: [crates/logic/src/commands.rs]
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
| play (replace what's playing): `/dm play`, the Activity's Play Now | `roomGuests.skip` and `roomGuests.add` | his call 3B, 2026-11-11 (§6) |
| volume (the bot's gain) | host only | |
| seek | `roomGuests.seek` | the Activity's scrub bar; no `/dm` command |
| search (the Activity's Search card) | none: any guest in the call | read only, as `/dm add`'s suggestions |

All five Rooms rules were "Everyone" on 2026-11-10.

## 4. Buttons
A card's buttons run the same check for the person who presses them. The queue's ◀ ▶ count as
"queue". The picker is visible only to the person who asked.

## 5. Never
- Never offer a guest a route outside BRIDGE.md §2.
- Never let a guest act from outside the call, or while the stream is off.
- Never give the Activity an action that `/dm` or a card does not have (ACTIVITY.md §8).

## 6. Play Now for guests (his call 3B, 2026-11-11)
Play Now replaces the song that plays, so it is a kind of skip. DeetsMusic's Rooms has no rule
of its own for it, so it follows **Guests may skip**. It holds for `/dm play` and for the
Activity alike, so neither has an action the other lacks. Before this call, play was host
only (2026-11-10).

Two limits keep it inside the Rooms rules (found by the build's review, 2026-11-11):
- **It also needs "Guests may add songs"**: the guest picks the song that plays. Without
  this, a guest with skip on and add off could play any song.
- **A guest's play keeps Up Next.** DeetsMusic's plain `/play` replaces Up Next too, which
  would need "Guests may change Up Next". A guest's play is sent with `keepQueue`: the song
  goes in at the top of Up Next and plays, and the rest stays. The host's `/dm play` replaces
  Up Next, as before; the Activity's Play Now keeps it for everyone.

## 7. The Activity
An action from the Activity runs the same `check` as a `/dm` command (`commands.rs`
`check_for`). The person is the one Discord signed in (ACTIVITY.md §6); the server is the one
whose call the bot is in, in the person's Activity channel. The refusal is the same line, shown
as a toast in that person's Activity only.
