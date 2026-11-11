//! `lildeets-logic`: every interaction (rust-port-plan.md §3.1, P6–P7). The host starts it
//! with `LILDEETS_IPC` and `LILDEETS_IPC_TOKEN`, and replaces it when it is rebuilt. It calls
//! Discord's REST API itself (`lildeets-rest`) and DeetsMusic's bridge (`deetsmusic-bridge`),
//! and asks the host for anything voice or gateway.
//!
//! The Python bot is the specification: every reply, its wording and who sees it follow
//! `docs/archive/bot/app.py`, `docs/archive/bot/voice.py` and `docs/archive/bot/dm.py` (see `commands.rs`).
//!
//! Threads (rule 10): tokio workers (the process); the blocking pool for `deets_tap::list_apps`
//! (`/vc play`'s autocomplete, < 2 s); nothing else.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use deetsmusic_bridge::Bridge;
use lildeets_ipc::{GuildVoice, ToHost, ToLogic, ENV_ADDR, ENV_TOKEN, PROTOCOL};
use lildeets_rest::Rest;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

mod commands;
mod deetsmusic_remote;
mod ix;
mod relay;
mod watch;

/// The repo root: `.env`, `config.py`, `exports/`.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A key from the environment, else from the repo's `.env`.
pub(crate) fn env_or_dotenv(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        return Some(v);
    }
    let text = std::fs::read_to_string(root().join(".env")).ok()?;
    let prefix = format!("{key}=");
    text.lines()
        .filter_map(|l| l.trim().strip_prefix(prefix.as_str()))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
        .find(|v| !v.is_empty())
}

/// `config.ALLOWED_CHANNEL_IDS` from `config.py` (empty: `/links` works anywhere).
fn allowed_links_channels() -> HashSet<u64> {
    let Ok(text) = std::fs::read_to_string(root().join("config.py")) else { return HashSet::new() };
    let Some(line) = text.lines().find(|l| l.trim_start().starts_with("ALLOWED_CHANNEL_IDS")) else { return HashSet::new() };
    let value = line.split_once('=').map_or("", |(_, v)| v);
    value.split(|c: char| !c.is_ascii_digit()).filter(|s| s.len() >= 15).filter_map(|s| s.parse().ok()).collect()
}

/// Requests to the host, each answered by a `Done`, `MemberVoice` or `VoiceSnapshot` with
/// the same `request_id`.
pub struct HostLink {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<ToLogic>>>,
}

impl HostLink {
    pub async fn tell(&self, msg: ToHost) {
        if let Err(e) = lildeets_ipc::write(&mut *self.writer.lock().await, &msg).await {
            error!(error = %e, "ipc: could not reach the host");
        }
    }

    /// Sends the message `make` builds with a fresh request id, and waits for its answer.
    pub async fn ask(&self, make: impl FnOnce(u64) -> ToHost, timeout: Duration) -> Option<ToLogic> {
        let rid = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        self.waiting.lock().unwrap().insert(rid, tx);
        self.tell(make(rid)).await;
        let out = tokio::time::timeout(timeout, rx).await.ok().and_then(Result::ok);
        self.waiting.lock().unwrap().remove(&rid);
        out
    }

    fn answer(&self, rid: u64, msg: ToLogic) {
        if let Some(tx) = self.waiting.lock().unwrap().remove(&rid) {
            let _ = tx.send(msg);
        }
    }

    /// A voice op: `None` when it worked, else the line to show.
    pub async fn op(&self, make: impl FnOnce(u64) -> ToHost, timeout: Duration) -> Option<String> {
        match self.ask(make, timeout).await {
            Some(ToLogic::Done { error, .. }) => error,
            _ => Some("The voice host did not answer.".into()),
        }
    }
}

pub struct Ctx {
    pub rest: Rest,
    pub bridge: Bridge,
    pub host: HostLink,
    pub app_id: OnceLock<u64>,
    owners: tokio::sync::OnceCell<HashSet<u64>>,
    extra_users: HashSet<u64>,
    pub voice: Mutex<Vec<GuildVoice>>,
    pub allowed_links: HashSet<u64>,
    pub np_watchers: Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>,
    /// The Activity's side of DeetsMusic (`deetsmusic_remote`).
    pub remote: deetsmusic_remote::Remote,
}

/// The app `/dm` opens up for (docs/archive/bot/dm.py `APP`).
pub const APP: &str = "deetsmusic.exe";

impl Ctx {
    /// The application's owner or team, plus `VOICE_USER_IDS`: whoever may start `/vc`.
    pub async fn is_host(&self, user: u64) -> bool {
        if self.extra_users.contains(&user) {
            return true;
        }
        let owners = self
            .owners
            .get_or_init(|| async {
                match self.rest.owners().await {
                    Ok(o) => o.into_iter().collect(),
                    Err(e) => {
                        error!(error = %e, "who is the host: GET /applications/@me failed");
                        HashSet::new()
                    }
                }
            })
            .await;
        owners.contains(&user)
    }

    pub fn guild_voice(&self, guild: u64) -> Option<GuildVoice> {
        self.voice.lock().unwrap().iter().find(|g| g.guild == guild).cloned()
    }

    /// The app the bot streams in this server right now (`voice.tapping`).
    pub fn tapping(&self, guild: u64) -> Option<String> {
        self.guild_voice(guild).and_then(|g| g.tapping_app)
    }

    pub fn tapping_app(&self, guild: u64) -> bool {
        self.tapping(guild).is_some_and(|a| a.eq_ignore_ascii_case(APP))
    }

    /// Which voice channel `user` is in, in `guild`.
    pub async fn where_is(&self, guild: u64, user: u64) -> Option<u64> {
        match self.host.ask(|request_id| ToHost::WhereIs { request_id, guild, user }, Duration::from_secs(2)).await {
            Some(ToLogic::MemberVoice { channel, .. }) => channel,
            _ => None,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,lildeets_logic=debug".into()))
        .init();
    if let Err(e) = run().await {
        error!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let addr = std::env::var(ENV_ADDR).map_err(|_| format!("{ENV_ADDR} is not set: the host starts this process"))?;
    let token = std::env::var(ENV_TOKEN).map_err(|_| format!("{ENV_TOKEN} is not set"))?;
    let discord = env_or_dotenv("DISCORD_TOKEN").ok_or("DISCORD_TOKEN is not set (env or .env)")?;
    let extra_users = env_or_dotenv("VOICE_USER_IDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let sock = tokio::net::TcpStream::connect(&addr).await.map_err(|e| format!("ipc: connect {addr}: {e}"))?;
    let (mut reader, mut writer) = sock.into_split();
    lildeets_ipc::write(&mut writer, &ToHost::Hello { token, version: PROTOCOL }).await.map_err(|e| e.to_string())?;
    info!(%addr, "logic: connected to the host");

    let ctx = Arc::new(Ctx {
        rest: Rest::new(&discord),
        bridge: Bridge::default(),
        host: HostLink { writer: tokio::sync::Mutex::new(writer), next: AtomicU64::new(0), waiting: Mutex::new(HashMap::new()) },
        app_id: OnceLock::new(),
        owners: tokio::sync::OnceCell::new(),
        extra_users,
        voice: Mutex::new(Vec::new()),
        allowed_links: allowed_links_channels(),
        np_watchers: Mutex::new(HashMap::new()),
        remote: deetsmusic_remote::Remote::default(),
    });

    let mut watcher_started = false;
    loop {
        let msg = match lildeets_ipc::read::<_, ToLogic>(&mut reader).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                info!("logic: the host closed the link; stopping");
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        };
        match msg {
            ToLogic::Ready { app_id, bot_user_id, guilds } => {
                let _ = ctx.app_id.set(app_id);
                info!(app_id, bot_user_id, guilds = guilds.len(), "logic: ready");
                let c = ctx.clone();
                tokio::spawn(async move { commands::sync_commands(&c, app_id).await });
                if !watcher_started {
                    watcher_started = true;
                    tokio::spawn(watch::status_watcher(ctx.clone()));
                    tokio::spawn(deetsmusic_remote::watcher(ctx.clone()));
                }
            }
            ToLogic::Dispatch { event } => {
                let c = ctx.clone();
                tokio::spawn(async move { commands::handle(c, event).await });
            }
            ToLogic::VoiceSnapshot { request_id, guilds } => {
                *ctx.voice.lock().unwrap() = guilds.clone();
                if let Some(rid) = request_id {
                    ctx.host.answer(rid, ToLogic::VoiceSnapshot { request_id, guilds });
                }
            }
            m @ ToLogic::MemberVoice { request_id, .. } => ctx.host.answer(request_id, m),
            m @ ToLogic::Done { request_id, .. } => ctx.host.answer(request_id, m),
            ToLogic::RelayCheck { rid, user, instance } => {
                let c = ctx.clone();
                tokio::spawn(async move { relay::check(&c, rid, user, instance).await });
            }
            ToLogic::RelayViewers { viewers } => {
                let c = ctx.clone();
                tokio::spawn(async move { deetsmusic_remote::viewers(&c, viewers).await });
            }
            ToLogic::RelayData { rid, data } => {
                let c = ctx.clone();
                tokio::spawn(async move { deetsmusic_remote::request(&c, rid, data).await });
            }
        }
        if ctx.app_id.get().is_none() {
            warn!("logic: a message before Ready");
        }
    }
}
