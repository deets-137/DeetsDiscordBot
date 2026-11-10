---
status: idea
desk_test: none
sources: []
updated: 2026-11-10
---
# A Discord Activity: DeetsMusic in the call

## 1. What he said (2026-11-10)
"Thoughts on making DeetsMusic stream as a live activity for friends to see and interact with
on top of the commands?" Held for later.

## 2. Claude's notes
An Activity is a web page that runs inside the voice channel; everyone in the call can open
it together. Ours would be DeetsMusic's Rooms look: now playing with large artwork, the queue
with drag to reorder, search with artwork, live reactions.

What it takes:
- **Hosting** the page (Cloudflare; use `cf`, not Wrangler, for a new project). Discord
  serves it through its own proxy and URL mappings.
- **A relay**, since the PC is not reachable from the internet: a Worker with a Durable Object
  holds the room's state; the bot opens a connection *out* to it, so no port opens at home.
- **Sign-in**: the Embedded App SDK says who each person is; the relay and the bot check they
  are in the voice channel, then apply the Rooms rules exactly as `/dm` does
  (architecture/PERMISSIONS.md).
- **The audio stays the bot's** in the call.

Open questions, to test before a design:
- Each friend playing on their own Apple Music account inside the Activity (the way DeetsMusic
  Rooms works): Apple's MusicKit sign-in may not work inside Discord's embedded frame.
- The Activity's own permission prompts and how a server enables it.
