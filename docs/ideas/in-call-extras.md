---
status: idea
desk_test: none
sources: []
updated: 2026-11-10
---
# In-call extras

## 1. What he said (2026-11-10)
Asked about ways for people to interact in the voice channel. Chose "vc status and profile
status now, hold on the rest for later". Status is built (features/STATUS.md). The rest is
here.

## 2. Claude's notes
- **The card in the call's own chat.** Every voice channel has a text chat. `/vc play` posts
  the np card there and keeps it (edit on change), so the controls sit where people listen.
- **Vote to skip.** For guests, ⏭ counts votes; it skips when, say, half the listeners (not
  the bot) have pressed it. The host skips at once. A fourth Rooms-style rule, kept in the bot
  (DeetsMusic's Rooms has no vote rule), or a request to DeetsMusic for one.
- **Not recommended: voice commands.** The bot would have to hear and transcribe everyone in
  the call (it joins deafened today). That needs friends' consent and DAVE decryption of the
  received audio.
