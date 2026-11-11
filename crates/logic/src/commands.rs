//! Every command, button and autocomplete: the port of `docs/archive/bot/app.py` (`/links`),
//! `docs/archive/bot/voice.py` (`/vc`) and `docs/archive/bot/dm.py` (`/dm`). Replies, their wording and who sees them
//! follow the Python bot. Differences, on purpose:
//! - `/vc tone` says "Test tone for 5 s." without discord.py's transport note.
//! - `/vc play` needs no "deets-tap isn't built" check: the capture is linked in.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use deetsmusic_bridge::BridgeError;
use lildeets_cards::cards;
use lildeets_cards::model::Track;
use lildeets_cards::text::{self, clip, confident, line, notices, suggestion, Placement};
use lildeets_ipc::{Sound, ToHost};
use lildeets_logic::links;
use lildeets_rest::{routes, File, RestError};
use serde_json::{Map, Value};
use tracing::{info, warn};

use crate::ix::{focused, opt, Answer, Ix, AUTOCOMPLETE, COMMAND, COMPONENT};
use crate::{root, Ctx, APP};

/// The command list, exactly as the Python bot registers it (the parity kit).
const COMMANDS: &str = include_str!("../../../tests/parity/expected/commands.json");

/// Registers the command list when it changed since the last sync (a hash in
/// `.lildeets-commands.sync`), so a logic rebuild does not re-sync every time. The host
/// deletes that record at each start, so a host start always syncs once (the Python bot,
/// on the same token, may have synced its own list in between).
pub async fn sync_commands(ctx: &Ctx, app_id: u64) {
    let hash = format!("{app_id}:{:016x}", fnv(COMMANDS.as_bytes()));
    let file = root().join(".lildeets-commands.sync");
    if std::fs::read_to_string(&file).is_ok_and(|h| h.trim() == hash) {
        info!("commands: unchanged since the last sync");
        return;
    }
    let body: Value = serde_json::from_str(COMMANDS).expect("commands.json is JSON");
    match ctx.rest.send(&routes::put_global_commands(app_id, body)).await {
        Ok(_) => {
            info!("commands: synced");
            let _ = std::fs::write(&file, hash);
        }
        Err(e) => warn!(error = %e, "commands: sync failed"),
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3))
}

pub async fn handle(ctx: Arc<Ctx>, event: Value) {
    let Some(ix) = Ix::parse(event) else {
        warn!("interaction: unreadable");
        return;
    };
    let a = Answer::new(&ctx.rest, &ix);
    match ix.kind {
        COMMAND => {
            let (name, sub, opts) = ix.command();
            info!(command = %name, sub = %sub, user = ix.user, "interaction: command");
            match name.as_str() {
                "links" => links_cmd(&ctx, &ix, &a, &opts).await,
                "vc" => vc(&ctx, &ix, &a, &sub, &opts).await,
                "dm" => dm(&ctx, &ix, &a, &sub, &opts).await,
                // (`dm` takes the Arc: `/dm np` hands it to the card's 3-hour watcher.)
                _ => {
                    let _ = a.text("That command is not known to this version of the bot.", true).await;
                }
            }
        }
        AUTOCOMPLETE => {
            let (name, sub, opts) = ix.command();
            let current = focused(&opts);
            let choices = match (name.as_str(), sub.as_str()) {
                ("vc", "play") => app_choices(&current).await,
                ("dm", "add") | ("dm", "play") => song_choices(&ctx, &ix, &current).await,
                _ => Vec::new(),
            };
            let _ = a.choices(choices).await;
        }
        COMPONENT => component(&ctx, &ix, &a).await,
        _ => {}
    }
}

// --- permissions (docs/archive/bot/dm.py RULE, check) --------------------------------------------------

/// action → the Rooms rule that lets a guest do it ("read": any guest in the call; None: host only).
fn rule(action: &str) -> Option<&'static str> {
    match action {
        "np" | "queue" => Some("read"),
        "pause" | "resume" => Some("roomGuests.playPause"),
        "skip" | "back" => Some("roomGuests.skip"),
        "add" => Some("roomGuests.add"),
        "remove" | "move" => Some("roomGuests.changeQueue"),
        _ => None,
    }
}

fn rule_label(rule: &str) -> &'static str {
    match rule {
        "roomGuests.playPause" => "Guests may start and stop",
        "roomGuests.skip" => "Guests may skip",
        "roomGuests.add" => "Guests may add songs",
        _ => "Guests may change Up Next",
    }
}

/// `check`: `None` when allowed; otherwise the reason, for an ephemeral reply.
async fn check(ctx: &Ctx, ix: &Ix, action: &str) -> Option<String> {
    let Some(guild) = ix.guild else { return Some("/dm works in a server.".into()) };
    if ctx.is_host(ix.user).await {
        return None;
    }
    let Some(rule) = rule(action) else { return Some("Only the host can do that.".into()) };
    if !ctx.tapping_app(guild) {
        // The owner, 2026-11-10: a drop must not read as a rules problem.
        if ctx.guild_voice(guild).is_some_and(|g| g.rejoining) {
            return Some("Lil Deets lost the call and is getting back in.".into());
        }
        return Some("/dm opens up to everyone while the host is playing DeetsMusic in voice.".into());
    }
    let bot_channel = ctx.guild_voice(guild).and_then(|g| g.bot_channel)?;
    if ctx.where_is(guild, ix.user).await != Some(bot_channel) {
        return Some(format!("Join <#{bot_channel}> to control the music."));
    }
    if rule == "read" {
        return None;
    }
    let rules = ctx.bridge.rules().await;
    if rules.get(rule).and_then(|v| v.as_deref()) != Some("everyone") {
        return Some(format!("The host has set “{}” to host only in DeetsMusic.", rule_label(rule)));
    }
    None
}

// --- /links (docs/archive/bot/app.py) ------------------------------------------------------------------

async fn links_cmd(ctx: &Ctx, ix: &Ix, a: &Answer<'_>, opts: &Map<String, Value>) {
    let Some(channel) = ix.channel else { return };
    if !ctx.allowed_links.is_empty() && !ctx.allowed_links.contains(&channel) {
        let _ = a.text("Not allowed in this channel.", true).await;
        return;
    }
    let _ = a.defer(true).await;
    let limit = opt(opts, "limit").and_then(Value::as_u64).map(|l| l as usize);
    let messages = match ctx.rest.history(channel, limit).await {
        Ok(m) => m,
        Err(e) => {
            let _ = a.follow_text(format!("Couldn't read this channel: {e}"), true).await;
            return;
        }
    };
    let records: Vec<_> = messages.iter().map(|m| links::Message::from_discord(m, ix.guild)).flat_map(|m| links::extract(&m)).collect();
    let label = ix.channel_name.clone().unwrap_or_else(|| channel.to_string());
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let out = links::outputs(&records, &label, (now.as_secs() as i64, now.subsec_micros()));
    // As write_outputs does: the files stay in exports/ too.
    let exports = root().join("exports");
    let _ = std::fs::create_dir_all(&exports);
    let _ = std::fs::write(exports.join(&out.md_name), &out.md);
    let _ = std::fs::write(exports.join(&out.csv_name), &out.csv);
    let summary = links::summary(&records, &label);
    let files = || vec![File { name: out.md_name.clone(), bytes: out.md.clone() }, File { name: out.csv_name.clone(), bytes: out.csv.clone() }];
    // DM first; if DMs are closed, reply with the files visible only to the asker.
    let dm = match ctx.rest.dm_channel(ix.user).await {
        Ok(dm) => ctx.rest.send(&routes::create_message(dm, &routes::MessageBody::text(summary.clone(), false), files())).await,
        Err(e) => Err(e),
    };
    match dm {
        Ok(_) => {
            let _ = a.follow_text(format!("{summary} Sent to your DMs."), true).await;
        }
        Err(RestError::Status { status: 403, .. }) => {
            let _ = a.follow_files(summary, files(), true).await;
        }
        Err(e) => {
            let _ = a.follow_text(format!("{summary} Couldn't send the files: {e}"), true).await;
        }
    }
}

// --- /vc (docs/archive/bot/voice.py) -------------------------------------------------------------------

async fn vc(ctx: &Ctx, ix: &Ix, a: &Answer<'_>, sub: &str, opts: &Map<String, Value>) {
    let Some(guild) = ix.guild else {
        let _ = a.text("Voice works in a server, not in DMs.", true).await;
        return;
    };
    if !ctx.is_host(ix.user).await {
        let _ = a.text("Only the bot's owner can use /vc.", true).await;
        return;
    }
    let long = Duration::from_secs(25);
    match sub {
        "play" => {
            let Some(channel) = ctx.where_is(guild, ix.user).await else {
                let _ = a.text("Join a voice channel first.", true).await;
                return;
            };
            let _ = a.defer(true).await;
            let app = opt(opts, "app").and_then(Value::as_str).unwrap_or("").trim().to_owned();
            let app = if app.to_ascii_lowercase().ends_with(".exe") { app } else { format!("{app}.exe") };
            let exe = app.clone();
            match ctx.host.op(|request_id| ToHost::Play { request_id, guild, channel, sound: Sound::App { exe } }, long).await {
                Some(problem) => {
                    let _ = a.follow_text(problem, true).await;
                }
                None => {
                    let hint = if app.eq_ignore_ascii_case(APP) { " Everyone here can use `/dm` (see `/dm help`)." } else { "" };
                    let _ = a.follow_text(format!("Playing **{app}** in <#{channel}>.{hint}"), true).await;
                }
            }
        }
        "tone" => {
            let bot_channel = ctx.guild_voice(guild).and_then(|g| g.bot_channel);
            let channel = match bot_channel {
                Some(c) => Some(c),
                None => ctx.where_is(guild, ix.user).await,
            };
            let Some(channel) = channel else {
                let _ = a.text("Join a voice channel first.", true).await;
                return;
            };
            let _ = a.defer(true).await;
            match ctx.host.op(|request_id| ToHost::Play { request_id, guild, channel, sound: Sound::Tone }, long).await {
                Some(problem) => drop(a.follow_text(problem, true).await),
                None => drop(a.follow_text("Test tone for 5 s.", true).await),
            }
        }
        "stop" => {
            let r = ctx.host.op(|request_id| ToHost::Stop { request_id, guild }, Duration::from_secs(2)).await;
            let _ = a.text(r.unwrap_or_else(|| "Stopped.".into()), true).await;
        }
        "leave" => {
            let r = ctx.host.op(|request_id| ToHost::Leave { request_id, guild }, Duration::from_secs(2)).await;
            let _ = a.text(r.unwrap_or_else(|| "Left.".into()), true).await;
        }
        _ => {}
    }
}

/// `/vc play`'s suggestions: apps making sound now (`deets-tap --list`), filtered by what is typed.
async fn app_choices(current: &str) -> Vec<(String, String)> {
    let names = tokio::time::timeout(Duration::from_secs(2), tokio::task::spawn_blocking(deets_tap::list_apps)).await;
    let Ok(Ok(Ok(names))) = names else { return Vec::new() };
    let cur = current.to_lowercase();
    names.into_iter().filter(|n| n.to_lowercase().contains(&cur)).take(25).map(|n| (n.clone(), n)).collect()
}

// --- /dm (docs/archive/bot/dm.py) ----------------------------------------------------------------------

/// `run`: gate, defer (bridge calls can take seconds), do the work, show a BridgeError.
async fn run<F, Fut>(ctx: &Ctx, ix: &Ix, a: &Answer<'_>, action: &str, ephemeral: bool, work: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), BridgeError>>,
{
    if let Some(reason) = check(ctx, ix, action).await {
        let _ = a.text(reason, true).await;
        return;
    }
    let _ = a.defer(ephemeral).await;
    if let Err(e) = work().await {
        let _ = a.follow_text(e.0, true).await;
    }
}

fn now() -> f64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// `np_view`.
pub async fn np_card(ctx: &Ctx, guild: Option<u64>) -> Result<Vec<twilight_model::channel::message::component::Component>, BridgeError> {
    let (np, q, accent) = tokio::join!(ctx.bridge.now_playing(), ctx.bridge.queue(), ctx.bridge.accent());
    let (np, q) = (np?, q?);
    let where_ = guild.filter(|g| ctx.tapping_app(*g)).and_then(|g| ctx.guild_voice(g)).and_then(|g| g.bot_channel).map(|c| format!("<#{c}>"));
    Ok(cards::np_card(&np, q.upcoming.first(), where_.as_deref(), accent, now()))
}

async fn queue_card(ctx: &Ctx, page: usize) -> Result<Vec<twilight_model::channel::message::component::Component>, BridgeError> {
    let (q, accent) = tokio::join!(ctx.bridge.queue(), ctx.bridge.accent());
    Ok(cards::queue_card(&q?, page, accent))
}

/// `_added`: the channel line for an add, with where it landed and when it plays.
async fn added(ctx: &Ctx, who: &str, tracks: &[Track], next_up: bool, song_id: &str) -> String {
    let (q, np) = tokio::join!(ctx.bridge.queue(), ctx.bridge.now_playing());
    match (q, np) {
        (Ok(q), Ok(np)) => text::added_line(who, tracks, next_up, song_id, Some(Placement { upcoming: &q.upcoming, now_playing: &np }), now()),
        _ => text::added_line(who, tracks, next_up, song_id, None, now()),
    }
}

fn reply_tracks(reply: &Value) -> Vec<Track> {
    reply.get("tracks").and_then(|t| serde_json::from_value(t.clone()).ok()).unwrap_or_default()
}

/// `resolve`: (song id to use, candidates). A picked suggestion is an id already.
async fn resolve(ctx: &Ctx, song: &str) -> Result<(Option<String>, Vec<Track>), BridgeError> {
    if song.starts_with("song:") {
        return Ok((Some(song.to_owned()), Vec::new()));
    }
    let songs = ctx.bridge.search_songs(song, None).await?;
    if confident(song, &songs) {
        return Ok((songs[0].catalog_id.as_ref().map(|id| format!("song:{id}")), Vec::new()));
    }
    Ok((None, songs.into_iter().take(5).collect()))
}

async fn song_choices(ctx: &Ctx, ix: &Ix, current: &str) -> Vec<(String, String)> {
    if current.trim().chars().count() < 2 || check(ctx, ix, "add").await.is_some() {
        return Vec::new();
    }
    let search = ctx.bridge.search_songs(current, Some(Duration::from_millis(2500)));
    let Ok(Ok(songs)) = tokio::time::timeout(Duration::from_millis(2600), search).await else { return Vec::new() };
    let rows: Vec<Track> = songs.into_iter().take(25).filter(|t| t.catalog_id.is_some()).collect();
    let w = text::column_width(&rows);
    rows.iter().map(|t| (suggestion(t, w), format!("song:{}", t.catalog_id.as_deref().unwrap_or("")))).collect()
}

async fn dm(ctx: &Arc<Ctx>, ix: &Ix, a: &Answer<'_>, sub: &str, opts: &Map<String, Value>) {
    let who = ix.display.clone();
    match sub {
        "help" => {
            let host = ix.guild.is_some() && ctx.is_host(ix.user).await;
            let mut rights = std::collections::HashMap::new();
            for action in text::ACTIONS {
                rights.insert(action, check(ctx, ix, action).await.is_none());
            }
            let session = ix.guild.is_some_and(|g| ctx.tapping_app(g));
            let accent = Some(ctx.bridge.accent().await);
            let help = text::help_text(|act| rights.get(act).copied().unwrap_or(false), host, session);
            let _ = a.card(cards::help_card(help, accent), true).await;
        }
        "np" => {
            run(ctx, ix, a, "np", false, || async {
                let card = np_card(ctx, ix.guild).await?;
                let _ = a.follow_card(card, false).await;
                if let (Some(channel), Some((ch, msg))) = (ix.channel, a.original().await) {
                    crate::watch::watch_np(ctx.clone(), channel, ch, msg, ix.guild);
                }
                Ok(())
            })
            .await
        }
        "pause" | "resume" | "skip" | "back" => {
            let (kind, done) = match sub {
                "pause" => ("pause", "⏸ Paused"),
                "resume" => ("play", "▶ Resumed"),
                "skip" => ("next", "⏭ Skipped"),
                _ => ("previous", "⏮ Back"),
            };
            run(ctx, ix, a, sub, false, || async {
                let reply = ctx.bridge.command(kind, None).await?;
                let _ = a.follow_text(format!("{done} — {who}{}", notices(&reply)), false).await;
                Ok(())
            })
            .await
        }
        "add" => {
            let song = opt(opts, "song").and_then(Value::as_str).unwrap_or("").to_owned();
            let next = opt(opts, "next").and_then(Value::as_bool).unwrap_or(false);
            // Ephemeral thinking: the picker is for the asker only; the "added" line goes to the channel.
            run(ctx, ix, a, "add", true, || async {
                let (song_id, candidates) = resolve(ctx, &song).await?;
                if let Some(song_id) = song_id {
                    let reply = ctx.bridge.enqueue(&song_id, next).await?;
                    let _ = a.delete().await;
                    let line_ = added(ctx, &who, &reply_tracks(&reply), next, &song_id).await;
                    let _ = a.follow_text(format!("{line_}{}", notices(&reply)), false).await;
                } else if !candidates.is_empty() {
                    let accent = ctx.bridge.accent().await;
                    let _ = a.follow_card(cards::picker_card(&song, &candidates, next, accent), true).await;
                } else {
                    let _ = a.follow_text(format!("No songs found for “{}”.", clip(&song, 80)), true).await;
                }
                Ok(())
            })
            .await
        }
        "queue" => {
            let page = opt(opts, "page").and_then(Value::as_u64).unwrap_or(1) as usize;
            run(ctx, ix, a, "queue", false, || async {
                let card = queue_card(ctx, page).await?;
                let _ = a.follow_card(card, false).await;
                Ok(())
            })
            .await
        }
        "remove" | "move" => {
            let n = opt(opts, "n").and_then(Value::as_u64).unwrap_or(0) as usize;
            let to = opt(opts, "to").and_then(Value::as_u64).map(|t| t as usize);
            run(ctx, ix, a, sub, false, || async {
                let q = ctx.bridge.queue().await?;
                let up = &q.upcoming;
                if n > up.len() || to.is_some_and(|t| t > up.len()) {
                    let _ = a.follow_text(format!("Up Next has {} songs.", up.len()), true).await;
                    return Ok(());
                }
                let it = line(&up[n - 1]);
                match to {
                    None => {
                        ctx.bridge.queue_edit("remove", n as i64, None).await?;
                        let _ = a.follow_text(format!("➖ {who} removed **{it}** (row {n})."), false).await;
                    }
                    Some(to) => {
                        ctx.bridge.queue_edit("move", n as i64, Some(to as i64)).await?;
                        let _ = a.follow_text(format!("↕ {who} moved **{it}** to row {to}."), false).await;
                    }
                }
                Ok(())
            })
            .await
        }
        "play" => {
            let song = opt(opts, "song").and_then(Value::as_str).unwrap_or("").to_owned();
            run(ctx, ix, a, "play", false, || async {
                let (song_id, candidates) = resolve(ctx, &song).await?;
                let Some(song_id) = song_id else {
                    let hint = if candidates.is_empty() { "" } else { "; pick one of the suggestions" };
                    let _ = a.follow_text(format!("Not sure which song you mean{hint}."), true).await;
                    return Ok(());
                };
                let reply = ctx.bridge.play(&song_id).await?;
                let what = reply_tracks(&reply).first().map(line).unwrap_or_else(|| "it".into());
                let _ = a.follow_text(format!("▶ {who} played **{what}**.{}", notices(&reply)), false).await;
                Ok(())
            })
            .await
        }
        "volume" => {
            if let Some(reason) = check(ctx, ix, "volume").await {
                let _ = a.text(reason, true).await;
                return;
            }
            let percent = opt(opts, "percent").and_then(Value::as_u64).unwrap_or(100);
            let guild = ix.guild.unwrap_or(0);
            let gain = percent as f32 / 100.0;
            ctx.host.op(|request_id| ToHost::SetGain { request_id, guild, gain }, Duration::from_secs(2)).await;
            let _ = a.text(format!("🔊 Volume {percent}%."), true).await;
        }
        _ => {}
    }
}

// --- buttons ------------------------------------------------------------------------------

async fn component(ctx: &Ctx, ix: &Ix, a: &Answer<'_>) {
    let id = ix.custom_id().to_owned();
    let parts: Vec<&str> = id.split(':').collect();
    info!(custom_id = %id, user = ix.user, "interaction: button");
    match parts.as_slice() {
        ["dm", "np", action] => {
            if let Some(reason) = check(ctx, ix, action).await {
                let _ = a.text(reason, true).await;
                return;
            }
            if *action == "queue" {
                match queue_card(ctx, 1).await {
                    Ok(card) => drop(a.card(card, true).await),
                    Err(e) => drop(a.text(e.0, true).await),
                }
                return;
            }
            let _ = a.defer_update().await;
            let kind = match *action {
                "back" => "previous",
                "pause" => "pause",
                "resume" => "play",
                _ => "next",
            };
            let result = async {
                ctx.bridge.command(kind, None).await?;
                np_card(ctx, ix.guild).await
            }
            .await;
            match result {
                Ok(card) => drop(a.edit_card(card).await),
                Err(e) => drop(a.follow_text(e.0, true).await),
            }
        }
        // `dm:queue:<page>:<prev|next>`; the 3-part form is from cards posted before 2026-11-11.
        ["dm", "queue", page, ..] => {
            if let Some(reason) = check(ctx, ix, "queue").await {
                let _ = a.text(reason, true).await;
                return;
            }
            let page = page.parse().unwrap_or(1);
            match queue_card(ctx, page).await {
                Ok(card) => drop(a.update(card).await),
                Err(e) => drop(a.text(e.0, true).await),
            }
        }
        ["dm", "pick", _n, next, catalog_id] => {
            if let Some(reason) = check(ctx, ix, "add").await {
                let _ = a.text(reason, true).await;
                return;
            }
            let _ = a.defer_update().await;
            let next_up = *next == "1";
            let song_id = format!("song:{catalog_id}");
            let reply = match ctx.bridge.enqueue(&song_id, next_up).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = a.follow_text(e.0, true).await;
                    return;
                }
            };
            let tracks = reply_tracks(&reply);
            let picked = tracks.first().map(line).unwrap_or_else(|| song_id.clone());
            let _ = a.edit_card(cards::line_card(format!("Picked {picked}."))).await;
            let line_ = added(ctx, &ix.display, &tracks, next_up, &song_id).await;
            let _ = a.follow_text(format!("{line_}{}", notices(&reply)), false).await;
        }
        _ => {
            // A button from the Python bot's cards (random custom_ids): it can't be routed.
            let _ = a.text("That button is from an older card. Run the command again.", true).await;
        }
    }
}
