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

### 1.1 His calls (2026-11-10, the design sitting)
- "Let's build out the discord live activity to allow users to control my music." The Activity
  controls the host's DeetsMusic. The bot keeps the audio.
- "I want to redo the rooms ui in deets solutions too, but we can build it from scratch here
  and port it to the website later."
- "I want to replicate DeetsMusic's UI as one to one as it makes sense for each aspect ratio.
  I'm of the opinion DeetsMusic's core UI is great for touchscreens of varying sizes and
  handling the dynamic sizing problem as currently."
- Names: the host piece is particular to DeetsMusic. Keep the parts modular, and name each
  part for what it is particular to ("say I wanted to productize just the tap or another
  piece of all this, keeping names accurate helps").
- The relay: "new worker in this repo works".
- The stack: "typescript and vite works with the website receiving a copy like our demo is
  built".
- Each person's profile: "we can show the song" (Rich Presence).

- The look: new code here, DeetsMusic's look files copied in by a script ("F1-A"). The bridge
  package is renamed `deetsmusic-bridge` ("F2-A"). The host's theme and skin show ("F3-A").
- "I would like it to be a private activity regardless": no Discord verification.
- Large servers: friends get seats on the developer team. "I don't mind everyone being a host
  since its all close friends anyway."

- After the mock UI: "1A works, 2A with a toast on the desktop/web version to play now, 3 a
  unique player card with cover, title, album, and artist can scale the cover down to fit it
  all in and make it neat." (the landscape composition; a tap adds to the queue; the tile.)
- "Can those gaps be fixed in any way?" "I can also continue dev on a Mac right?"

**The design is [features/ACTIVITY.md](../features/ACTIVITY.md).** §2 below is the first
notes, kept as a record.

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
