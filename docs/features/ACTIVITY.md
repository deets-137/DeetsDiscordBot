---
status: project
desk_test: open
sources: [activity/deetsmusic-protocol/index.ts, activity/deetsmusic-ui/layout.ts, activity/deetsmusic-ui/surface.ts, activity/deetsmusic-ui/ui.css, activity/deetsmusic-ui/album-color.ts, activity/deetsmusic-ui/hint.ts, activity/deetsmusic-ui/row-drag.ts, activity/deetsmusic-ui/toast.ts, activity/deetsmusic-ui/scenery.ts, activity/relay/src/index.ts, activity/relay/src/room.ts, activity/relay/src/wire.ts, activity/discord-shell/main.ts, activity/discord-shell/relay-transport.ts, crates/logic/src/deetsmusic_remote.rs, crates/relay-client/src/lib.rs, crates/logic/src/relay.rs, activity/scripts/look-sync.mjs, activity/mock/transport.ts, activity/deetsmusic-ui/full-player.ts, activity/deetsmusic-ui/grow.ts, activity/deetsmusic-ui/vinyl.ts, activity/deetsmusic-ui/ocean.ts, activity/deetsmusic-ui/scene.ts, activity/deetsmusic-ui/pet.ts, activity/deetsmusic-ui/metal.ts, activity/deetsmusic-ui/wallpaper.ts, activity/deetsmusic-ui/album-light.ts, activity/deetsmusic-ui/skin-settings.ts, crates/bridge/src/lib.rs]
updated: 2026-11-11
---
# The Activity: DeetsMusic's look inside the call

> **Status:** project · 2026-11-11. Build steps 1 to 5 of §10 are built (§13): the bridge
> package's new name, the UI on fake data (2, 2b), the relay and the sign-in (3, deployed),
> and the real cards in the call (4 and 5, §13.10: live since 2026-11-11 13:53).
> His desk tests: §12.2 (part run), §12.3 (passed 2026-11-11), §12.4 (open). §14: the full
> player, the card grow, the Queue/History swap and every skin's scenery, built 2026-11-11 and
> NOT released (no release build, no deploy, his word).
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
| play now | `POST /play` with `keepQueue` | `roomGuests.skip` and `roomGuests.add` (his call 3B, PERMISSIONS.md §6) |
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

**Run so far** (2026-11-11): steps 1, 2, 3 and 5 passed ("showed as expected", after the fix
in §13.9). Steps 4 and 6 to 11 are open.

### 12.3 The cards in the call (build steps 4 and 5): passed 2026-11-11
His word: "It passes".
`LD` is the bot's log (`logs\lildeets-<date>.log` when started from the shortcut).

**Setup, once.**
1. **His, in the Developer Portal** (Activities › URL Mappings). Apple's covers come from five
   hosts, and the frame loads nothing from another site. Add five rows; keep the `/` row:

   | Prefix | Target |
   |---|---|
   | `/mz/is1` | `is1-ssl.mzstatic.com` |
   | `/mz/is2` | `is2-ssl.mzstatic.com` |
   | `/mz/is3` | `is3-ssl.mzstatic.com` |
   | `/mz/is4` | `is4-ssl.mzstatic.com` |
   | `/mz/is5` | `is5-ssl.mzstatic.com` |

   The root mapping (`/`) is its own section in the Portal, so the order does not matter.
   **Done** 2026-11-11 (Claude in Chrome, at his word).
2. **Done** 2026-11-11, at his word: deploy the relay (`cf deploy`, WIRE 2) and make the release build.
   The IPC version is 3, so the old host refuses the new logic; the new pair goes live at the
   host's restart (the next song boundary, or at once when nothing plays). `LD`: `remote:
   armed; it reads DeetsMusic while someone has the Activity open`.

**The test.**
1. `/vc play` DeetsMusic in a voice channel. Open the Activity. The cards show the song, Up
   Next and History in DeetsMusic's current theme and skin, with the covers. `LD`: `remote:
   someone has the Activity open; reading DeetsMusic every 1 s`.
2. In DeetsMusic, press pause. The Activity shows it paused within about 1 s. Skip in
   DeetsMusic: the Activity follows within about 1 s.
3. In the Activity: ⏯, ⏮, ⏭. Drag the scrub bar: DeetsMusic jumps there.
4. Search for a song and tap it: it goes to the end of Up Next, with a toast. As the host, the
   toast's **Play Now** plays it.
5. In the Queue: right-click a row › Play Next, then › Remove. Drag a row to a new place.
   DeetsMusic's Up Next follows each one.
6. Change the theme and the skin in DeetsMusic. The Activity follows within 60 s.
7. Your Discord profile shows the song while the Activity is open. Pause: it shows "Paused".
8. Make the Activity small (picture-in-picture: go to a text channel). The tile card shows
   the cover, the title, the artist and the album.
9. **A guest** (a friend in the call, no team seat): they see the same song. With "Guests may
   skip" and "Guests may add songs" on, their toast has **Play Now**: the song plays, and Up
   Next keeps every other song. In DeetsMusic › Settings › Rooms, turn "Guests may skip" off.
   Within about 10 s their ⏭ greys, its hint gives the reason, and Play Now is gone. Turn it on
   again.
10. **`/dm play` for a guest** follows both rules (PERMISSIONS.md §6) and keeps Up Next.
    Your own `/dm play` still replaces Up Next, as before.
10a. **Two at once:** you and the guest each open the Queue. You remove row 1 in the Activity;
    straight after, the guest removes the same row from their stale list. Their remove takes
    out the song they saw, or says "Up Next changed a moment ago…". Never another song.
11. Run `/dm np`. The card has **Open in call**. Press it while in the voice channel: the
    Activity opens. Press it while in no voice channel: write down what Discord does.
12. Close the Activity. `LD`: `remote: nobody has the Activity open; reading stopped`.
13. Close DeetsMusic with the Activity open: the title reads "The host is not playing here".
    Open DeetsMusic again and play: the Activity follows.
14. Restart the host during a call (LIVE-RELOAD.md §7). The page shows "The host is not
    playing here", then the song again when the bot is back.

### 12.4 The full player, the grow, the swap and the scenery (§14): open
**Part A: the mock page, now** (nothing live changes). In `activity/`: `npm run mock`, open the
address it prints, add `?host` to it. The ⚙ at the bottom left is the desk (theme, skin, Sky,
Album light, Tiles).
1. A wide browser window (Max). Tap the cover: the full player opens; the Queue fills the left
   column; History slides in beside the cover. The record keeps turning. Press Escape, then
   open it again and close it with the square at the cover's corner, then with the strip under
   the title bar.
2. Hover the Queue's title: a Grow button shows beside it. Press it: the Queue grows up over Now
   Playing. Press it again: it goes back. Do the same with the strip between the Queue and Now
   Playing, and with the History card (it widens over Search).
3. Make the window shorter until the Queue shows 5 songs or fewer. Its title now opens a menu:
   pick History. History goes under Now Playing; the Queue takes the tall right column. Pick
   Queue from either title to swap back.
4. A window about 700 px wide (Midi): tap the cover. The player covers both cards, the
   controls on the dark band over the cover. A phone-sized window (Mini): unchanged.
5. Skins, from the desk: Ocean (a sea with neon crests; tap ⏭: a ripple where you tapped),
   Happy (sky, sun, clouds, grass, Deets and Happy; ⏭: Happy pushes the cover out; wait about
   40 s for a round of fetch and "Good boy!" or "Gundu..."), Metal (brushed cards, the covers
   before and after beside the cover; ⏭ turns the drum), Glass (the cover as the wallpaper;
   Tiles "some" adds the next albums), Cyber and Press (⏭: the album's colour turns in around
   each card).
6. Sky: set Sunset, then Night: the sky and the moon follow at once.

**Part B: in the call, after a release build and a relay deploy** (his word first; a release
build is a live change, CLAUDE.md › Never).
7. In DeetsMusic, change Skin settings › Album light and Sky. Within 60 s the Activity follows.
8. On a phone: tap the cover in landscape (Wide) and in a tablet-sized frame. The Grow button
   shows without hover.
9. Picture-in-picture while the full player is open: the tile shows; back in the call the
   cards are at rest.
10. **After a DeetsMusic release with `look`** (§14.3): set Settings › Look schedule to Set
    times with the night starting a few minutes from now, a night look different from your own
    pick. When DeetsMusic changes, the Activity follows within 60 s. A friend in another time
    zone sees your sky under Happy.

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
1. The page reads the app's id from its own address (`<app id>.discordsays.com`) and starts
   Discord's SDK. (Before §13.9 it asked the relay, `GET /api/config`.)
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

### 13.9 The first launch: "Not from the Activity" (2026-11-11)
**Seen.** His first launch in Discord showed **Sign-in did not work** / **Not from the
Activity.** The bot was running and in no call (its log: `relay: armed`, `commands: synced
launch=true`).

**Cause.** The page's first call was `GET /.proxy/api/config`. It is same-origin (the page and
the proxy share `<app id>.discordsays.com`), and a browser sends no `Origin` header on a
same-origin GET. The relay refuses an `/api/*` call with no origin (§6, "The origin"), so the
sign-in stopped before it began. The bot had no part in it.

**Fix (one option left, so no fork).** The page reads the app id from its own address, which
Discord sets, and `/api/config` is removed. The origin check is unchanged and stays strict: each
`/api/*` call left is a POST or a WebSocket, and a browser always sends `Origin` on those. The
other option, accepting a GET with no origin, weakens the check for no gain.

**Checked (Claude).** `npm test` in `activity/relay`: 13 steps pass (the origin test now uses
`POST /api/token`; a step checks the old route answers 404). `npm run check` and
`cf deploy --dry-run` pass. **Deployed** (2026-11-11, `cf deploy`, his yes). Checked live: the
page's script reads the address and has no `/api/config`; `/api/config` answers 404; a made-up
code on `POST /api/token` with the Activity's origin reaches Discord and is refused (401), so
the origin check and the secrets work. NOT checked: inside Discord.
**Passed** in Discord (his word, 2026-11-11: "Yup, showed as expected").

### 13.10 Steps 4 and 5 as built: the real cards in the call (2026-11-11)
> **Part:** built and live · 2026-11-11. His word: "Let's build out the UI", then "1A, 2A,
> 3B", then "go ahead and deploy + release". The five URL mappings were added in the Portal
> by Claude in Chrome at his word; the relay was deployed; the release build went live at
> 13:53 (the host restarted with no call). Desk test §12.3.

**His calls.**
| # | Question | His call |
|---|---|---|
| 1 | How fast the Activity follows DeetsMusic | **A**: the bridge is read every 1 s while someone has the Activity open, and not at all when nobody does. Messages leave the PC on a change only. |
| 2 | A button that opens the Activity on the `/dm np` card | **A**: "Open in call", the fifth button. |
| 3 | May a guest use Play Now? | **B**: yes, under "Guests may skip". It also opens `/dm play` to guests under that rule (PERMISSIONS.md §6), so neither surface has an action the other lacks. The review added "Guests may add songs" and keeping Up Next, so a guest's Play Now never passes a rule that is off. |

**Built.**
| Part | Files |
|---|---|
| The relay, WIRE 2: `data` both ways, `state` (kept in the room's storage for a newcomer), `to`, `joined`, `left`, 30 requests per 10 s per page | `activity/relay/src/wire.ts`, `src/room.ts`, `test/relay.test.mjs` (17 steps) |
| The bot's connection: the new frames, and a `Reset` on each connect and drop | `crates/relay-client` |
| Host ⇄ logic, `PROTOCOL` 3: `RelayViewers` (the host keeps the list and gives it to each new logic), `RelayData`, `RelayState`, `RelayTo` | `crates/ipc`, `crates/host/src/main.rs` |
| State out, actions in, search | `crates/logic/src/deetsmusic_remote.rs` |
| One check for `/dm`, the cards and the Activity; `seek`, `search`, `play` under its rule | `crates/logic/src/commands.rs` (`check_for`, `rule`) |
| The theme and the skin from the bridge (cached 60 s with the accent) | `crates/bridge` (`appearance`) |
| "Open in call" (`dm:np:open`, Discord's response type 12) | `crates/cards/src/cards.rs` (`np_card`'s `open`), `crates/logic/src/ix.rs` (`launch_activity`) |
| The page: the cards on the relay's transport, the sign-in card over them, the layout modes (picture-in-picture and grid show the tile card), Rich Presence, the safe areas | `activity/discord-shell/main.ts`, `relay-transport.ts`, `activity/relay/index.html` |
| The message shapes over the relay | `activity/deetsmusic-protocol` (`Request`, `Reply`) |

**Decided inside his calls (for his review).**
1. **Steps 4 and 5 were built together:** step 4 alone shows nothing in Discord.
2. **An action from the Activity posts nothing in the text channel** (his call A at the
   hand-off, 2026-11-11: "A, no line"). A `/dm` action posts a line ("⏭ Skipped — name"); the
   Activity shows the change itself. A refusal is a toast for that person only (§5).
3. **What counts as a change:** the song, play or pause, Up Next, History, the theme, the skin,
   a rule, the bot coming or going, DeetsMusic opening or closing, or a position more than 2 s
   from where the clock puts it (a seek). A song playing on is not a change.
4. **Up Next shows at most 200 songs; History the last 50, newest first.** (The bridge lists
   History oldest first.) Search shows 25 songs.
5. **The progress bar runs on the viewer's own clock**, stamped when a state arrives, so a
   phone with a wrong clock still shows the right time. A person who comes in gets a fresh
   state from the bot within 1 s.
6. **Covers come through Discord's proxy** (`/.proxy/mz/is1/…`), rewritten in the page; the
   profile's Rich Presence uses Apple's own address (Discord fetches that itself).
7. **Rich Presence** is "Listening to" the title, with the artist (or "Paused · artist"), the
   cover and the album, and the song's time while it plays. Set on a change of song or of play
   and pause only.
8. **Who is the host** is sent once to each person, when the bot learns they are in. The relay
   never knows it.
9. **"Open in call"** shows only when `.env` has the relay. Anyone may press it: what they may do
   inside is checked there, per action.
10. **A search refusal** shows in the Search card. A `/dm` line's channel mention reads "Lil
    Deets' voice channel" in the Activity (a mention shows as digits outside a message).

**The Sonnet pass** (2026-11-11, CLAUDE.md › Agents; Claude checked each finding against the
code and DeetsMusic's). Fixed:
1. **A remove or a move hit a row by number only.** Two people at once, or a song ending,
   could remove the wrong song. The page now sends the song it saw in the row. The bot refuses
   a move when that row holds another song ("Up Next changed a moment ago…"); a remove takes
   the nearest row with that song, or is refused.
2. **DeetsMusic's plain `/play` replaces all of Up Next.** A guest's Play Now (and `/dm play`)
   would have wiped the queue without "Guests may change Up Next". Play Now now keeps Up Next
   (`keepQueue`), and needs "Guests may add songs" too (PERMISSIONS.md §6).
3. **The page never offered a guest Play Now** (its rule was "host only"). It now follows
   both rules, in the toast and in a search row's menu.
4. **The toast's Play Now** took out its copy by a row number the page saw. It is now one
   action: the bot takes the last copy out of its own Up Next, then plays.
5. **With DeetsMusic closed**, the old Up Next and History showed. They are cleared.
6. **A search while the bot is away** said "/dm works in a server." It says "The host is not
   playing here".

Not changed, and why:
- A rule turned off in DeetsMusic still lets a press through for up to 10 s: the rules cache
  is the `/dm` one (BRIDGE.md §1); the buttons grey on the same clock.
- An action that waited 15 s says "The host did not answer"; it may have run. The relay drops a
  page's requests above 30 in 10 s; a person pressing buttons stays far below that.
- The progress bar runs a network delay (a few hundred ms) behind.
- A seek sent as the song changes lands at the same fraction of the new song.

**Checked (Claude, 2026-11-11).** `cargo clippy --workspace --all-targets` (no warnings) and
`cargo test --workspace` pass: 4 new tests in `deetsmusic_remote`, new frames in
`lildeets-relay-client`, the "Open in call" card. `npm run check` (types, the look) passes.
`activity/relay`: `npm test` against `cf dev`, 17 steps (data reaches the bot only while it is
in the call; a reply reaches that page only; the state reaches each page in the call and a
newcomer gets the last one; a flood is cut off). `cf deploy --dry-run` builds the page with
DeetsMusic's fonts and look. NOT checked: anything inside Discord or against the real
DeetsMusic (the 1 s reads, the seek fraction, the covers through the URL mappings, the layout
mode events, Rich Presence, response type 12).

## 14. The full player and all the scenery (design, 2026-11-11)

> **Part:** designed · 2026-11-11. Nothing is built. His ask: "can we allow expanding the
> player card if the view meets a size minimum? ... the whole shebang with Happy, Deets,
> sunset/sunrise and neon oceans, etc." The forks in §14.5 are his.

### 14.1 The full player
DeetsMusic's full player (`../DeetsMusic/docs/features/fullscreen-player.md`, shipped 0.26.1):
the Now Playing card grows over the cards beside it, so the cover is as large as the frame
allows. Its two rules hold: the cover is never cut, and no control leaves the screen.

| Frame | What the full player does | Precedent |
|---|---|---|
| Max (above 820 × 745) | Grows over Search and History. The Queue keeps the stage column. The cover on the left; the rows beside it when the card is about 356 px wider than tall, else over its bottom on the dark band. | fullscreen-player.md §6.2, §7.3 |
| Midi (461 to 820 wide) | Grows over the two slots. The rows over the cover. | §6.1 |
| Mini, Wide | No change: Mini's player view (a tap on the cover) and Wide's left half already are the player. | §7.5 |
| The tile | Never. | §13.5 call 3 |

- **The size minimum is Midi's floor**, the app's own rule: a frame wider than 460 px.
- **Each person's own frame.** Opening it changes nothing for the others in the call.
- **Open** (his call 2A): an edge zone (left and right on Max, top and bottom on Midi), or
  a tap on the cover.
- **Close:** the square over the cover's top-right corner (above it when there is room,
  §17), an edge zone, Escape, or a click outside. Never kept after the frame closes (§6.6).
- **The motion** is the app's: the clip-open from the card, the Press record's turning
  flight, Metal's drum (`full-player.ts`, `full-player-layout.ts`: 1,651 lines, ported).
- **The cover file** grows with the box (`fullArtPx`, 200 px steps, at most 2000 px). Apple's
  image server takes the size in the URL; the URL mapping already covers it.

### 14.2 The scenery, skin by skin
Today the page has Glass's aurora and Cyber's storm (§13.7). The rest:

| Skin | What the call sees | The app's driver |
|---|---|---|
| Press | The record turns while the song plays and slides out on a song change | `vinyl.ts` (701 lines) |
| Ocean | The sea, the album's glow from the deep, the **neon crests** in the album's colours (the host's *Album light*), ripples where you tap. No heave with the music (his call 1B) | `ocean.ts`, its worker, `ocean-texture.ts`, `album-light.ts` |
| Glass | The wallpaper of covers from Up Next and History | `wallpaper.ts`, its worker |
| Metal | The material, and the previous and next covers behind the cover | `metal.ts`, `metal-material.ts` |
| Happy | The sky, the sun or the moon, the clouds, the grass; Deets and Happy play fetch on the grass ("Good boy!", "Gundu..."); Happy pushes the old cover out on a song change; the tennis-ball playhead; Baloo 2, Quicksand, Pixelify Sans | `scene.ts`, `pet.ts`, `sky-phase.ts`, the sun equation in `look-schedule.ts` |

- **The host's skin settings ride in the state message** (F3: the same for everyone): Album
  light, card opacity, Sky, Metal's rows. Read from `GET /settings` with the theme, on the
  same 60 s cache.
- **The sky follows the host's sunrise and sunset** (F3): the page runs the app's `skyAt` on
  the host's two times for today (`look.sun`, §14.3). With a DeetsMusic that does not send them
  yet, each viewer's sky follows their own time zone.
- **Happy's fetch and pushes are drawn in each frame**, at random, as in the app: two friends
  may see different rounds. Nothing about them crosses the relay.
- **Glass's wallpaper uses covers only.** The host's own pictures never leave the PC.
- **The rules that stop motion stay:** reduced motion, a hidden frame (`visibilitychange`),
  and the app's per-skin gates. Each driver logs when it arms, runs and stops (console).

### 14.3 A fault found: the call never sees the look schedule
The bridge's `theme` and `skin` rows are the host's **own pick** (`ownSetting` in DeetsMusic's
`agent-settings.ts`), not the look on screen. With Settings › Look schedule on (for example a
day look at sunrise and Black & Red × Ocean at sunset), or a rule that sets the look, the call
keeps showing the own pick. F3 says "the host's current theme and skin".

The fix is in DeetsMusic: its bridge reports the look that shows, and today's sunrise and
sunset for the Sky. The bot cannot work it out from the rows: it cannot see a rule's look or
how long a manual pick lasts. **A change in `../DeetsMusic`, in its own session** (never from
this repo, CLAUDE.md › Never). Until it lands, the Activity shows the own pick and Happy's sky
follows the viewer's own clock.

**Fixed on both sides (2026-11-11).** DeetsMusic's session added `look` to every `GET /settings`
reply: `{theme, skin, sun}`, the ids on screen now and today's sun (`{rise, set}` in epoch ms, or
"day" / "night"; without Sun shift). The `theme` and `skin` rows still mean the own pick. The
bot's `appearance()` now takes `look.theme` and `look.skin` when they are there, else the rows (an
older DeetsMusic), and passes `look.sun` on as the state's `look.sun` after a shape check (two
finite numbers, rise before set, or the one word). The cards' accent follows the shown theme too.
Live only after **both** a DeetsMusic release and a bot release build; until then, the old
behaviour. Test: `appearance_prefers_the_look_on_screen_and_its_sun` (crates/bridge).

### 14.4 The build
One feature: the full player and the scenery. After the §12.3 desk test passes (CLAUDE.md ›
One load-bearing feature in flight). The order:
1. `npm run look:sync` (`qcard.css` changed in DeetsMusic since the copy).
2. The full player on Midi and Max.
3. The Press record, then Ocean (with its neon), Happy, Glass's wallpaper, Metal.
4. The state message's new fields: the skin settings, the showing look and the sun's times
   once DeetsMusic sends them.
5. The Sonnet pass, then his desk test (written into §12 at hand-over).

No new dependency: the two workers are Vite's own `new Worker(new URL(...))`.

### 14.5 Forks — his calls (2026-11-11: "1B, 2A")
**1B:** no heave; the bot sends no level. **2A:** the edge zones and a tap on the cover.

1. **Ocean's heave.** In the app the sea rises and falls with the music's loudness. The page
   cannot hear the call. **A (recommended):** the host measures the level of the sound it
   already sends to the call and sends it to the relay 3 times a second while Ocean shows and
   the music plays. Cost: about 10,800 small messages an hour to the relay (about 540 billed
   Durable Object requests an hour at Cloudflare's 20:1 rate for socket messages; an estimate),
   and the relay stays awake while Ocean plays. **B:** no heave; the sea rolls, glows and
   ripples, but does not move with the music.
2. **How the full player opens.** **A (recommended):** the app's edge zones (left and right
   on Max, top and bottom on Midi) and a tap on the cover, as Mini's cover tap does now. A
   finger finds the cover more easily than a thin edge. **B:** the edge zones only, one to one
   with the app.

### 14.6 His two asks at the build (2026-11-11)
> "Can we also allow the queue to be swapped with history if we note the users display doesn't
> display more than 5 songs of the queue? Can we also allow the queue to be expanded like in DM?"

- **The swap (Max).** The page counts the Up Next rows the Queue under Now Playing can show
  without a scroll (`rowsShown`, queue-card.ts; a hidden row stands in when Up Next is empty).
  At 5 or fewer, the Queue's title becomes a picker (DeetsMusic's title picker) with Queue and
  History: a pick swaps them. The Queue takes History's tall column on the right, History goes
  under Now Playing. Swapped, both titles keep the picker, so the way back is the same.
  Midi and Mini already pick their cards by title. Kept for the visit, as the picks.
- **Expand (DeetsMusic's card grow, CARD-GROW.md).** Max: the Queue grows up over Now
  Playing; Search and History widen over each other. Midi: the two cards widen over each other.
  After a swap the Queue widens from the right column. Ways in: the strip in the gap beside the
  card (its bar shows on hover) and the Grow button beside the card's title (on hover, as the
  app; always on a touchscreen). Ways out: the same button, the grown card's strip, Escape, a
  request for the covered card, the full player, a surface change.

### 14.7 As built (2026-11-11, not released)
> **Part:** built · 2026-11-11. His word: "don't release / deploy / impact the live stream but go
> ahead and build." No release build and no deploy: the running bot and the deployed page are
> unchanged. His desk test is §12.4.

| Part | Files |
|---|---|
| Full player | `full-player.ts` (DeetsMusic's `full-player.ts` and `full-player-layout.ts`), `art-size.ts`, `now-playing-card.ts` (the cover tap, the close square, the larger file), `ui.css` |
| Card grow | `grow.ts`, `ui.css` (the button shows on hover or on a touchscreen) |
| The swap | `layout.ts` (`maxSwap`, `paintSwap`, `measureQueue`), `queue-card.ts` (`rowsShown`) |
| The record, Metal's drum, Happy's push | `vinyl.ts` (DeetsMusic's `vinyl.ts`) |
| Ocean | `ocean.ts`; `ocean-texture.ts` and `ocean-worker.ts` copied as they are |
| Happy | `scene.ts`, `pet.ts` (copied, imports adapted); `look/src/sky-phase.ts` and `look/src/sun-zones.ts` (copied by `look:sync`) |
| Metal | `metal.ts`, `metal-material.ts` (copied, imports adapted) |
| Glass | `wallpaper.ts`; `wallpaper-worker.ts` copied as it is |
| Album light (Press, Cyber edges; Cyber's bolts) | `album-light.ts`, `scenery.ts` (`stormTakeAlbum`) |
| The host's skin settings | `skin-settings.ts`; `SkinLook` in `deetsmusic-protocol`; `SKIN_LOOK_KEYS` in `crates/bridge`; `look` in `crates/logic/src/deetsmusic_remote.rs` |
| Shared | `motion.ts` (reduced motion, tokens, row entry), `player-bus.ts` (the song for the drivers; a play intent; a dropped row) |

**Decided inside his calls (for his review).**
1. **The full player's side column** (Max, a wide card) shows History, or the Queue after a
   swap. The app's picker on that column and its Queue-side setting are not here: the
   Activity has no settings.
2. **The cover file grows** with the full player (200 px steps, at most 2000 px), through the
   same Apple URL mapping.
3. **A host on Glass › Canvas › Picture shows Covers in the call.** The host's own picture
   never leaves the PC.
4. **Happy's sky follows the host's sun** (`look.sun`, §14.3), and the viewer's own time
   zone when the host's DeetsMusic does not send it.
5. **The pair always rest in the bottom-left corner.** The app moves them for its grand
   opening card, which the Activity does not have.
6. **Only the listed skin rows travel** (`SKIN_LOOK_KEYS`, 29 keys). A null value stays home.
7. **No heave on Ocean** (his call 1B). Ripples answer this person's own play, next, back and
   Play Now taps, and a dropped Queue row.
8. **Reduced motion** is the device's own setting in a frame (the app reads its Motion row).
   The host's Animate backgrounds row also holds the scenery still.

**Checked (Claude, Chrome, mock page, 2026-11-11).** Max, Lilac × Press, as the host: a tap on
the cover opened the full player (Queue in the stage column, History beside the cover, the
record turning); Escape closed it. The Queue's Grow button grew it over Now Playing; the
button collapsed it. In a 1000 × 760 frame the Queue's title offered the swap; a pick put
History under Now Playing and the Queue on the right. Ocean × Black & Red: the sea and the neon
crests in the album's colour. Happy × Lilac: sky, sun, clouds, grass, Deets and Happy; ⏭ made
Happy push the cover out. Metal × Sepia: the brushed material and the side covers. Glass ×
Moonlight: the cover wallpaper under the frost. Cyber: the album light's wave on ⏭. Midi
(700 × 700): the full player with the rows on the band. Mini (400 × 700) and Wide (700 × 380):
unchanged. `npm run check`, `npm run mock:build` (both workers bundled), `cargo clippy
--workspace --all-targets` and `cargo test --workspace` (a new bridge test: only the listed
keys leave) pass. NOT checked: anything inside Discord, a real touchscreen, the real bot's
state with `look`, Happy's fetch round (it starts 10 s in; not watched to the end).

**The Sonnet pass** (2026-11-11; Claude checked each finding against the code). Fixed:
1. **Reduced motion on Max: the side column never came in.** The snap open returned before the
   step that brings it. Both opens now end in `afterOpen`.
2. **Midi: Search or Queue on the full player did nothing** when that card was not in a slot
   (it was placed under the player). A card request now closes the player unless the card is
   the Max stage card beside it.
3. **A grow started while the player waited for its large cover** outlived the player and
   left two cards on top of each other. Nothing grows while the player opens.
4. **Ocean with its painter blocked** (a frame's content policy) waited for ever. It now keeps
   the plain sea and logs it.
5. **The tile ran all the scenery behind it.** The layers leave under the tile, Happy plays no
   round, the wallpaper draws nothing (it catches up after), and Metal's side covers go. The
   card's own glow keeps moving.
6. **Happy could be frozen into the full player's still copy.** The copy leaves him out.
7. **Only plain values travel** in `look`: a toggle, a number or a choice of 32 characters at
   most (`SKIN_LOOK_KEYS`, with a test).
Not changed, and why:
- **Happy's sky can differ between two friends** (§14.3): fixed later the same day, once
  DeetsMusic sends the host's sun.
- **A tap during the large cover's wait does nothing more** (up to 0.25 s, the app's own wait).
- **A keyboard that shortens a tablet frame under 500 px makes it Wide** and remounts Search:
  the Wide rule from §13.7, older than this work. Worth his look at the desk test.
- **The host's Animate backgrounds Off holds every guest's scenery still** (F3: the host's
  look for everyone). A guest's own reduced motion still holds theirs.
- Inside Discord, untested: the two workers and the wallpaper's `blob:` picture under the
  frame's content policy (§12.4 part B).
