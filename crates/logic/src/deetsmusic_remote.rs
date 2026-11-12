//! `deetsmusic_remote`: DeetsMusic for the Activity (docs/features/ACTIVITY.md §5, build step 4).
//!
//! - **State out.** While at least one person has the Activity open, the state watcher reads
//!   the bridge every 1 s (his call 1A, 2026-11-11): `/now-playing` each time, `/queue` when
//!   the song changes, after an action, and every 5 s. It sends the state to the relay **on a
//!   change only**: the song, play or pause, the queue, the history, the look, the rules, or a
//!   jump in the position (a seek). With nobody in the Activity it reads nothing.
//! - **Actions in.** A request from a page runs the same check as `/dm` (`check_for`), then
//!   one bridge call from BRIDGE.md §2. The answer goes to that person only.
//! - **Who is who.** The host keeps the list of people in the Activity and gives it to each
//!   new logic (`ToLogic::RelayViewers`). This module keeps a copy; it holds no voice state.
//!
//! The shapes are `activity/deetsmusic-protocol/index.ts` (`HostState`, `Action`, `Request`,
//! `Reply`). It logs when it arms, fires and stops (target `lildeets_logic`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use deetsmusic_bridge::{Appearance, BridgeError};
use lildeets_cards::model::{NowPlaying, Queue, Track};
use lildeets_ipc::{RelayViewer, ToHost};
use serde_json::{json, Value};
use tokio::sync::Notify;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::commands::check_for;
use crate::{Ctx, APP};

/// deetsmusic-protocol `PROTOCOL_VERSION`.
const PROTOCOL_VERSION: u32 = 1;
/// How often the bridge is read while someone has the Activity open (his call 1A).
const EVERY: Duration = Duration::from_secs(1);
/// `/queue` is read at least this often (the host may change Up Next in DeetsMusic).
const QUEUE_EVERY: Duration = Duration::from_secs(5);
/// A position this far from where the clock puts it is a seek: send it.
const DRIFT_MS: f64 = 2000.0;
const UP_NEXT_MAX: usize = 200;
const HISTORY_MAX: usize = 50;
const SEARCH_MAX: usize = 25;
const TERM_MAX: usize = 100;

const NOT_HERE: &str = "The host is not playing here";
const REOPEN: &str = "Close the Activity and open it again.";

/// What the remote keeps between reads. Nothing here is voice state.
#[derive(Default)]
pub struct Remote {
    viewers: Mutex<Vec<RelayViewer>>,
    /// Wakes the watcher: a person came in, or an action changed something.
    wake: Notify,
    /// Send the next state even if it looks the same (a person came in after a reconnect).
    force: AtomicBool,
    /// Read `/queue` on the next look (an action may have changed it).
    queue_now: AtomicBool,
    /// The current song's length, for a seek (the protocol sends a position, the bridge wants
    /// a fraction).
    duration_ms: Mutex<Option<f64>>,
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// The host's new list of people in the Activity.
pub async fn viewers(ctx: &Arc<Ctx>, list: Vec<RelayViewer>) {
    let added: Vec<RelayViewer> = {
        let mut held = ctx.remote.viewers.lock().unwrap();
        let added = list.iter().filter(|v| !held.iter().any(|h| h.rid == v.rid)).cloned().collect();
        *held = list;
        added
    };
    // Each newcomer hears whether they are the host: it draws their controls.
    for v in &added {
        let host = ctx.is_host(v.user).await;
        info!(user = v.user, host, channel = v.channel, "remote: a person opened the Activity");
        ctx.host.tell(ToHost::RelayTo { rid: v.rid.clone(), data: json!({"kind": "you", "host": host}) }).await;
    }
    if !added.is_empty() {
        ctx.remote.force.store(true, Ordering::Relaxed);
    }
    ctx.remote.wake.notify_one();
}

/// The state watcher: runs for the logic's life, reads only while someone has the Activity open.
pub async fn watcher(ctx: Arc<Ctx>) {
    info!("remote: armed; it reads DeetsMusic while someone has the Activity open");
    let mut last_key = String::new();
    let mut last_now: Option<(f64, f64, bool)> = None; // (positionMs, at, playing) last sent
    let mut queue: Option<Queue> = None;
    let mut queue_at = Instant::now() - QUEUE_EVERY;
    let mut queue_song = String::new();
    let mut reading = false;
    loop {
        if ctx.remote.viewers.lock().unwrap().is_empty() {
            if reading {
                info!("remote: nobody has the Activity open; reading stopped");
                reading = false;
                last_key.clear();
                last_now = None;
                queue = None;
            }
            ctx.remote.wake.notified().await;
            continue;
        }
        if !reading {
            info!("remote: someone has the Activity open; reading DeetsMusic every 1 s");
            reading = true;
        }

        let np = ctx.bridge.now_playing().await;
        if np.is_err() {
            // DeetsMusic is closed: its old Up Next and History must not show as if real.
            queue = None;
        }
        let song = np.as_ref().ok().and_then(|n| n.catalog_id.clone().or(n.title.clone())).unwrap_or_default();
        let force = ctx.remote.force.swap(false, Ordering::Relaxed);
        if np.is_ok() && (force || ctx.remote.queue_now.swap(false, Ordering::Relaxed) || queue.is_none() || song != queue_song || queue_at.elapsed() >= QUEUE_EVERY) {
            if let Ok(q) = ctx.bridge.queue().await {
                queue = Some(q);
            }
            queue_at = Instant::now();
            queue_song = song;
        }
        let (rules, look) = tokio::join!(ctx.bridge.rules(), ctx.bridge.appearance());
        let active = np.as_ref().is_ok_and(|n| n.active) && ctx.voice.lock().unwrap().iter().any(|g| g.bot_channel.is_some() && !g.rejoining && g.tapping_app.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(APP)));
        let at = now_ms();
        let state = build_state(np.as_ref().ok(), queue.as_ref(), &rules, &look, active, at);
        *ctx.remote.duration_ms.lock().unwrap() = state["now"]["track"]["durationMs"].as_f64();

        // The key leaves out the clock: a song playing on is not a change.
        let mut keyed = state.clone();
        keyed["now"]["positionMs"] = Value::Null;
        keyed["now"]["at"] = Value::Null;
        let key = keyed.to_string();
        let pos = state["now"]["positionMs"].as_f64().unwrap_or(0.0);
        let playing = state["now"]["playing"].as_bool().unwrap_or(false);
        let jumped = last_now.is_some_and(|(p, t, was_playing)| {
            let expected = if was_playing { p + (at - t) } else { p };
            (pos - expected).abs() > DRIFT_MS
        });
        if force || key != last_key || jumped {
            ctx.host.tell(ToHost::RelayState { data: json!({"kind": "state", "state": state}) }).await;
            if key != last_key {
                info!(active, playing, up_next = state["upNext"].as_array().map_or(0, Vec::len), "remote: state sent (a change)");
            }
            last_key = key;
            last_now = Some((pos, at, playing));
        }

        // The next look in 1 s, or at once after an action.
        tokio::select! {
            _ = tokio::time::sleep(EVERY) => {}
            _ = ctx.remote.wake.notified() => {}
        }
    }
}

/// deetsmusic-protocol `Track` from a bridge track.
fn track(t: &Track) -> Value {
    let mut out = json!({
        "catalogId": t.catalog_id.clone().unwrap_or_default(),
        "title": t.title.clone().unwrap_or_default(),
        "artistName": t.artist_name.clone().unwrap_or_default(),
    });
    if let Some(v) = t.album_name.as_ref().filter(|s| !s.is_empty()) {
        out["albumName"] = json!(v);
    }
    if let Some(v) = t.duration_ms {
        out["durationMs"] = json!(v);
    }
    if let Some(v) = t.content_rating.as_deref().filter(|r| matches!(*r, "explicit" | "clean")) {
        out["contentRating"] = json!(v);
    }
    if let Some(v) = t.artwork.as_ref().and_then(|a| a.url_template.as_ref()).filter(|s| !s.is_empty()) {
        out["artworkTemplate"] = json!(v);
    }
    out
}

/// The song now: the queue's `current` when it is the same song (it has the rating and the
/// artwork template), else what `/now-playing` says.
fn now_track(np: &NowPlaying, q: Option<&Queue>) -> Option<Value> {
    let title = np.title.as_deref().filter(|t| np.active && !t.is_empty())?;
    if let Some(cur) = q.and_then(|q| q.current.as_ref()) {
        if cur.catalog_id.is_some() && cur.catalog_id == np.catalog_id {
            return Some(track(cur));
        }
    }
    let mut out = json!({
        "catalogId": np.catalog_id.clone().unwrap_or_default(),
        "title": title,
        "artistName": np.artist.clone().unwrap_or_default(),
    });
    if let Some(v) = np.album.as_ref().filter(|s| !s.is_empty()) {
        out["albumName"] = json!(v);
    }
    if let Some(d) = np.duration.filter(|d| *d > 0.0) {
        out["durationMs"] = json!(d * 1000.0);
    }
    if let Some(v) = [&np.artwork_template, &np.artwork_url].into_iter().flatten().find(|s| !s.is_empty()) {
        out["artworkTemplate"] = json!(v);
    }
    Some(out)
}

/// deetsmusic-protocol `HostState` without `you` (each page has its own).
fn build_state(np: Option<&NowPlaying>, q: Option<&Queue>, rules: &HashMap<String, Option<String>>, look: &Appearance, active: bool, at: f64) -> Value {
    let rule = |key: &str| if rules.get(key).and_then(|v| v.as_deref()) == Some("everyone") { "everyone" } else { "host" };
    let now = match np {
        Some(np) => json!({
            "track": now_track(np, q),
            "playing": np.playing && np.active,
            "positionMs": (np.current_time.unwrap_or(0.0) * 1000.0).round(),
            "at": at.round(),
        }),
        None => json!({"track": null, "playing": false, "positionMs": 0, "at": at.round()}),
    };
    let up_next: Vec<Value> = q.map(|q| q.upcoming.iter().take(UP_NEXT_MAX).map(track).collect()).unwrap_or_default();
    // The bridge lists history oldest first; the cards show it newest first.
    let history: Vec<Value> = q.map(|q| q.history.iter().rev().take(HISTORY_MAX).map(|t| json!({"track": track(t)})).collect()).unwrap_or_default();
    json!({
        "v": PROTOCOL_VERSION,
        "active": active,
        "now": now,
        "upNext": up_next,
        "history": history,
        "theme": if look.theme.is_empty() { "lilac" } else { look.theme.as_str() },
        "skin": if look.skin.is_empty() { "press" } else { look.skin.as_str() },
        // The host's skin settings (ACTIVITY.md §14.2): only the bridge's SKIN_LOOK_KEYS.
        "look": look.skin_look,
        "rules": {
            "playPause": rule("roomGuests.playPause"),
            "skip": rule("roomGuests.skip"),
            "add": rule("roomGuests.add"),
            "changeQueue": rule("roomGuests.changeQueue"),
            "seek": rule("roomGuests.seek"),
        },
    })
}

/// A request from a page: an action or a search. The answer goes back to that page only.
pub async fn request(ctx: &Arc<Ctx>, rid: String, data: Value) {
    let q = data.get("q").and_then(Value::as_u64).unwrap_or(0);
    let viewer = ctx.remote.viewers.lock().unwrap().iter().find(|v| v.rid == rid).cloned();
    let reply = match viewer {
        None => refusal(q, REOPEN),
        Some(v) => {
            if let Some(term) = data.get("search").and_then(Value::as_str) {
                search(ctx, &v, q, term).await
            } else if let Some(act) = data.get("act") {
                act_on(ctx, &v, q, act).await
            } else {
                refusal(q, REOPEN)
            }
        }
    };
    ctx.host.tell(ToHost::RelayTo { rid, data: reply }).await;
}

fn refusal(q: u64, reason: &str) -> Value {
    json!({"kind": "answer", "q": q, "result": {"ok": false, "reason": reason}})
}

/// The server whose call the bot is in, in this person's Activity channel.
fn guild_of(ctx: &Ctx, channel: u64) -> Option<u64> {
    ctx.voice.lock().unwrap().iter().find(|g| g.bot_channel == Some(channel)).map(|g| g.guild)
}

async fn search(ctx: &Ctx, v: &RelayViewer, q: u64, term: &str) -> Value {
    let term: String = term.trim().chars().take(TERM_MAX).collect();
    let found = |tracks: Vec<Value>, error: Option<String>| json!({"kind": "found", "q": q, "tracks": tracks, "error": error});
    if term.is_empty() {
        return found(Vec::new(), None);
    }
    let Some(guild) = guild_of(ctx, v.channel) else { return found(Vec::new(), Some(NOT_HERE.into())) };
    if let Some(reason) = check_for(ctx, Some(guild), v.user, "search").await {
        return found(Vec::new(), Some(plain(&reason)));
    }
    match ctx.bridge.search_songs(&term, None).await {
        Ok(songs) => found(songs.iter().filter(|t| t.catalog_id.is_some()).take(SEARCH_MAX).map(track).collect(), None),
        Err(e) => found(Vec::new(), Some(e.0)),
    }
}

/// An id that may go to the bridge as `song:<id>`: Apple's catalog ids are digits.
fn catalog_id(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')).map(str::to_owned)
}

async fn act_on(ctx: &Ctx, v: &RelayViewer, q: u64, act: &Value) -> Value {
    let kind = act.get("kind").and_then(Value::as_str).unwrap_or("");
    // (the `/dm` action name the check knows, what the bridge does)
    let index = |key: &str| act.get(key).and_then(Value::as_u64).filter(|n| *n < 10_000).map(|n| n as i64 + 1);
    let job: Option<(&str, Job)> = match kind {
        "play" => Some(("resume", Job::Command("play", None))),
        "pause" => Some(("pause", Job::Command("pause", None))),
        "next" => Some(("skip", Job::Command("next", None))),
        "previous" => Some(("back", Job::Command("previous", None))),
        "seek" => {
            let at = act.get("positionMs").and_then(Value::as_f64).filter(|p| p.is_finite() && *p >= 0.0);
            let dur = *ctx.remote.duration_ms.lock().unwrap();
            match (at, dur) {
                (Some(at), Some(d)) if d > 0.0 => Some(("seek", Job::Command("seek", Some(json!((at / d).clamp(0.0, 1.0)))))),
                _ => return refusal(q, "This song cannot be moved to a time."),
            }
        }
        "enqueue" => catalog_id(&act["catalogId"]).map(|id| ("add", Job::Enqueue(id, act.get("mode").and_then(Value::as_str) == Some("next")))),
        "playNow" => catalog_id(&act["catalogId"]).map(|id| ("play", Job::Play(id, act.get("dropCopy").and_then(Value::as_bool) == Some(true)))),
        "queueRemove" => index("index").map(|i| ("remove", Job::Edit("remove", i, None, catalog_id(&act["catalogId"])))),
        "queueMove" => match (index("index"), index("to")) {
            (Some(i), Some(t)) => Some(("move", Job::Edit("move", i, Some(t), catalog_id(&act["catalogId"])))),
            _ => None,
        },
        _ => None,
    };
    let Some((action, job)) = job else { return refusal(q, REOPEN) };
    let Some(guild) = guild_of(ctx, v.channel) else {
        info!(user = v.user, action, "remote: an action from the Activity; the bot is not in that call");
        return refusal(q, NOT_HERE);
    };
    if let Some(reason) = check_for(ctx, Some(guild), v.user, action).await {
        info!(user = v.user, action, "remote: an action from the Activity was refused");
        return refusal(q, &plain(&reason));
    }
    let result: Result<Value, BridgeError> = match job {
        Job::Command(kind, value) => ctx.bridge.command(kind, value).await,
        Job::Enqueue(id, next) => ctx.bridge.enqueue(&format!("song:{id}"), next).await,
        Job::Play(id, drop_copy) => play_now(ctx, &id, drop_copy).await,
        Job::Edit(what, i, to, saw) => match row_now(ctx, what, i, saw.as_deref()).await {
            Ok(Some(i)) => ctx.bridge.queue_edit(what, i, to).await,
            Ok(None) => {
                info!(user = v.user, action, "remote: Up Next changed under the person; refused");
                return refusal(q, QUEUE_CHANGED);
            }
            Err(e) => Err(e),
        },
    };
    // The next look reads everything again, at once.
    ctx.remote.queue_now.store(true, Ordering::Relaxed);
    ctx.remote.wake.notify_one();
    match result {
        Ok(reply) => {
            info!(user = v.user, action, "remote: an action from the Activity");
            let notices: Vec<&str> = reply.get("notices").and_then(Value::as_array).into_iter().flatten().filter_map(|n| n.get("text").and_then(Value::as_str)).collect();
            json!({"kind": "answer", "q": q, "result": {"ok": true, "notices": notices}})
        }
        Err(e) => {
            warn!(user = v.user, action, error = %e, "remote: DeetsMusic refused an action from the Activity");
            refusal(q, &e.0)
        }
    }
}

enum Job {
    Command(&'static str, Option<Value>),
    Enqueue(String, bool),
    /// The song, and whether to take out the copy the person's own add just put in Up Next.
    Play(String, bool),
    /// remove | move, the 1-based row, the row to move to, the song the person saw there.
    Edit(&'static str, i64, Option<i64>, Option<String>),
}

const QUEUE_CHANGED: &str = "Up Next changed a moment ago. Look again, then try once more.";

/// A `/dm` line made fit for the Activity: a channel mention (`<#id>`) shows as raw digits
/// outside Discord's message text.
fn plain(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len());
    let mut rest = reason;
    while let Some(start) = rest.find("<#") {
        let tail = &rest[start + 2..];
        match tail.find('>') {
            Some(end) if end > 0 && tail[..end].bytes().all(|b| b.is_ascii_digit()) => {
                out.push_str(&rest[..start]);
                out.push_str("Lil Deets' voice channel");
                rest = &tail[end + 1..];
            }
            _ => {
                out.push_str(&rest[..start + 2]);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The row to edit now. With the song the person saw (`saw`): that row when it still holds
/// that song; for a remove, the nearest row with that song; else `None` (Up Next changed
/// under them, so nothing is touched). Without `saw` (an older page): the row as sent.
async fn row_now(ctx: &Ctx, what: &str, row: i64, saw: Option<&str>) -> Result<Option<i64>, BridgeError> {
    let Some(saw) = saw else { return Ok(Some(row)) };
    let q = ctx.bridge.queue().await?;
    let id_at = |r: i64| q.upcoming.get((r - 1) as usize).and_then(|t| t.catalog_id.as_deref());
    if id_at(row) == Some(saw) {
        return Ok(Some(row));
    }
    if what != "remove" {
        return Ok(None);
    }
    Ok((1..=q.upcoming.len() as i64).filter(|r| id_at(*r) == Some(saw)).min_by_key(|r| (r - row).abs()))
}

/// Play Now keeps Up Next (DeetsMusic's `keepQueue`): replacing it would need "Guests may
/// change Up Next". With `drop_copy` (the toast's Play Now), the last copy of the song in Up
/// Next goes first: the person's own add put it there a moment ago.
async fn play_now(ctx: &Ctx, id: &str, drop_copy: bool) -> Result<Value, BridgeError> {
    if drop_copy {
        let q = ctx.bridge.queue().await?;
        if let Some(i) = q.upcoming.iter().rposition(|t| t.catalog_id.as_deref() == Some(id)) {
            ctx.bridge.queue_edit("remove", i as i64 + 1, None).await?;
        }
    }
    ctx.bridge.play(&format!("song:{id}"), true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn np() -> NowPlaying {
        serde_json::from_value(json!({"active": true, "playing": true, "title": "Song", "artist": "Artist", "album": "Album", "catalogId": "111", "currentTime": 42.5, "duration": 200.0, "artworkUrl": "https://x/{w}x{h}bb.jpg"})).unwrap()
    }

    fn queue() -> Queue {
        serde_json::from_value(json!({
            "current": {"catalogId": "111", "title": "Song", "artistName": "Artist", "albumName": "Album", "durationMs": 200000.0, "contentRating": "explicit", "artwork": {"urlTemplate": "https://a/{w}x{h}bb.jpg"}},
            "upcoming": [{"catalogId": "222", "title": "Next", "artistName": "B"}],
            "history": [{"catalogId": "1", "title": "Oldest", "artistName": "C"}, {"catalogId": "2", "title": "Newest", "artistName": "D"}]
        }))
        .unwrap()
    }

    #[test]
    fn the_state_has_the_protocol_shape() {
        let rules: HashMap<String, Option<String>> = [("roomGuests.skip".to_owned(), Some("everyone".to_owned())), ("roomGuests.add".to_owned(), Some("host".to_owned()))].into();
        let skin_look = [("oceanLight".to_owned(), json!(60)), ("happySky".to_owned(), json!("sunset"))].into_iter().collect();
        let look = Appearance { theme: "sepia".into(), skin: "glass".into(), skin_look };
        let s = build_state(Some(&np()), Some(&queue()), &rules, &look, true, 1000.0);
        assert_eq!(s["v"], 1);
        assert_eq!(s["active"], true);
        // The queue's current song wins: it has the rating and the template.
        assert_eq!(s["now"]["track"]["contentRating"], "explicit");
        assert_eq!(s["now"]["track"]["artworkTemplate"], "https://a/{w}x{h}bb.jpg");
        assert_eq!(s["now"]["positionMs"], 42500.0);
        assert_eq!(s["now"]["at"], 1000.0);
        assert_eq!(s["upNext"][0]["catalogId"], "222");
        assert!(s["upNext"][0].get("albumName").is_none());
        // History newest first.
        assert_eq!(s["history"][0]["track"]["title"], "Newest");
        assert_eq!(s["theme"], "sepia");
        assert_eq!(s["skin"], "glass");
        assert_eq!(s["look"]["oceanLight"], 60);
        assert_eq!(s["look"]["happySky"], "sunset");
        assert_eq!(s["rules"]["skip"], "everyone");
        assert_eq!(s["rules"]["add"], "host");
        assert_eq!(s["rules"]["seek"], "host"); // unknown: host only
    }

    #[test]
    fn with_no_queue_the_song_comes_from_now_playing() {
        let s = build_state(Some(&np()), None, &HashMap::new(), &Appearance::default(), false, 0.0);
        assert_eq!(s["now"]["track"]["title"], "Song");
        assert_eq!(s["now"]["track"]["durationMs"], 200000.0);
        assert_eq!(s["now"]["track"]["artworkTemplate"], "https://x/{w}x{h}bb.jpg");
        assert_eq!(s["theme"], "lilac");
        assert_eq!(s["upNext"], json!([]));
    }

    #[test]
    fn deetsmusic_closed_is_no_song() {
        let s = build_state(None, None, &HashMap::new(), &Appearance::default(), false, 5.0);
        assert_eq!(s["now"]["track"], Value::Null);
        assert_eq!(s["now"]["playing"], false);
        assert_eq!(s["active"], false);
    }

    #[test]
    fn a_channel_mention_becomes_words() {
        assert_eq!(plain("Join <#1234567890> to control the music."), "Join Lil Deets' voice channel to control the music.");
        assert_eq!(plain("No mention here."), "No mention here.");
        assert_eq!(plain("Odd <#x> and <#12"), "Odd <#x> and <#12");
    }

    #[test]
    fn only_plain_ids_reach_the_bridge() {
        assert_eq!(catalog_id(&json!("1440857781")), Some("1440857781".into()));
        assert_eq!(catalog_id(&json!("i.abc123")), Some("i.abc123".into()));
        assert_eq!(catalog_id(&json!("playlist:1")), None);
        assert_eq!(catalog_id(&json!("1/../x")), None);
        assert_eq!(catalog_id(&json!("")), None);
        assert_eq!(catalog_id(&json!(5)), None);
    }
}
