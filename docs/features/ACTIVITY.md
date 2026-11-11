---
status: project
desk_test: open
sources: [activity/deetsmusic-protocol/index.ts, activity/deetsmusic-ui/layout.ts, activity/deetsmusic-ui/surface.ts, activity/deetsmusic-ui/ui.css, activity/deetsmusic-ui/album-color.ts, activity/deetsmusic-ui/hint.ts, activity/deetsmusic-ui/row-drag.ts, activity/deetsmusic-ui/toast.ts, activity/deetsmusic-ui/scenery.ts, activity/relay/src/index.ts, activity/relay/src/room.ts, activity/relay/src/wire.ts, activity/discord-shell/main.ts, crates/relay-client/src/lib.rs, crates/logic/src/relay.rs, activity/scripts/look-sync.mjs, activity/mock/transport.ts]
updated: 2026-11-11
---
# The Activity: DeetsMusic's look inside the call

> **Status:** project · 2026-11-11. Build steps 1 and 2 of §10 are built (§13): the bridge
> package's new name, and the UI on fake data. Most of step 2b is built (§13.7). Nothing talks
> to Discord or to the bot yet.
> §1 and §2 are the owner's calls. §3 to §10 are Claude's design inside those calls. §11 holds
> what is not confirmed. The first notes are in [ideas/activity.md](../ideas/activity.md).

**Terms used here.**
- **The Activity**: a web page that Discord shows inside the call, in a frame. Everyone in the
  call can open it.
- **The relay**: a Cloudflare Worker between the Activity and the bot. The PC is not reachable
  from the internet, so the bot connects out to the relay.
- **A surface**: one of DeetsMusic's three compositions, Mini, Midi and Max
  (`../DeetsMusic/docs/cards/SURFACES-AND-CARDS.md` §4).
- **A seat**: a place on the Discord developer team that owns the Lil Deets app.

## 1. What he wants (2026-11-10)
- The people in the call control his DeetsMusic from the Activity. The bot keeps the audio.
- The Activity looks like DeetsMusic, "as one to one as it makes sense for each aspect ratio".
  DeetsMusic's surfaces already fit touchscreens of many sizes. Use them.
- The same UI replaces the Rooms page on deets.solutions later. It is built here first.
- The parts are modular. A part that is particular to DeetsMusic says so in its name.
- The Activity is **private**. No Discord verification, ever (§8).

## 2. His calls

| # | Question | His call |
|---|---|---|
| R | Where the relay lives | A new Worker in this repo. |
| S | The stack | TypeScript and Vite. The site receives a built copy, as the DeetsMusic demo is published. |
| P | Each person's profile | Shows the song (Rich Presence). |
| F1 | Where the 1:1 comes from | New code here. DeetsMusic's look files are copied in by a script. |
| F2 | The bridge crate's name | Rename the package `lildeets-bridge` to `deetsmusic-bridge`. |
| F3 | Which theme and skin show | The host's current theme and skin, the same for everyone. |
| V | Large servers (25 members or more) | Friends get seats on the developer team. "I don't mind everyone being a host since its all close friends anyway." |

This replaces the 2026-10-13 plan for the site (`../DeetsSolutions/docs/rooms-web.md`: the
DeetsMusic repo's own four cards, built by `listen:build`). Mark that doc when the port starts.

## 3. Surfaces: what each frame size shows
The Activity uses DeetsMusic's bands and numbers (`../DeetsMusic/src/surface.ts`).

| What the person sees in Discord | Frame | Surface |
|---|---|---|
| The picture-in-picture tile, or a grid tile | small, about 16:9 (not documented; measure) | The tile card: the cover, the title, the artist, the album. No controls (§13.5 call 3) |
| A phone in portrait, a narrow desktop frame | up to 460 px wide | Mini: Now Playing plus one card. Mini › Player one tap away. |
| A phone in landscape | wider than 460 px, under 500 px high | Wide: the player on the left, one card on the right (§13.5 call 1). Not in the app. |
| A tablet, a medium frame | 461 to 820 px wide | Midi: the Now Playing strip plus two card slots |
| The Activity filling a desktop call | above 820 px wide | Max: the stage column plus the card grid |

- The flips keep DeetsMusic's dead-bands: in below 455 px and out above 465 px for Mini; 40 px
  at the Midi and Max edge. A wide frame between 500 and 745 px high stays Midi. Wide starts
  under 500 px high and ends above 520 px.
- Discord reports the mode (`ACTIVITY_LAYOUT_MODE_UPDATE`: focused 0, picture-in-picture 1,
  grid 2). Picture-in-picture and grid always show the tile card, whatever the size.
- The cards are Now Playing, Queue, Search and History. Library, Playlists, Settings, the Diary
  and the Compass are never in the Activity (CLAUDE.md › Never).
- Phones: the safe-area values (`--discord-safe-area-inset-*`) pad the page. No orientation
  lock.

## 4. The parts and their names
The rule: a part that knows DeetsMusic's data has `deetsmusic` in its name. A part that does
not, never does.

| Part | Path | Knows DeetsMusic? |
|---|---|---|
| The capture | `crates/tap` (`deets-tap`), unchanged | No |
| The relay Worker | `activity/relay` | No. It checks Discord sign-in and the call, then passes messages between one host and its guests. |
| The host's connection to the relay | `crates/relay-client` | No |
| Discord's frame code: SDK, sign-in, layout modes, Rich Presence | `activity/discord-shell` | No |
| The message shapes: track, queue, state, actions | `activity/deetsmusic-protocol` | Yes |
| The cards and surfaces | `activity/deetsmusic-ui` | Yes |
| A fake transport and a dev page, so the UI runs in a browser with no Discord | `activity/mock` | Yes |
| The part of the bot that turns an action into a bridge call | module `deetsmusic_remote` in `crates/logic` | Yes |
| The bridge client | `crates/bridge`, package `deetsmusic-bridge` (F2) | Yes |

`deetsmusic-ui` talks to one **transport** interface: state in, actions out. The Activity's
transport is the relay. The site's transport, later, is the rooms worker plus MusicKit
(`activity/site`, not designed here).

## 5. How an action travels
1. A guest presses ⏭ in the Activity.
2. `discord-shell` sends `{action: "skip"}` on its WebSocket to the relay
   (`<app id>.discordsays.com/.proxy/…`; WebSocket is the only real-time transport Discord allows).
3. The relay knows who the guest is (§6) and passes the action, with the guest's user id, to
   the host's socket for that voice channel.
4. `crates/host` holds that socket. It passes the action to the logic over the IPC, as it
   passes an interaction. The logic is swapped while music plays, so it holds no socket
   (LIVE-RELOAD.md §5).
5. `deetsmusic_remote` runs the same `check` as `/dm` (PERMISSIONS.md), then calls the bridge.
6. The reply goes back the same way. A refusal says why, in the guest's Activity only, with
   the `/dm` wording.

**State** goes the other way. The logic already watches DeetsMusic for the now-playing card
and the status line. On a change it sends one state message (now playing, queue, history,
theme, skin, the five guest rules) to the relay, which sends it to every guest. On a change
only: never on a timer (CARDS.md §4). The progress bar runs in the page from `currentTime`
and a time stamp.

| Action | Bridge route | Rule |
|---|---|---|
| play, pause | `POST /command` | `roomGuests.playPause` |
| skip, back | `POST /command` | `roomGuests.skip` |
| add to queue (next, later) | `POST /queue` | `roomGuests.add` |
| remove, drag to reorder | `POST /queue/edit` | `roomGuests.changeQueue` |
| search | `POST /search`, cached 2 minutes per term as `/dm add` | any guest in the call |
| seek | not on the bot's bridge list yet (BRIDGE.md §2) | `roomGuests.seek`; add the route to BRIDGE.md first |
| play now (replace) | `POST /play` | host only |
| the bot's volume | the host's gain | host only |

A control the guest may not use is drawn as DeetsMusic's Rooms draws it for a guest, not
hidden.

## 6. Security
- **Sign-in.** The Activity asks Discord for a code (`authorize`, scopes `identify` and
  `rpc.activities.write`). The relay trades the code for a token with the client secret, and
  asks Discord who the token belongs to. The page's own claim of a user id is never trusted.
- **The call.** The **bot** asks Discord for the instance
  (`GET /applications/<id>/activity-instances/<instance>`, bot token): its channel and its
  users. The relay sends the question down the bot's own socket (his call A, 2026-11-11: the
  bot token stays on the PC). The guest must be in that list. The relay then compares the
  instance's channel with the channels the bot named.
- **The ticket.** After sign-in the relay gives the page a ticket: the person's id and name,
  signed by the relay, good for five minutes. The page's socket shows it in its first frame.
- **The host's socket.** The bot signs in to the relay with a secret that only the relay and
  the bot's `.env` hold. No host socket for a channel: the relay sends one line ("The host is
  not playing here") and nothing else.
- **The rules.** The relay decides nothing about DeetsMusic. The logic's `check` does, for
  every action, as for a button on a card.
- **Secrets** (his call A, 2026-11-11). The client secret and the host secret are Worker
  secrets (`cf`). The bot token is not on Cloudflare: a leak there cannot control the bot.
  None is in the repo. The bridge token never leaves the PC. (The 2026-11-10 text put the bot
  token on the Worker too.)
- **The origin.** The relay answers `/api/*` only to a page from
  `https://<app id>.discordsays.com`. It logs each refusal with the origin it saw.
- **Text from Discord** (names, nicknames) is drawn with `textContent`.

## 7. The look (F1, F3)
- A script copies DeetsMusic's look files into `activity/deetsmusic-ui/look/`: `palette.css`,
  `themes.css`, `skin.css`, the fonts, and the rules of the four cards and three surfaces from
  `styles.css` and `styles/qcard.css`. It reads `../DeetsMusic`; it never writes there.
- A check compares the copy with the source and fails when they differ. Run it before a
  publish.
- The cards are written new, with DeetsMusic's class names and token names, so the copied
  rules apply as they are.
- The theme and skin come from the host through the bridge (`GET /settings`, cached 60 s, the
  accent's precedent in BRIDGE.md §4) and ride in the state message.
- Apple's artwork host needs a URL mapping in the Developer Portal.

## 8. Private, and the 25-member limit
- **No verification.** The bot plays the host's Apple Music into a call. A Discord review has
  a reason to refuse that, and it ties his identity to the app. His call: private.
- Discord blocks an unverified Activity in a server with 25 or more members, except for the
  app's developer team. Two of his servers are that large (The Lads is one).
- **The app is already owned by a team**: "DeetsTeam", one member, him, as admin (read from
  Discord 2026-11-10). No transfer is needed.
- **His call V: friends get seats.** Use the **Read-only** role: it cannot see the client
  secret or reset the bot token (the Developer role can). Each friend needs two-factor
  sign-in and must be his Discord friend.
- **Every team member is a host in the bot** (`crates/rest/src/lib.rs` `owners`): each seat
  can use `/vc`, which plays any app on his PC into a call. He accepts this (2026-11-10).
  The code counts a member who was invited and has not accepted yet; fix that in the build.
- `/dm` and the cards work in every server, for everyone, with no seat. An action must never
  exist in the Activity only.
- Portal settings, read 2026-11-10: "Public Bot" is on; the app can be installed to a server
  and to a user. Proposed for him: Public Bot off. User Install: see §11.

## 9. Discord features used
- **Rich Presence** (`setActivity`): the song, the artist, the artwork, the elapsed time, on
  each person's profile while they have the Activity open. Set on a song change only.
- **The Launch command.** Discord makes one Entry Point command when Activities are turned
  on. The host's command sync replaces the whole list (BOT.md §6), so the sync must carry
  that command or it deletes it.
- **A button that opens the Activity** on the now-playing card (reply type `LAUNCH_ACTIVITY`).
  A card change: his fork when the build reaches it.
- **Who is here**: the participants list and avatars, for the "added by" marks.
- **Open in Apple Music**: `openExternalLink`.
- Not used: invites, share moments, purchases, thermal state, friends lists, voice events.

## 10. Build order
One part at a time. Each ends with a check he can see.
1. **F2**: rename the bridge package. `cargo clippy`, `cargo test`. A release build is a live
   change: build release when he says.
2. **The mock UI**: `deetsmusic-protocol`, `deetsmusic-ui` and `mock`. The four cards and
   three surfaces on fake data, in a browser at each width in §3. He compares it with
   DeetsMusic side by side.
   **2b** (§13.6): his three calls from the build, and the mock's gaps.
3. **The relay** and **`relay-client`**, with a blank Activity page that only signs in. This
   is the step that answers §11 items 1 to 3.
4. **`deetsmusic_remote`**: state out, actions in, the `check`.
5. **`discord-shell`**: the real UI in the frame, layout modes, Rich Presence, the Launch
   command in the sync.
6. The Sonnet pass (how a user might break it), then his desk test.

New dependencies, each needs his yes: `@discord/embedded-app-sdk`, `vite`, `typescript`
(Node comes into this repo with them); a WebSocket crate for `relay-client`
(`tokio-websockets` and `tokio-tungstenite` are already in `Cargo.lock` through twilight and
songbird).

## 11. Not confirmed
1. **A Read-only seat can launch the Activity in a large server.** Discord's pages say
   "members of the developer team" and name no role. The Teams page does not mention
   Activities. Test at build step 3: one friend, one Read-only seat, one launch in The Lads.
   If it fails, try the Developer role with him (it can reset the bot token).
2. Whether a seat needs Developer Mode on in Discord's settings to see the Activity.
3. The 25-member number. It comes from support pages about 18 months old that refused a
   read on 2026-11-10.
4. Whether anything uses the app's User Install today. If nothing does, turn it off so the
   Activity cannot be opened where the bot is not.
5. The size of the picture-in-picture and grid tiles. Measure at build step 5.

## 12. Desk test
The whole Activity's test is to write when step 5 is built. It must cover: each surface width
in §3; a phone; a guest with each Rooms rule on and off; a person outside the call; a server
without the bot; a logic swap and a host restart while the Activity is open; DeetsMusic closed.

### 12.1 The mock UI (build steps 2 and 2b): passed 2026-11-11
His words: "ran through 17 steps, looks good."

In `activity/`: `npm install` once, then `npm run mock`. Open the address it prints.
1. Open `/frames.html`. Eight frames show: four tiles, a phone in portrait, a phone in
   landscape, a medium frame, a desktop call. Compare the last four with DeetsMusic at that
   size. Each tile shows the cover, the title, the artist and the album, with no control. The
   phone in landscape shows the player on the left and one card on the right.
2. Open `/` in its own window. Drag the window narrower. It flips Max → Midi near 780 px and
   Midi → Mini near 455 px, and back near 860 px and 465 px, with no flicker at an edge.
   Max also needs a window taller than 785 px; a shorter one stays Midi, as in the app.
3. The ⚙ at the bottom left is the mock's desk. Change the theme and the skin: the whole page
   follows. Compare two looks with DeetsMusic.
4. Press ⏯, ⏮, ⏭. Drag the scrub bar. The song, the Queue and History follow.
5. In Search, type a word. Tap a song: it goes to the end of Up Next, and a toast says so.
   Right-click a song (or press and hold on a touchscreen): Play Next, Add to Queue.
6. Right-click a row in the Queue: Play Next, Remove from Queue.
7. On the desk, clear "Guests may skip". ⏮ and ⏭ grey out, and their hint gives the reason.
   Clear "Guests may add songs" and tap a search result: a toast gives the host's line.
   Tick "I am the host": every control works again.
8. Clear "The host is playing here": the title reads "The host is not playing here".
9. On Mini (a narrow window): tap the cover. The player shows alone. The square at its top
   right goes back to the cards. On Midi and Mini, a card's title picks the card in that place.

Step 2b (§13.7) adds these. Use a mouse.
10. Make the window wide and under 500 px high. The player is on the left, one card on the
    right. The card's title picks Queue, Search or History. Make it taller than 520 px: Midi
    comes back.
11. Tick "I am the host". In Search, tap a song. The toast "… is in the queue" has a **Play
    Now** button. Press it: the song plays, and it is not in Up Next a second time. Clear "I am
    the host" and tap a song: the toast has no button.
12. On the desk, tick "Tile". The tile card shows. Drag the window to a tall shape: the cover
    goes on top and the text under it.
13. Set the skin to Glass. The Now Playing card glows in the cover's colours, and its title
    takes a colour from the cover. Press ⏭: the colours fade to the next cover's. Three drifting
    blobs of light show behind the cards.
14. Set the skin to Cyber. Two lightning bolts strike behind the cards, each time in a new place.
15. Rest the pointer on ⏭. After a moment a hint box in DeetsMusic's look says "Plays the next
    song". Rest it on a Queue row with a cut-off name: the hint shows the whole name and the artist.
16. In the Queue, press a row and drag it. A copy of the row follows the pointer and a line
    shows where it lands. Let go: the row is there. Press Escape during a drag: nothing moves.
    Clear "Guests may change Up Next" (and "I am the host"): a row does not drag.
17. On a touchscreen (or Chrome's device mode with touch), as the host: hold a Queue row
    still. After half a second it gets an outline. Move the finger: the row drags, and the
    list does not scroll. Hold a row and let go with no move: its menu opens. Swipe the list
    with no hold: it scrolls, and nothing lifts.

### 12.2 The relay and the sign-in page (build step 3): open
**Setup, once. All five steps are his: they hold secrets, publish, or change the Discord app.**
1. **Write the Worker's three secrets.** Make a file `activity/relay/.env.deploy` (gitignored):
   ```
   DISCORD_CLIENT_ID=<the application id>
   DISCORD_CLIENT_SECRET=<Developer Portal › the app › OAuth2 › Client Secret>
   HOST_SECRET=<a new random value: openssl rand -hex 32>
   ```
2. **Deploy the relay** (a public change). In `activity/relay`: `npm install`, then
   `cf deploy --secrets-file .env.deploy`. It prints the Worker's address,
   `lildeets-relay.<account>.workers.dev`. Delete the file after. A later deploy with no
   secret change is `cf deploy` alone.
3. **Tell the bot.** In the repo's `.env`:
   `LILDEETS_RELAY_URL=wss://lildeets-relay.<account>.workers.dev/host/ws` and
   `LILDEETS_RELAY_SECRET=<the same HOST_SECRET>`.
4. **Build release** (`cargo build --workspace --release`): a live change. The logic's new
   build does not swap in at once this time: the IPC version changed, so the old host refuses
   it and keeps the old logic. The new pair goes live at the host's restart (the next song
   boundary, or at once when nothing plays). The log shows `commands: synced`.
5. **The Developer Portal**, the app "Lil Deets":
   - OAuth2 › Redirects: add `https://127.0.0.1` (Discord's own placeholder).
   - Activities › Settings: turn on **Enable Activities**. Discord makes the Launch command.
   - Activities › URL Mappings: prefix `/`, target `lildeets-relay.<account>.workers.dev`.

**The test.** `LD` is the bot's log; `W` is the Worker's log (the Cloudflare dashboard › the
Worker › Logs).
1. Start the bot. `LD`: `relay: armed; it connects when the bot is in a call`.
2. Join a voice channel in a small server. `/vc play` DeetsMusic. `LD`: `relay: connected`.
   `W`: `[room] the host connected`.
3. Open the Activity: the rocket button in the call, or `/launch`. Discord asks you to
   authorize Lil Deets (once). The page shows **Signed in as** your Discord name, then **Lil
   Deets is in this call.** `LD`: `relay: a person is in the Activity`.
4. `/vc leave`. The page changes to **The host is not playing here**. `LD`: `relay:
   disconnected; the bot is in no call`.
5. `/vc play` again. The page changes back to **Lil Deets is in this call.** with no reload.
6. Open the Activity in a voice channel where the bot is not. The page shows your name and
   **The host is not playing here**.
7. Stop the bot (Ctrl+C), then open the Activity. The page shows your name and **The host is
   not playing here**. Start the bot and `/vc play`: the page changes by itself.
8. Restart the host during a call (LIVE-RELOAD.md §7). After the bot is back, the page shows
   **Lil Deets is in this call.** again.
9. Run `/dm np`, `/vc` and `/links` once: the command sync kept them, and **Launch** is still
   in the command list after a bot restart.
10. **§11 item 1:** a friend with no team seat opens the Activity in the small server. Then a
    friend with a Read-only seat opens it in The Lads. Write down what each one sees.
11. **§11 item 2:** note whether a seat needed Developer Mode on.

If sign-in fails with "Not from the Activity", `W` shows the origin the relay saw
(`[relay] refused … origin …`): Discord's proxy sent an origin the relay does not expect.

## 13. As built (2026-11-10)

> **Part:** built · 2026-11-10. Steps 1 and 2 of §10. Checked by Claude in Chrome on this PC
> (§13.3). His desk test is §12.1.

### 13.1 The files
| Part | Files |
|---|---|
| Step 1, the name | `crates/bridge/Cargo.toml` (`deetsmusic-bridge`); the `use` lines in `crates/host`, `crates/logic`, the bridge's tests. The folder stays `crates/bridge`. |
| `deetsmusic-protocol` | `activity/deetsmusic-protocol/index.ts`: `Track`, `HostState`, `Action`, `Transport`, the rule for each action (`ACTION_RULE`, `allowed`). |
| `deetsmusic-ui` | `index.ts` (`mountDeetsMusicUI(transport)`), `layout.ts` (the page skeleton, which card in which slot), `surface.ts` (the bands), `look.ts` (theme, skin, the skin hooks), the four cards, `rows.ts`, `menu.ts`, `slider.ts`, `toast.ts`, `store.ts`, `ui.css`. |
| The look | `activity/deetsmusic-ui/look/`: 60 files copied from DeetsMusic 1.4.2 by `scripts/look-sync.mjs` (all of `src/styles.css` and `src/styles/`, the fonts, the images a stylesheet names). `MANIFEST.json` holds each file's hash. **Never edit a file there.** |
| `mock` | `mock/index.html`, `main.ts` (the desk), `transport.ts` (a fake host that obeys the rules), `data.ts`, `frames.html`, `vite.config.ts`. |

Commands, in `activity/`: `npm run mock`, `npm run check` (types, then the look against
DeetsMusic), `npm run look:sync` (copy the look again after a DeetsMusic change),
`npm run mock:build`.

### 13.2 Decided inside his calls (for his review)
1. **The whole stylesheet set is copied**, not only the four cards' rules. The rules lean on
   each other; a part of them would break in ways a check cannot see. Cost: 1.8 MB in the repo.
2. **Max is DeetsMusic's two-card layout** (`data-max-cards="2cols"`): Now Playing over the
   Queue, Search and History beside them. The precedent is DeetsMusic's own Rooms web port.
3. **Midi opens with Queue and Search; Mini opens with Queue.** A card's title picks another,
   as in the app. Now Playing's Queue and Search squares bring that card in.
4. **Mini's player:** a tap on the cover opens it; a square at its top right goes back. The
   app does both from its Surface menu, which a frame does not have.
5. **A tap on a search result adds the song to the end of the queue.** In the app a click
   plays the song now, which is host only for a guest. The menu has Play Next and Add to
   Queue, and Play Now for the host.
6. **A row's menu opens on a right-click or a long press** (450 ms), so a touchscreen has it.
7. **A control the rules forbid stays on screen, greyed**, with the `/dm` refusal as its hint.
8. **The title bar** shows the app's name and "Host" or "Guest". No menu, no window buttons.
9. **Left out of Now Playing:** Shuffle and Repeat (the bridge has no route), Favorite, Add
   to Library, AirPlay, the Last.fm counts, and the volume row (the bot's volume is the
   host's; each person sets Lil Deets's level for themself in Discord).
10. **Search shows songs only**, with no category filter and no recents: the bot's bridge
    call asks for songs only (BRIDGE.md §2).
11. **The tile is the cover alone**, as DeetsMusic's pin › Cover. The controls show on hover.
    Replaced on 2026-11-11 by his call 3 (§13.5): the tile card (§13.7).

### 13.3 Checked (Claude, Chrome, 2026-11-10)
On `npm run mock`: Max, Midi, Mini, the player and the cover draw in DeetsMusic's look
(Lilac × Press; also Sepia × Ocean, Moonlight × Glass, Black & Red × Cyber). Pause, next, the
Search square, a search, an add with its toast, a refused add with the host's line, ⏭ greyed
under "Guests may skip", the cover tap and the title picker all work.
`cargo clippy --workspace --all-targets` and `cargo test --workspace` pass after the rename.
`npm run check` and `npm run mock:build` pass. NOT checked: a real phone, a touch long press,
the Metal and Happy skins.

### 13.4 Known gaps in the mock UI
As of 2026-11-10. §13.7 says which of these step 2b closed.
- **No album colour.** The app reads colours from the cover for the Now Playing card
  (`album-color.ts`). On Glass the song's title is faint without them.
- **No skin scenery.** The Ocean sea, the Cyber storm, the Glass aurora and wallpaper, the
  Happy scene and the Press record's turn are driven by the app's code. Their layers are not
  on the page; the cards, the type and the colours are.
- **No drag to reorder** in the Queue. The menu's Play Next and Remove stand in.
- **A phone in landscape** (760 × 390) is Midi by DeetsMusic's bands, and the two cards get
  about 120 px of height. The app never meets a window that short. A fork for him (§13.5).
- **No hints.** The app's hover hint box is not ported; controls carry plain `title` text.
- `npm install` printed that esbuild's install script was blocked. Vite runs and builds.

### 13.5 Forks found by the build — his calls (2026-11-10)
| # | Question | His call |
|---|---|---|
| 1 | A short, wide frame (a phone in landscape) | "1A works": Now Playing on the left and one card on the right. A new composition, not in the app. |
| 2 | A tap on a search result | "2A with a toast on the desktop/web version to play now": the tap adds the song to the end of the queue. On desktop and web, the toast that confirms it carries a **Play Now** button. |
| 3 | The tile (picture-in-picture, grid) | "a unique player card with cover, title, album, and artist can scale the cover down to fit it all in and make it neat." Not the cover alone. |

**All three are built** (2026-11-11, §13.7).

**One question on call 2 (open).** Play Now replaces what plays. PERMISSIONS.md §3 makes
that host only, and DeetsMusic's Rooms has no rule for it. As the rule stands, only the host
sees the toast's Play Now button. If guests should have it, that is a new permission: his
call, and PERMISSIONS.md changes first.

### 13.6 Step 2b: the gaps and his three calls
> **Part:** designed · 2026-11-10. His question: "Can those gaps be fixed in any way?" Yes,
> each one. This table is the plan. §13.7 is what was built from it, and what was not.

| Item | How | Size, and what to check first |
|---|---|---|
| Album colour (§13.4) | Port DeetsMusic's `src/album-color.ts`: it reads the cover on a canvas and sets `--album-bg`, `--album-c1`, `--album-c2` and the `np--album` class. | About 260 lines. The canvas needs the cover with CORS. In Discord the cover comes through a URL mapping, so it is same-origin there; check it at step 5. |
| Skin scenery (§13.4) | Put the layers from DeetsMusic's `index.html` on the page (ocean, wallpaper, aurora, storm, scene), then port each driver: `ambient.ts`, `storm.ts`, `ocean.ts` (with its worker), `scene.ts`, `vinyl.ts` (the Press record's turn). | The largest item. Read each driver's imports first: one that reaches into the player or the settings store needs a small stand-in. Order by what he uses: the Press record, then Glass, Cyber, Ocean, Happy. Each must stop when the frame is hidden (the app's `ambient.ts` rule). |
| Drag to reorder (§13.4) | Port `src/row-drag.ts` (pointer events, so a finger works) onto the Queue's rows; send `queueMove` on the drop. | About 600 lines in the app. On a touchscreen a drag must start from a press-and-hold or a grip, or it fights the list's scroll and the long-press menu. |
| Hints (§13.4) | Port `src/hint.ts`; its stylesheet is already in the look. Each `title` becomes `data-hint`. | About 300 lines. No hover on a touchscreen: hints are for desktop and web. |
| Call 1: the landscape composition | A fourth composition for a frame under about 500 px high and over 460 px wide: Now Playing (the player view) on the left, one pickable card on the right. | New rules in `ui.css` and one more case in `surface.ts` and `layout.ts`. The height edge needs its own dead-band. |
| Call 2: the Play Now toast | The toast gains DeetsMusic's `toast__actions` row with one `toast__btn`. Shown when the pointer is a mouse (`matchMedia("(hover: hover)")`) and the person may play now. | Small. Waits on the question above only for guests. |
| Call 3: the tile card | A new composition for Discord's picture-in-picture and grid modes: the cover, then title, artist and album beside or under it, the cover scaled so all four fit at any tile size (a container query on the frame). No controls: the tile is not clickable by default. | New rules in `ui.css`; `setCover` becomes `setTile`. Measure Discord's real tile sizes at step 5 (§11 item 5). |

### 13.7 Step 2b as built (2026-11-11)
> **Part:** built · 2026-11-11, in part. His word: "Go to step 2b". Checked by Claude in Chrome
> on this PC. His desk test is §12.1 steps 10 to 17. Not built: the scenery that needs a large
> port.

| Item | Built | Files |
|---|---|---|
| Call 1, the landscape layout ("Wide") | Yes | `surface.ts` (`wideFor`, `data-wide`), `layout.ts`, `ui.css` |
| Call 2, the Play Now button on the toast | Yes | `toast.ts` (the app's timed toast with a button row), `search-card.ts` |
| Call 3, the tile card | Yes | `surface.ts` (`setTile`, `data-tile`), `ui.css`. `pin.css` is not loaded now. |
| Album colour | Yes | `album-slots.ts`, `album-color.ts`, `now-playing-card.ts` |
| Hints | Yes | `hint.ts` |
| Drag to reorder in the Queue | Yes: a mouse, a pen, and a finger (hold, then move) | `row-drag.ts`, `queue-card.ts`, `menu.ts` |
| Skin scenery | Glass's aurora and Cyber's storm only | `scenery.ts`, `layout.ts` (the two layers) |

**Decided inside his calls (for his review).**
1. **Wide starts under 500 px high** on a frame wider than 460 px, and ends above 520 px. The
   player gets 5 parts of the width and the card 7. The card's title picks Queue, Search or
   History. Now Playing's Queue and Search squares pick that card too.
2. **Play Now on the toast removes the copy that the tap added.** The tap put the song at the
   end of Up Next. Play Now plays it, then removes that last copy, so the song is not there
   twice. The button shows only where a pointer can rest on it (`hover: hover`) and only to a
   person who may play a song now. As PERMISSIONS.md §3 stands, that is the host.
3. **A toast holds 3.2 s** (the app's time), and 6 s when it has a button. The pointer on it
   holds the clock, as in the app.
4. **The tile's layout follows its shape.** Wider than 5:4: the cover on the left, the text
   beside it. Else: the cover on top, the text under it, centred. The type scales with the
   tile, up to the app's sizes. The Now Playing card's aurora layer stays on the tile.
5. **Album colour reads the cover's pixels.** §13.6 said the app reads the cover on a canvas.
   That was wrong: the app asks its Rust side for Apple's palette, and the bridge sends no
   palette. The page uses the app's own rule for a picture with no Apple palette
   (`paletteFromPixels`): one 48 px image per album from Apple's image server, 0 API calls.
   Apple's server sends `Access-Control-Allow-Origin: *` (checked 2026-11-11), so the canvas
   may be read. The colours can differ a little from the app's for the same album. The exact
   palette needs a new field from DeetsMusic's bridge: a change in DeetsMusic, not here.
6. **Hints use the app's defaults:** on, the 600 ms wait, a row's hint only when a name is cut
   off. A frame has no settings for them.
7. **A drag moves a row inside Up Next only.** The app also carries a song from Search into
   the Queue at a chosen place. The protocol has no "add at this place", so that is not built.
   The row shows in its new place at once; a refusal from the host puts it back.
8. **The scenery pauses while the frame is hidden** (`visibilitychange`), and the page logs
   each pause and start to the console.

**How a finger starts a drag: his call, "A: hold, then move"** (2026-11-11). Built the same
day in `row-drag.ts`, `menu.ts` (`holdElsewhere`) and `queue-card.ts`. Inside his call:
the lifted row shows the look's outline (`is-context`) and the phone gives a short buzz where
it can; a move of more than 10 px before the 450 ms ends is a scroll, and nothing lifts; a
person who may not change Up Next gets the menu at 450 ms as before, with each line greyed.
In Search and History a long press still opens the menu at 450 ms: their rows do not drag.
Checked with simulated finger events in Chrome: the drag, the menu on a still release, the
scroll, the guest's menu. NOT checked on a real touchscreen: whether the list stays still
under a lifted row (the page refuses the scroll through `touchmove`).
The two ways that passed the test were:
- **A. Hold, then move** (recommended). A press held 450 ms lifts the row. A move then drags
  it. A release with no move opens the menu, as now. No new control. The precedent is
  DeetsMusic's own hold mode (`row-drag.ts` `hold`, MOVABLE-ROWS.md). Cost: a person must
  learn the gesture.
- **B. A grip.** On a touchscreen each Up Next row shows a grip at its right. A drag from the
  grip starts at once. Cost: a new control that the app does not have, and less room for the
  song's name.

**Not built: the rest of the scenery.** Each needs a larger port of the app's driver, with
stand-ins for what the driver reads from the player and the settings store.
| Scenery | The app's driver | Size |
|---|---|---|
| The Press record's turn and slide | `vinyl.ts` | 701 lines |
| Glass's wallpaper of covers | `wallpaper.ts` | not sized |
| Ocean's sea | `ocean.ts`, its worker, `ocean-texture.ts` | 473 lines and two more files |
| Happy's sky, sun and clouds | `scene.ts`, `sky-phase.ts`, `look-schedule.ts` | 398 lines and two more files |

**Checked (Claude, Chrome, 2026-11-11).** `/frames.html`: the four tiles (320 × 180,
240 × 240, 160 × 90, 200 × 300) and the Wide frame (760 × 390) draw with nothing cut off, on
Lilac × Press. On Max, Moonlight × Glass, as the host: the Now Playing card took the cover's
colours; a tap on a search result gave the toast with Play Now; Play Now played the song and
Up Next went from 10 songs back to 9; a drag moved the first Up Next row to the fifth place,
in the page and in the fake host; the hint on ⏭ showed. On Black & Red × Cyber the storm layer
ran. No console error. `npm run check` and `npm run mock:build` pass. NOT checked: a real
phone, a touchscreen, a pen, the tile at Discord's real sizes, the hint on a cut-off row, the
Wide dead-band by hand, a refused drag.

### 13.8 Step 3 as built (2026-11-11)
> **Part:** built · 2026-11-11. His word: "Step 3, the relay". Not deployed, and no release
> build: nothing is live. Tested on this PC against a fake Discord. His setup and desk test
> are §12.2.

**His calls.** Secrets: "A: secret on Cloudflare" (the client secret on the Worker, the bot
token on the PC). Dependencies: "Yes, all three", then "Yes, in activity/relay" for
Cloudflare's own tool set.

| Part | Files |
|---|---|
| The relay Worker | `activity/relay/`: `cloudflare.config.ts`, `src/index.ts` (the routes), `src/room.ts` (the room, a Durable Object), `src/ticket.ts`, `src/wire.ts` (the messages), `test/relay.test.mjs` |
| The sign-in page | `activity/relay/index.html`, `activity/discord-shell/main.ts` |
| The bot's connection | `crates/relay-client` (`lildeets-relay-client`) |
| The host | `crates/host/src/main.rs`: `tell_relay`, `on_relay`, the two `.env` keys |
| Host ⇄ logic | `crates/ipc`: `RelayCheck`, `RelayChecked`; `PROTOCOL` is 2 |
| The check | `crates/logic/src/relay.rs`; `crates/rest`: `activity_instance`, `get_global_commands` |
| The command sync | `crates/logic/src/commands.rs` `sync_commands` |
| The §8 fix | `crates/rest/src/lib.rs` `owners`: accepted team members only |

**How a sign-in travels.**
1. The page asks the relay for the app's id (`GET /api/config`) and starts Discord's SDK.
2. Discord gives the page a code (`authorize`, scopes `identify`, `rpc.activities.write`).
3. The page sends the code to the relay (`POST /api/token`). The relay trades it at Discord
   with the client secret, asks Discord who it is, and returns Discord's token, the person,
   and a ticket.
4. The page opens a socket (`/api/ws`) and sends the ticket and Discord's instance id.
5. The relay sends `check` down the bot's socket. The host passes it to the logic. The logic
   asks Discord for the instance (once more after 1.5 s when the person is not listed yet)
   and answers with the instance's channel, or with a line for the person.
6. The relay tells the page `welcome`, with whether the bot is in that channel now. After
   that it sends `host` each time that answer changes.

**Decided inside his calls (for his review).**
1. **One Worker serves the page and the relay**, at a `workers.dev` address. Discord's proxy
   hides the address from the people in the call.
2. **One room for the whole app** (one Durable Object, `storage: "sqlite"`, WebSocket
   hibernation). It keeps nothing in memory: each socket's state is in its attachment. Limits:
   200 pages, 4 KiB a frame from a page.
3. **The bot connects only while it is in a call.** It tries again after 2 s, then up to 60 s
   apart. A `ping` every 30 s is answered by the relay without waking the room. It logs
   `relay: armed`, `connected`, `the connection dropped`, `disconnected`, `stopped`.
4. **With no bot connected, a signed-in person waits.** They see "The host is not playing
   here". When a bot connects, the relay checks each waiting person. A new bot connection
   replaces the old one, and everyone is checked again.
5. **The command sync keeps the Launch command.** It reads Discord's list first and carries
   the Entry Point command as Discord holds it. When there is none and `.env` has the relay,
   it makes Discord's default (`launch`, "Launch an activity"); if Discord refuses that, the
   list goes without it. When the read fails, there is no sync at that start.
6. **The lines a person can read on the page** (a test page; step 5 replaces it): "Signing
   in…", "Signed in as <name>", "Lil Deets is in this call.", "The host is not playing here",
   "Open this from Discord", and the refusals: "Lil Deets is starting. Close the Activity and
   open it again.", "Discord did not answer. Close the Activity and open it again.", "Discord
   does not know this Activity. Close it and open it again.", "Discord says you are not in
   this Activity. Close it and open it again.", "The Activity works in a server's voice
   channel.", "The sign-in ran out. Close the Activity and open it again.", "This page is out
   of date. Close the Activity and open it again."
7. **`cf dev` talks to a fake Discord only** (`cloudflare.config.ts`, mode `development`), so a
   local run can never send a real secret anywhere.
8. **Two more direct Rust dependencies than he approved by name:** `futures-util` and `http`.
   `tokio-websockets`' API is written in their traits and types. Both were in `Cargo.lock`
   already; no new crate is compiled. His word, 2026-11-11: "Those crates are ok by me."

**Checked (Claude, 2026-11-11).** `activity/relay`: `npm test`, 13 steps against `cf dev` and
a fake Discord (an origin that is not the Activity; a bad code; a forged ticket; a wrong host
secret; no host; the check; the bot coming and going; a refusal; a second frame; a second
host). The test found one defect, now fixed: after a bot reconnect the room sent the check to
the old, closing socket. `cf deploy --dry-run` passes. `cargo clippy --workspace --all-targets`
and `cargo test --workspace` pass (debug): 3 new tests in `lildeets-relay-client` (one against
a local fake relay), 1 new and 1 changed in `lildeets-rest`. NOT checked: anything against the
real Discord or the real Cloudflare; the page inside Discord's frame (the `/.proxy` path, the
origin header through Discord's proxy, the consent prompt); the Launch command's carry-over
against Discord; the bot's client against the deployed Worker over TLS.
