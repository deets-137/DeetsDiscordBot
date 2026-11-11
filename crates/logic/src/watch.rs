//! What acts on its own (P7), each logging when it arms, fires and stops:
//! - [`status_watcher`]: while the bot plays DeetsMusic into a call, the voice channel's status
//!   line ("🎵 Title — Artist", "⏸ …") and the bot's "Listening to" (`bot/dm.py` StatusWatcher).
//! - [`watch_np`]: keeps a `/dm np` card on the current song (`bot/dm.py` refresh).
//!
//! Both poll the local bridge every 5 s (free) and call Discord only on a change: about once
//! a song (CARDS.md §4: never edit on a timer).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use lildeets_cards::text::clip;
use lildeets_ipc::ToHost;
use lildeets_rest::{routes, MessageBody, RestError};
use tracing::{info, warn};

use crate::Ctx;

const EVERY: Duration = Duration::from_secs(5);
const WATCH_FOR: Duration = Duration::from_secs(3 * 3600);
/// Discord's limits: a voice channel status, a presence name.
const STATUS_MAX: usize = 500;
const PRESENCE_MAX: usize = 128;

pub async fn status_watcher(ctx: Arc<Ctx>) {
    info!("status watcher: armed");
    let mut channel_line: HashMap<u64, String> = HashMap::new();
    let mut presence: Option<String> = None;
    let mut warned: HashSet<u64> = HashSet::new();
    loop {
        tokio::time::sleep(EVERY).await;
        let live: Vec<u64> = ctx
            .voice
            .lock()
            .unwrap()
            .iter()
            .filter(|g| g.tapping_app.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(crate::APP)))
            .filter_map(|g| g.bot_channel)
            .collect();
        let mut line = None;
        let mut want_presence = None;
        if !live.is_empty() {
            if let Ok(np) = ctx.bridge.now_playing().await {
                if let Some(title) = np.title.as_deref().filter(|t| np.active && !t.is_empty()) {
                    let song = format!("{title} — {}", np.artist.as_deref().unwrap_or(""));
                    line = Some(format!("{}{song}", if np.playing { "🎵 " } else { "⏸ " }));
                    want_presence = np.playing.then_some(song);
                }
            }
        }
        if want_presence != presence {
            info!(listening = ?want_presence, "status watcher: presence");
            ctx.host.tell(ToHost::Presence { listening: want_presence.as_deref().map(|s| clip(s, PRESENCE_MAX)) }).await;
            presence = want_presence;
        }
        for &cid in &live {
            let want = line.clone().unwrap_or_default();
            // Remembered only once Discord took it: a write lost to a network drop is tried
            // again on the next look (every 5 s), not left stale until the next song.
            if channel_line.get(&cid) != Some(&want) && set_status(&ctx, cid, &want, &mut warned).await {
                channel_line.insert(cid, want);
            }
        }
        let gone: Vec<u64> = channel_line.keys().copied().filter(|c| !live.contains(c)).collect();
        for cid in gone {
            if set_status(&ctx, cid, "", &mut warned).await {
                channel_line.remove(&cid);
            }
        }
    }
}

/// Writes a channel's status. True when it is settled: written, or refused for good (403:
/// no permission, or the bot has left the channel; Discord clears an empty channel's status).
async fn set_status(ctx: &Ctx, channel: u64, line: &str, warned: &mut HashSet<u64>) -> bool {
    let status = clip(line, STATUS_MAX);
    match ctx.rest.send(&routes::put_voice_status(channel, &status)).await {
        Ok(_) => {
            info!(channel, %status, "status watcher: channel status");
            true
        }
        Err(RestError::Status { status: 403, .. }) => {
            if warned.insert(channel) {
                warn!(channel, "status watcher: no Set Voice Channel Status permission there (or not in the channel); skipping its status");
            }
            true
        }
        // Any other refusal (the channel is gone, a bad request) will not change on a retry.
        Err(RestError::Status { status, .. }) if (400..500).contains(&status) && status != 429 => {
            warn!(channel, status, "status watcher: Discord refused the channel status; not retrying");
            true
        }
        Err(e) => {
            warn!(channel, error = %e, "status watcher: channel status not written; trying again in 5 s");
            false
        }
    }
}

/// Starts (or replaces) the np card's watcher for `channel`: the card at (`card_channel`,
/// `message`) is edited when the song or play/pause changes, for 3 hours.
pub fn watch_np(ctx: Arc<Ctx>, channel: u64, card_channel: u64, message: u64, guild: Option<u64>) {
    let c = ctx.clone();
    let task = tokio::spawn(async move {
        let ctx = c;
        info!(channel, message, "np watcher: armed");
        let started = tokio::time::Instant::now();
        let mut last: Option<(String, bool)> = None;
        while started.elapsed() < WATCH_FOR {
            tokio::time::sleep(EVERY).await;
            let Ok(np) = ctx.bridge.now_playing().await else { continue };
            let key = (np.catalog_id.clone().or(np.title.clone()).unwrap_or_default(), np.playing);
            match &last {
                None => last = Some(key),
                Some(k) if *k == key => {}
                Some(_) => {
                    let Ok(card) = crate::commands::np_card(&ctx, guild).await else { continue };
                    match ctx.rest.send(&routes::edit_message(card_channel, message, &MessageBody::card(card, false))).await {
                        Ok(_) => {
                            last = Some(key);
                            info!(channel, message, "np watcher: edited (song or play state changed)");
                        }
                        // The card is gone (deleted) or out of reach: nothing left to edit.
                        Err(RestError::Status { status: 403 | 404, .. }) => {
                            info!(channel, message, "np watcher: the card is gone; stopping");
                            return;
                        }
                        // A network drop or a Discord hiccup: `last` stays, so the next look
                        // (5 s) tries the edit again.
                        Err(e) => warn!(channel, error = %e, "np watcher: the edit failed; trying again in 5 s"),
                    }
                }
            }
        }
        info!(channel, "np watcher: stopped after 3 h");
    });
    if let Some(old) = ctx.np_watchers.lock().unwrap().insert(channel, task) {
        old.abort();
        info!(channel, "np watcher: a newer /dm np took over");
    }
}
