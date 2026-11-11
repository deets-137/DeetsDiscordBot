//! `lildeets-host`: Lil Deets' long-lived process (rust-port-plan.md §3.1, P5).
//!
//! It holds everything a restart would cut: the gateway (twilight-gateway 0.18), the voice
//! connections (songbird's `driver` with our glue, proved by the voice spike), the capture
//! streams (`deets_tap::TapStream`), and the bot's presence. It starts `lildeets-logic`, which
//! handles every interaction, and talks to it over `lildeets-ipc`.
//!
//! - **Logic swaps with no gap.** The host runs a *copy* of `lildeets-logic.exe`
//!   (`lildeets-logic.run-<n>.exe`), so a rebuild can overwrite the original while it runs.
//!   When the original changes on disk, the host starts the new one, waits for its `Hello`,
//!   then stops the old one. The music never stops.
//! - **Voice.** `Play` joins (op 4, self-deafened), collects our VOICE_STATE_UPDATE (session)
//!   and VOICE_SERVER_UPDATE (token, endpoint), and hands them to `Driver::connect`. The
//!   capture opens on a blocking thread (`spawn_blocking`), never on the runtime.
//! - **Leave when alone**, as `docs/archive/bot/voice.py` does: when someone leaves the bot's channel and
//!   no person is left in it.
//!
//! - **The song-boundary restart** (docs/archive/bot/live.py, LIVE-RELOAD.md §4). The launcher runs this
//!   host as a copy (`lildeets-host.run.exe`), so cargo can write a new `lildeets-host.exe`.
//!   When that original changes and stays the same for one more look (2 s), the host waits
//!   for the song to end (or for a pause) if it is playing DeetsMusic, pauses DeetsMusic
//!   through the bridge, writes `.lildeets-resume.json` (beside, flushed, renamed over),
//!   leaves voice and exits with 75. The launcher starts the new host, which rejoins, starts
//!   the same app, waits [`DAVE_SETTLE`], and presses play. Not playing DeetsMusic: at once.
//! - **`.lildeets-hold`**: while it exists, neither a logic swap nor a host restart happens;
//!   changes wait (CLAUDE.md › Never).
//!
//! Threads (rule 10):
//! | thread | does | lives |
//! |---|---|---|
//! | tokio workers | the gateway, IPC, voice glue, the wait for a song boundary | the process |
//! | tokio blocking pool | opening a capture (`TapStream::for_exe`) | per open, < 5 s |
//! | deets-airplay capture | per-process loopback for one stream | as long as the stream |
//! | songbird mixer / UDP / WS | Opus, DAVE, RTP | as long as a call |

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use deets_tap::TapStream;
use deetsmusic_bridge::Bridge;
use lildeets_ipc::{GuildVoice, Sound, ToHost, ToLogic, ENV_ADDR, ENV_TOKEN, PROTOCOL};
use serde_json::Value;
use songbird::input::{Input, RawAdapter};
use songbird::tracks::TrackHandle;
use songbird::{Config, ConnectionInfo, Driver};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::process::Child;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use twilight_gateway::{Event, EventTypeFlags, Intents, MessageSender, Shard, ShardId, StreamExt as _};
use twilight_model::gateway::payload::outgoing::update_presence::UpdatePresencePayload;
use twilight_model::gateway::payload::outgoing::{UpdatePresence, UpdateVoiceState};
use twilight_model::gateway::presence::{ActivityType, MinimalActivity, Status};
use twilight_model::id::marker::{ChannelMarker, GuildMarker};
use twilight_model::id::Id;

mod tone;

const RESUME_FILE: &str = ".lildeets-resume.json";
const RESUME_MAX_AGE: Duration = Duration::from_secs(180);
/// While this file exists, changes queue up and nothing goes live (docs/archive/bot/live.py `HOLD_FILE`).
const HOLD_FILE: &str = ".lildeets-hold";
/// EX_TEMPFAIL, "run me again": the launcher starts the new host (crates/launcher).
const RESTART_CODE: i32 = 75;
/// Set by the launcher: it will start the host again on [`RESTART_CODE`].
const ENV_LAUNCHED: &str = "LILDEETS_LAUNCHED";
/// The app whose song boundary the restart waits for.
const DEETSMUSIC: &str = "deetsmusic.exe";
/// After a resume, the wait before DeetsMusic plays again. songbird sends the first packets
/// before the call's DAVE session is ready and does not say when it is (docs/archive/bot/live.py waits on
/// discord.py's `can_encrypt`, up to 6 s); the spike connected in 258–741 ms.
const DAVE_SETTLE: Duration = Duration::from_secs(2);
/// The command-sync record of `lildeets-logic` (crates/logic commands.rs). Cleared at start.
const SYNC_FILE: &str = ".lildeets-commands.sync";
/// The network probe while the gateway is down: the gateway's host, how soon after the
/// close, how often (also the connect timeout), and the least time between shard swaps.
const PROBE_HOST: &str = "gateway.discord.gg:443";
const PROBE_AFTER: Duration = Duration::from_secs(3);
const PROBE_EVERY: Duration = Duration::from_secs(3);
const REPLACE_EVERY: Duration = Duration::from_secs(15);
const INTENTS: Intents = Intents::GUILDS.union(Intents::GUILD_VOICE_STATES);
const JOIN_TIMEOUT: Duration = Duration::from_secs(15);
const TONE_SECS: u64 = 5;

/// The repo root: `.env`, the resume file, and the logic's working folder.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `DISCORD_TOKEN` from the environment, else from the repo's `.env`. Never logged.
fn token() -> Result<String, String> {
    if let Ok(t) = std::env::var("DISCORD_TOKEN") {
        return Ok(t);
    }
    let text = std::fs::read_to_string(root().join(".env")).map_err(|e| format!("cannot read .env: {e}"))?;
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("DISCORD_TOKEN="))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
        .find(|v| !v.is_empty())
        .ok_or_else(|| "DISCORD_TOKEN is not set (env or .env)".into())
}

fn nz(id: u64) -> NonZeroU64 {
    NonZeroU64::new(id).expect("Discord ids are never 0")
}

// --- messages into the main loop ----------------------------------------------------------

enum Inbox {
    /// A logic process connected and sent this Hello.
    Hello { token: String, version: u32, reader: tokio::net::tcp::OwnedReadHalf, writer: OwnedWriteHalf },
    FromLogic { gen: u64, msg: ToHost },
    /// The logic's connection ended; `error` when it was not a clean close.
    LogicGone { gen: u64, error: Option<String> },
    /// `sound` is `None` for a join with nothing to play (a resume of a silent call).
    /// `gen`: the driver it was for; a newer driver (a leave, a newer join) drops it.
    Connected { guild: u64, gen: u64, request_id: u64, channel: u64, sound: Option<Sound>, result: Result<(), String> },
    TapOpened { guild: u64, gen: u64, request_id: u64, exe: String, result: Result<TapStream, String> },
    /// The song boundary came (or no wait was needed): restart now.
    RestartNow { paused_by_us: bool },
    /// songbird gave up on a call (its own retries failed). `gen` is the driver's.
    VoiceLost { guild: u64, gen: u64, reason: String },
    /// The pause asked for when a call was lost: did we pause DeetsMusic?
    LostPaused { paused: bool },
    /// A call moved to a new voice server (VOICE_SERVER_UPDATE): songbird's reconnect.
    Moved { guild: u64, gen: u64, result: Result<(), String> },
    /// The gateway is down and Discord answered a TCP probe: the network is back.
    ProbeOk,
}

/// songbird's driver events for one call, into the main loop (rule 12: log what acts alone).
/// Written without `async_trait` (no new crate): the boxed future is what that macro makes.
struct DriverEvents {
    guild: u64,
    gen: u64,
    inbox: mpsc::UnboundedSender<Inbox>,
}

impl songbird::EventHandler for DriverEvents {
    fn act<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        ctx: &'life1 songbird::EventContext<'life2>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<songbird::Event>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        use songbird::events::context_data::DisconnectReason as R;
        match ctx {
            songbird::EventContext::DriverDisconnect(d) => match &d.reason {
                // Ours (a leave, a new connect on this driver): nothing was lost.
                None | Some(R::Requested) | Some(R::AttemptDiscarded) => {}
                Some(reason) => {
                    let _ = self.inbox.send(Inbox::VoiceLost { guild: self.guild, gen: self.gen, reason: format!("{:?}: {reason:?}", d.kind) });
                }
            },
            songbird::EventContext::DriverReconnect(_) => info!(guild = self.guild, "voice: songbird reconnected the call by itself"),
            _ => {}
        }
        Box::pin(async { None })
    }
}

/// A call the bot lost and is getting back (the owner, 2026-11-10: "Build it").
struct Lost {
    channel: u64,
    app: Option<String>,
    since: Instant,
    next_try: Instant,
    tries: u32,
    /// A join for it is in flight.
    rejoining: bool,
}

// --- voice state --------------------------------------------------------------------------

enum Playing {
    Nothing,
    App(String),
    /// A tone, until this time.
    Tone(Instant),
}

struct Call {
    driver: Driver,
    /// This driver's number: songbird events from an older driver are ignored.
    gen: u64,
    channel: Option<u64>,
    /// Our voice session (VOICE_STATE_UPDATE), for a move to a new voice server.
    session: Option<String>,
    connected: bool,
    playing: Playing,
    track: Option<TrackHandle>,
}

struct Pending {
    request_id: u64,
    channel: u64,
    sound: Option<Sound>,
    session: Option<String>,
    server: Option<(String, String)>,
    started: Instant,
}

// --- the logic process --------------------------------------------------------------------

struct Supervisor {
    /// The built logic: `lildeets-logic.exe` beside this exe.
    source: PathBuf,
    addr: String,
    gen: u64,
    /// gen → (child, its run copy, its token)
    children: HashMap<u64, (Child, PathBuf, String)>,
    /// The source's mtime when the running logic was copied from it.
    running_mtime: Option<SystemTime>,
    /// The source's mtime seen on the last check (a rebuild must be stable for one tick).
    seen_mtime: Option<SystemTime>,
    next_try: Instant,
}

impl Supervisor {
    fn mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.source).and_then(|m| m.modified()).ok()
    }

    async fn spawn(&mut self) -> Result<u64, String> {
        self.gen += 1;
        let gen = self.gen;
        let run = self.source.with_file_name(format!("lildeets-logic.run-{gen}.exe"));
        let mtime = self.mtime();
        tokio::fs::copy(&self.source, &run).await.map_err(|e| format!("copy {}: {e}", self.source.display()))?;
        let token = lildeets_ipc::new_token();
        let child = tokio::process::Command::new(&run)
            .env(ENV_ADDR, &self.addr)
            .env(ENV_TOKEN, &token)
            .current_dir(root())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("start {}: {e}", run.display()))?;
        info!(gen, "logic: started (run copy {})", run.display());
        self.children.insert(gen, (child, run, token));
        self.running_mtime = mtime;
        // Give it time to say Hello before anything counts it as missing.
        self.next_try = Instant::now() + Duration::from_secs(10);
        Ok(gen)
    }

    /// The gen whose token this is (compared in constant time against each).
    fn gen_for(&self, token: &str) -> Option<u64> {
        self.children.iter().find(|(_, (_, _, t))| same(t.as_bytes(), token.as_bytes())).map(|(g, _)| *g)
    }

    /// Stops every logic but `keep`, and removes their run copies.
    async fn retire_all_but(&mut self, keep: u64) {
        let old: Vec<u64> = self.children.keys().copied().filter(|g| *g != keep).collect();
        for gen in old {
            if let Some((mut child, run, _)) = self.children.remove(&gen) {
                let _ = child.kill().await;
                let _ = tokio::fs::remove_file(&run).await;
                info!(gen, "logic: stopped the old one");
            }
        }
    }
}

fn held() -> bool {
    root().join(HOLD_FILE).exists()
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The host's own exe, watched for a rebuild. Only under the launcher: it runs this host as
/// a copy, so the original is free to change and someone will start the new one.
struct SelfWatch {
    /// `lildeets-host.exe` beside the running copy.
    source: PathBuf,
    /// Its mtime when this host started.
    started: Option<SystemTime>,
    /// Its mtime on the last look (a rebuild must be stable for one tick).
    seen: Option<SystemTime>,
}

impl SelfWatch {
    fn new() -> Option<Self> {
        let me = std::env::current_exe().ok()?;
        let source = me.with_file_name("lildeets-host.exe");
        if std::env::var_os(ENV_LAUNCHED).is_none() || me == source {
            info!("host: not run by lildeets.exe; a rebuilt host goes live only by hand");
            return None;
        }
        let started = mtime_of(&source);
        info!(watching = %source.display(), "host: armed the song-boundary restart");
        Some(Self { source, started, seen: started })
    }

    /// True while the original differs from the one this host started from and stayed the
    /// same for one look ([`Host::request_restart`] acts on the first).
    fn rebuilt(&mut self) -> bool {
        let now = mtime_of(&self.source);
        let stable = now.is_some() && now == self.seen;
        self.seen = now;
        stable && now != self.started
    }
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// --- the host -----------------------------------------------------------------------------

struct Host {
    sender: MessageSender,
    inbox: mpsc::UnboundedSender<Inbox>,
    me: Option<u64>,
    app_id: Option<u64>,
    guilds: Vec<u64>,
    bots: HashSet<u64>,
    /// user → (guild, channel)
    voice: HashMap<u64, (u64, u64)>,
    calls: HashMap<u64, Call>,
    gains: HashMap<u64, f32>,
    pending: HashMap<u64, Pending>,
    logic: Option<(u64, OwnedWriteHalf)>,
    resumed: bool,
    /// DeetsMusic's bridge, for the song-boundary restart and its resume.
    bridge: Arc<Bridge>,
    /// A rebuilt host is waiting for its song boundary.
    restarting: bool,
    /// We paused DeetsMusic (a drop, a restart) and owe it a play (`maybe_resume_dm`).
    dm_paused: bool,
    /// Guilds whose GUILD_CREATE (the voice list) has arrived.
    loaded: HashSet<u64>,
    /// A change the logic has not seen yet (set where `push_snapshot` cannot be awaited).
    snapshot_stale: bool,
    /// Calls lost to a network drop, being rejoined.
    lost: HashMap<u64, Lost>,
    /// The next driver's number.
    driver_gen: u64,
    /// "Listening to", as last asked: sent again after a new gateway session.
    listening: Option<String>,
    /// How many READYs: from the second on, Discord has forgotten presence and voice.
    readies: u32,
    /// Since when the gateway is down (a GatewayClose with no READY / RESUMED since).
    gateway_down: Option<Instant>,
    /// When the last TCP probe was sent, and when the shard was last replaced.
    last_probe: Instant,
    last_replace: Instant,
}

impl Host {
    async fn send_logic(&mut self, msg: ToLogic) {
        let Some((gen, w)) = self.logic.as_mut() else {
            debug!("ipc: no logic connected; dropped a message");
            return;
        };
        if let Err(e) = lildeets_ipc::write(w, &msg).await {
            warn!(gen = *gen, error = %e, "ipc: the logic stopped listening");
            self.logic = None;
        }
    }

    async fn done(&mut self, request_id: u64, error: Option<String>) {
        if request_id != 0 {
            self.send_logic(ToLogic::Done { request_id, error }).await;
        }
    }

    fn is_person(&self, user: u64) -> bool {
        !self.bots.contains(&user) && Some(user) != self.me
    }

    fn people_in(&self, channel: u64) -> Vec<u64> {
        let mut v: Vec<u64> = self.voice.iter().filter(|(u, (_, c))| *c == channel && self.is_person(**u)).map(|(u, _)| *u).collect();
        v.sort_unstable();
        v
    }

    fn gain(&self, guild: u64) -> f32 {
        self.gains.get(&guild).copied().unwrap_or(1.0)
    }

    fn snapshot(&self) -> Vec<GuildVoice> {
        let mut v: Vec<GuildVoice> = self
            .calls
            .iter()
            .filter(|(g, _)| !self.lost.contains_key(*g))
            .filter_map(|(g, c)| {
                let channel = c.channel?;
                Some(GuildVoice {
                    guild: *g,
                    bot_channel: Some(channel),
                    tapping_app: match &c.playing {
                        Playing::App(exe) => Some(exe.clone()),
                        _ => None,
                    },
                    gain: self.gain(*g),
                    dave_ready: c.connected,
                    members_in_bot_channel: self.people_in(channel),
                    rejoining: false,
                })
            })
            .collect();
        // A call being rejoined after a drop: not in it yet, but on its way back.
        v.extend(self.lost.keys().map(|g| GuildVoice {
            guild: *g,
            bot_channel: None,
            tapping_app: None,
            gain: self.gain(*g),
            dave_ready: false,
            members_in_bot_channel: Vec::new(),
            rejoining: true,
        }));
        v
    }

    async fn push_snapshot(&mut self, request_id: Option<u64>) {
        let guilds = self.snapshot();
        self.send_logic(ToLogic::VoiceSnapshot { request_id, guilds }).await;
    }

    fn op4(&self, guild: u64, channel: Option<u64>) {
        let cmd = UpdateVoiceState::new(Id::<GuildMarker>::from(nz(guild)), channel.map(|c| Id::<ChannelMarker>::from(nz(c))), true, false);
        if let Err(e) = self.sender.command(&cmd) {
            error!(error = %e, "gateway: op 4 could not be sent");
        }
    }

    fn call(&mut self, guild: u64) -> &mut Call {
        if !self.calls.contains_key(&guild) {
            self.driver_gen += 1;
            let gen = self.driver_gen;
            let mut driver = Driver::new(Config::default());
            for event in [songbird::CoreEvent::DriverDisconnect, songbird::CoreEvent::DriverReconnect] {
                driver.add_global_event(songbird::Event::Core(event), DriverEvents { guild, gen, inbox: self.inbox.clone() });
            }
            self.calls.insert(guild, Call { driver, gen, channel: None, session: None, connected: false, playing: Playing::Nothing, track: None });
        }
        self.calls.get_mut(&guild).expect("inserted")
    }

    // --- voice ops ------------------------------------------------------------------------

    /// Joins `channel` if needed, then plays `sound` (`None`: join only).
    async fn play(&mut self, request_id: u64, guild: u64, channel: u64, sound: Option<Sound>) {
        let ready = self.calls.get(&guild).is_some_and(|c| c.connected && c.channel == Some(channel));
        if ready {
            match sound {
                Some(sound) => self.start_sound(guild, request_id, sound).await,
                None => {
                    self.done(request_id, None).await;
                    self.settled(guild);
                }
            }
            return;
        }
        if let Some(old) = self.pending.remove(&guild) {
            self.done(old.request_id, Some("Another join replaced this one.".into())).await;
        }
        info!(guild, channel, "voice: joining (op 4, self-deafened)");
        self.call(guild);
        self.op4(guild, Some(channel));
        self.pending.insert(guild, Pending { request_id, channel, sound, session: None, server: None, started: Instant::now() });
    }

    /// When a join has its session and its server, hand them to songbird.
    fn try_connect(&mut self, guild: u64) {
        let complete = self.pending.get(&guild).is_some_and(|p| p.session.is_some() && p.server.is_some());
        let Some(me) = self.me else { return };
        if !complete {
            return;
        }
        let p = self.pending.remove(&guild).expect("checked");
        let (token, endpoint) = p.server.expect("checked");
        let session = p.session.expect("checked");
        let info = ConnectionInfo {
            channel_id: nz(p.channel).into(),
            endpoint,
            guild_id: nz(guild).into(),
            session_id: session.clone(),
            token,
            user_id: nz(me).into(),
        };
        let call = self.call(guild);
        call.session = Some(session);
        let (connect, gen) = (call.driver.connect(info), call.gen);
        let inbox = self.inbox.clone();
        let (request_id, channel, sound) = (p.request_id, p.channel, p.sound);
        tokio::spawn(async move {
            let started = Instant::now();
            let result = connect.await.map_err(|e| e.to_string());
            if result.is_ok() {
                info!(guild, channel, ms = started.elapsed().as_millis() as u64, "voice: connected (DAVE by songbird)");
            }
            let _ = inbox.send(Inbox::Connected { guild, gen, request_id, channel, sound, result });
        });
    }

    async fn start_sound(&mut self, guild: u64, request_id: u64, sound: Sound) {
        match sound {
            Sound::Tone => {
                let gain = self.gain(guild);
                let call = self.call(guild);
                let track = call.driver.play_only_input(Input::from(RawAdapter::new(tone::Tone::new(TONE_SECS), deets_tap::RATE, 2)));
                let _ = track.set_volume(gain);
                call.track = Some(track);
                call.playing = Playing::Tone(Instant::now() + Duration::from_secs(TONE_SECS));
                info!(guild, "voice: tone for {TONE_SECS} s");
                self.done(request_id, None).await;
                self.push_snapshot(None).await;
            }
            Sound::App { exe } => {
                let exe = if exe.to_ascii_lowercase().ends_with(".exe") { exe } else { format!("{exe}.exe") };
                let inbox = self.inbox.clone();
                let gen = self.calls.get(&guild).map_or(0, |c| c.gen);
                tokio::task::spawn_blocking(move || {
                    let result = TapStream::for_exe(&exe);
                    let _ = inbox.send(Inbox::TapOpened { guild, gen, request_id, exe, result });
                });
            }
        }
    }

    async fn tap_opened(&mut self, guild: u64, gen: u64, request_id: u64, exe: String, result: Result<TapStream, String>) {
        // The call this capture was opened for is gone (a leave, or a newer join).
        if !self.calls.get(&guild).is_some_and(|c| c.gen == gen && c.connected) {
            debug!(guild, gen, %exe, "voice: a capture for an older call; dropped");
            self.done(request_id, Some("The bot left that call before the app started.".into())).await;
            return;
        }
        match result {
            Ok(tap) => {
                info!(guild, %exe, note = tap.format_note(), "voice: streaming");
                let gain = self.gain(guild);
                let call = self.call(guild);
                let track = call.driver.play_only_input(Input::from(RawAdapter::new(tap, deets_tap::RATE, 2)));
                let _ = track.set_volume(gain);
                call.track = Some(track);
                call.playing = Playing::App(exe);
                self.done(request_id, None).await;
                self.push_snapshot(None).await;
                self.maybe_resume_dm("a call hears DeetsMusic again");
            }
            Err(e) => {
                if request_id == 0 {
                    error!(guild, %exe, error = %e, "resume: could not capture the app again");
                }
                self.done(request_id, Some(format!("Couldn't capture {exe}: {e}"))).await;
            }
        }
        self.settled(guild);
    }

    /// A join or a capture for `guild` finished (or failed).
    fn settled(&mut self, guild: u64) {
        self.rejoin_settled(guild);
    }

    fn taps_dm(c: &Call) -> bool {
        c.connected && matches!(&c.playing, Playing::App(exe) if exe.eq_ignore_ascii_case(DEETSMUSIC))
    }

    /// Plays DeetsMusic again if we paused it (a drop, a restart), only once a call hears it
    /// and no call that heard it is still lost. One DeetsMusic serves every call, so this is
    /// one flag for the host, not one per call. Nobody listening: it stays paused.
    fn maybe_resume_dm(&mut self, why: &'static str) {
        let dm_lost = self.lost.values().any(|l| l.app.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(DEETSMUSIC)));
        if self.dm_paused && !dm_lost && self.calls.values().any(Self::taps_dm) {
            self.dm_paused = false;
            self.resume_deetsmusic(why);
        }
    }

    /// Plays DeetsMusic again after [`DAVE_SETTLE`] (we paused it; the call hears again).
    fn resume_deetsmusic(&self, why: &'static str) {
        let bridge = self.bridge.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DAVE_SETTLE).await;
            match bridge.command("play", None).await {
                Ok(_) => info!(why, "resume: DeetsMusic playing again"),
                Err(e) => warn!(why, error = %e, "resume: DeetsMusic could not be resumed"),
            }
        });
    }

    // --- network drops (docs/ops/DEBUGGING.md, "the network dropped") -----------------------

    /// The bot lost its call in `guild` (songbird gave up, or a new gateway session dropped
    /// it): forget the driver, pause DeetsMusic if the call was hearing it, and rejoin from
    /// [`Host::tick`] while a person is still in the channel.
    async fn lose_call(&mut self, guild: u64, why: &str) {
        let Some(mut call) = self.calls.remove(&guild) else { return };
        call.driver.leave();
        let Some(channel) = call.channel else { return };
        let app = match &call.playing {
            Playing::App(exe) => Some(exe.clone()),
            _ => None,
        };
        warn!(guild, channel, why, app = app.as_deref().unwrap_or("-"), "voice: lost the call; rejoining it");
        // Discord may still list us in the channel; a clean leave makes the rejoin start fresh.
        self.op4(guild, None);
        // Pause only if no other call still hears DeetsMusic (one DeetsMusic serves them all).
        let was_dm = app.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(DEETSMUSIC));
        if was_dm && !self.calls.values().any(Self::taps_dm) {
            let (bridge, inbox) = (self.bridge.clone(), self.inbox.clone());
            tokio::spawn(async move {
                let paused = pause_if_playing(&bridge).await;
                if paused {
                    info!(guild, "voice: paused DeetsMusic while the call can't hear it");
                }
                let _ = inbox.send(Inbox::LostPaused { paused });
            });
        }
        let now = Instant::now();
        // A second drop before the first rejoin finished keeps its start time and tries.
        let (since, tries) = self.lost.get(&guild).map_or((now, 0), |l| (l.since, l.tries));
        self.lost.insert(guild, Lost { channel, app, since, next_try: now + Duration::from_secs(2), tries, rejoining: false });
        self.push_snapshot(None).await;
    }

    /// Drops a rejoin in progress for `guild`. True if there was one.
    fn forget_lost(&mut self, guild: u64, by: &str) -> bool {
        let Some(l) = self.lost.remove(&guild) else { return false };
        self.snapshot_stale = true;
        info!(guild, by, dm_paused = self.dm_paused, "voice: the rejoin is cancelled by a command");
        if l.rejoining {
            self.op4(guild, None);
            self.pending.remove(&guild);
            self.calls.remove(&guild);
        }
        // Another call may still hear DeetsMusic; a /vc play of it resumes from tap_opened.
        self.maybe_resume_dm("a lost call was cancelled; another call hears DeetsMusic");
        true
    }

    /// Once a second: start the rejoins that are due (only while the gateway is up).
    async fn rejoin_due(&mut self) {
        if self.gateway_down.is_some() {
            return;
        }
        let now = Instant::now();
        let due: Vec<u64> = self.lost.iter().filter(|(_, l)| !l.rejoining && now >= l.next_try).map(|(g, _)| *g).collect();
        for guild in due {
            // Until its GUILD_CREATE, a guild's voice list is empty: wait for it.
            if !self.loaded.contains(&guild) {
                continue;
            }
            let channel = self.lost[&guild].channel;
            if self.people_in(channel).is_empty() {
                self.lost.remove(&guild);
                self.snapshot_stale = true;
                info!(guild, channel, dm_paused = self.dm_paused, "voice: nobody is left in the channel; not rejoining");
                self.maybe_resume_dm("a lost call was given up; another call hears DeetsMusic");
                continue;
            }
            let l = self.lost.get_mut(&guild).expect("listed");
            l.rejoining = true;
            l.tries += 1;
            let (tries, app) = (l.tries, l.app.clone());
            info!(guild, channel, tries, "voice: rejoining the lost call");
            self.play(0, guild, channel, app.map(|exe| Sound::App { exe })).await;
        }
    }

    /// A rejoin's join finished (or failed): back, or try again later.
    fn rejoin_settled(&mut self, guild: u64) {
        if !self.lost.get(&guild).is_some_and(|l| l.rejoining) {
            return;
        }
        // Back means in the call AND hearing the app again (a failed capture is not back:
        // the next try finds the call ready and only opens the capture again).
        let wanted = self.lost[&guild].app.is_some();
        let back = self.calls.get(&guild).is_some_and(|c| c.connected && (!wanted || matches!(c.playing, Playing::App(_))));
        if back {
            let l = self.lost.remove(&guild).expect("checked");
            self.snapshot_stale = true;
            info!(guild, secs = l.since.elapsed().as_secs(), tries = l.tries, "voice: back in the call");
            // If everyone left during the handshake, `alone_check` (each second) leaves.
            self.maybe_resume_dm("back in the call after a drop");
        } else {
            let l = self.lost.get_mut(&guild).expect("checked");
            l.rejoining = false;
            let wait = Duration::from_secs((2u64 << l.tries.min(3)).min(15));
            l.next_try = Instant::now() + wait;
            warn!(guild, tries = l.tries, wait_secs = wait.as_secs(), "voice: the rejoin failed; trying again");
        }
    }

    /// Once a second while the gateway is down: every 3 s, can Discord be reached? (One TCP
    /// connect to the gateway's host; no Discord API call.) twilight's own retry waits grow
    /// to 255 s, so the host replaces the shard once the network answers.
    fn probe_if_down(&mut self) {
        let Some(since) = self.gateway_down else { return };
        if since.elapsed() < PROBE_AFTER || self.last_probe.elapsed() < PROBE_EVERY {
            return;
        }
        self.last_probe = Instant::now();
        let inbox = self.inbox.clone();
        tokio::spawn(async move {
            let ok = matches!(tokio::time::timeout(PROBE_EVERY, tokio::net::TcpStream::connect(PROBE_HOST)).await, Ok(Ok(_)));
            if ok {
                let _ = inbox.send(Inbox::ProbeOk);
            }
        });
    }

    // --- the song-boundary restart --------------------------------------------------------

    /// The host's exe was rebuilt: wait for a song boundary, then send [`Inbox::RestartNow`].
    fn request_restart(&mut self) {
        if self.restarting {
            return;
        }
        self.restarting = true;
        let playing_dm = self.calls.values().any(|c| matches!(&c.playing, Playing::App(exe) if exe.eq_ignore_ascii_case(DEETSMUSIC)));
        info!(playing_dm, "restart: lildeets-host.exe changed; restarting at the next song boundary");
        let bridge = self.bridge.clone();
        let inbox = self.inbox.clone();
        tokio::spawn(async move {
            let paused_by_us = if playing_dm {
                wait_for_boundary(&bridge).await;
                pause_if_playing(&bridge).await
            } else {
                false
            };
            let _ = inbox.send(Inbox::RestartNow { paused_by_us });
        });
    }

    /// Where every call is, for the new host (docs/archive/bot/live.py `restart`'s fields). A call being
    /// rejoined after a drop is written too, so the new host carries on with it, and a
    /// pause owed from a drop counts as ours.
    fn resume_state(&self, paused_by_us: bool) -> Value {
        let at = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let mut streams: Vec<Value> = self
            .calls
            .iter()
            .filter(|(g, c)| c.connected && !self.lost.contains_key(*g))
            .filter_map(|(g, c)| {
                let app = match &c.playing {
                    Playing::App(exe) => Some(exe.clone()),
                    _ => None,
                };
                Some(serde_json::json!({"guild": g, "channel": c.channel?, "app": app, "gain": self.gain(*g)}))
            })
            .collect();
        streams.extend(self.lost.iter().map(|(g, l)| serde_json::json!({"guild": g, "channel": l.channel, "app": l.app, "gain": self.gain(*g)})));
        serde_json::json!({"at": at, "paused_by_us": paused_by_us || self.dm_paused, "streams": streams, "reason": ["lildeets-host.exe"]})
    }

    fn is_playing(&self, guild: u64) -> bool {
        match self.calls.get(&guild).map(|c| &c.playing) {
            Some(Playing::App(_)) => true,
            Some(Playing::Tone(until)) => *until > Instant::now(),
            _ => false,
        }
    }

    async fn stop(&mut self, request_id: u64, guild: u64) {
        if !self.is_playing(guild) {
            self.done(request_id, Some("Nothing is playing.".into())).await;
            return;
        }
        let call = self.call(guild);
        call.driver.stop();
        call.track = None;
        call.playing = Playing::Nothing;
        info!(guild, "voice: stopped");
        self.done(request_id, None).await;
        self.push_snapshot(None).await;
    }

    async fn leave(&mut self, request_id: u64, guild: u64, why: &str) {
        let in_call = self.calls.get(&guild).is_some_and(|c| c.channel.is_some());
        if !in_call {
            self.done(request_id, Some("Not in a voice channel.".into())).await;
            return;
        }
        if let Some(mut call) = self.calls.remove(&guild) {
            call.driver.leave();
        }
        self.op4(guild, None);
        info!(guild, why, "voice: left");
        self.done(request_id, None).await;
        self.push_snapshot(None).await;
    }

    async fn set_gain(&mut self, request_id: u64, guild: u64, gain: f32) {
        let gain = gain.clamp(0.0, 2.0);
        self.gains.insert(guild, gain);
        if let Some(track) = self.calls.get(&guild).and_then(|c| c.track.as_ref()) {
            let _ = track.set_volume(gain);
        }
        self.done(request_id, None).await;
        self.push_snapshot(None).await;
    }

    /// Sets "Listening to …", or clears it. Built by hand: twilight's `UpdatePresence::new`
    /// refuses an empty activity list, which is how Discord clears one (seen 2026-11-10).
    fn presence(&self, listening: Option<String>) {
        let activities = listening
            .map(|name| vec![MinimalActivity { kind: ActivityType::Listening, name, url: None }.into()])
            .unwrap_or_default();
        let cmd = UpdatePresence {
            d: UpdatePresencePayload { activities, afk: false, since: None, status: Status::Online },
            op: twilight_model::gateway::OpCode::PresenceUpdate,
        };
        if let Err(e) = self.sender.command(&cmd) {
            warn!(error = %e, "gateway: presence could not be sent");
        }
    }

    fn gateway_up(&mut self, how: &str) {
        if let Some(since) = self.gateway_down.take() {
            info!(how, secs = since.elapsed().as_secs(), "gateway: back");
            // A new session forgot "Listening to"; a swapped shard lost what was queued on
            // the old sender. Either way: send it again (one gateway op).
            self.presence(self.listening.clone());
        }
    }

    async fn on_logic(&mut self, msg: ToHost) {
        match msg {
            ToHost::Hello { .. } => {}
            ToHost::WhereIs { request_id, guild, user } => {
                let channel = self.voice.get(&user).filter(|(g, _)| *g == guild).map(|(_, c)| *c);
                self.send_logic(ToLogic::MemberVoice { request_id, user, guild, channel }).await;
            }
            // A person's /vc command replaces a rejoin in progress.
            ToHost::Play { request_id, guild, channel, sound } => {
                self.forget_lost(guild, "/vc play");
                self.play(request_id, guild, channel, Some(sound)).await;
            }
            ToHost::Stop { request_id, guild } => {
                if self.forget_lost(guild, "/vc stop") {
                    self.done(request_id, None).await;
                } else {
                    self.stop(request_id, guild).await;
                }
            }
            ToHost::Leave { request_id, guild } => {
                if self.forget_lost(guild, "/vc leave") {
                    self.done(request_id, None).await;
                } else {
                    self.leave(request_id, guild, "asked").await;
                }
            }
            ToHost::SetGain { request_id, guild, gain } => self.set_gain(request_id, guild, gain).await,
            ToHost::Presence { listening } => {
                self.listening = listening.clone();
                self.presence(listening);
            }
            ToHost::Snapshot { request_id } => self.push_snapshot(Some(request_id)).await,
        }
    }

    // --- the gateway ----------------------------------------------------------------------

    async fn gateway(&mut self, event: Event) {
        match event {
            Event::Ready(ready) => {
                self.me = Some(ready.user.id.get());
                self.app_id = Some(ready.application.id.get());
                self.guilds = ready.guilds.iter().map(|g| g.id.get()).collect();
                self.readies += 1;
                self.gateway_up("a new session");
                info!(user = %ready.user.name, guilds = self.guilds.len(), session = self.readies, "gateway: ready");
                self.send_ready().await;
            }
            Event::Resumed => self.gateway_up("resumed"),
            Event::GatewayClose(frame) => {
                if self.gateway_down.is_none() {
                    self.gateway_down = Some(Instant::now());
                    warn!(code = frame.as_ref().map(|f| f.code), "gateway: the connection closed; twilight reconnects, and the host probes the network");
                }
            }
            Event::GuildCreate(gc) => {
                if let twilight_model::gateway::payload::incoming::GuildCreate::Available(g) = &*gc {
                    let gid = g.id.get();
                    self.loaded.insert(gid);
                    self.bots.extend(g.members.iter().filter(|m| m.user.bot).map(|m| m.user.id.get()));
                    // The guild's whole voice list: drop what we had (people who left while
                    // the gateway was down), then take Discord's.
                    self.voice.retain(|_, (vg, _)| *vg != gid);
                    for vs in &g.voice_states {
                        if let Some(c) = vs.channel_id {
                            self.voice.insert(vs.user_id.get(), (gid, c.get()));
                        }
                    }
                    if !self.guilds.contains(&gid) {
                        self.guilds.push(gid);
                    }
                    // After a new session: is the bot still in its call there?
                    let in_call = self.calls.get(&gid).filter(|c| c.connected).and_then(|c| c.channel);
                    if let (Some(channel), Some(me)) = (in_call, self.me) {
                        if self.voice.get(&me) != Some(&(gid, channel)) {
                            self.lose_call(gid, "not in the call after a new gateway session").await;
                        }
                    }
                }
                self.try_resume().await;
            }
            Event::VoiceStateUpdate(vsu) => {
                let vs = &vsu.0;
                let user = vs.user_id.get();
                if let Some(m) = &vs.member {
                    if m.user.bot {
                        self.bots.insert(user);
                    }
                }
                let before = self.voice.get(&user).copied();
                match (vs.guild_id, vs.channel_id) {
                    (Some(g), Some(c)) => {
                        self.voice.insert(user, (g.get(), c.get()));
                    }
                    _ => {
                        self.voice.remove(&user);
                    }
                }
                let guild = vs.guild_id.map(|g| g.get()).or(before.map(|(g, _)| g));
                if Some(user) == self.me {
                    if let Some(g) = guild {
                        if let Some(p) = self.pending.get_mut(&g) {
                            p.session = Some(vs.session_id.clone());
                            self.try_connect(g);
                        } else if vs.channel_id.is_none() {
                            // Disconnected from outside (kicked, channel deleted).
                            if let Some(mut call) = self.calls.remove(&g) {
                                call.driver.leave();
                                info!(guild = g, "voice: disconnected by Discord");
                            }
                        } else if let Some(call) = self.calls.get_mut(&g) {
                            call.channel = vs.channel_id.map(|c| c.get());
                            call.session = Some(vs.session_id.clone());
                        }
                    }
                } else if let Some((g, old_channel)) = before {
                    // Leave when alone (docs/archive/bot/voice.py on_voice_state_update).
                    let bot_channel = self.calls.get(&g).and_then(|c| c.channel);
                    let left_it = vs.channel_id.map(|c| c.get()) != Some(old_channel);
                    if bot_channel == Some(old_channel) && left_it && self.people_in(old_channel).is_empty() {
                        self.leave(0, g, "nobody is left in the channel").await;
                        return;
                    }
                }
                if guild.is_some_and(|g| self.calls.contains_key(&g)) {
                    self.push_snapshot(None).await;
                }
            }
            Event::VoiceServerUpdate(vsu) => {
                let g = vsu.guild_id.get();
                if let Some(p) = self.pending.get_mut(&g) {
                    match &vsu.endpoint {
                        Some(endpoint) => {
                            p.server = Some((vsu.token.clone(), endpoint.clone()));
                            self.try_connect(g);
                        }
                        None => warn!(guild = g, "voice: the voice server has no endpoint yet"),
                    }
                } else if let (Some(endpoint), Some(me)) = (&vsu.endpoint, self.me) {
                    // Discord moved a call in progress to another voice server: connect this
                    // driver to it (songbird keeps the track playing across a connect).
                    let Some(call) = self.calls.get_mut(&g).filter(|c| c.connected) else { return };
                    let (Some(channel), Some(session)) = (call.channel, call.session.clone()) else { return };
                    info!(guild = g, "voice: Discord moved the call to another voice server; following it");
                    let info = ConnectionInfo {
                        channel_id: nz(channel).into(),
                        endpoint: endpoint.clone(),
                        guild_id: nz(g).into(),
                        session_id: session,
                        token: vsu.token.clone(),
                        user_id: nz(me).into(),
                    };
                    let (connect, gen, inbox) = (call.driver.connect(info), call.gen, self.inbox.clone());
                    tokio::spawn(async move {
                        let result = connect.await.map_err(|e| e.to_string());
                        let _ = inbox.send(Inbox::Moved { guild: g, gen, result });
                    });
                }
            }
            Event::InteractionCreate(ic) => match serde_json::to_value(&ic.0) {
                Ok(event) => {
                    if self.logic.is_none() {
                        warn!("interaction: no logic is connected; it will time out");
                    }
                    self.send_logic(ToLogic::Dispatch { event }).await;
                }
                Err(e) => error!(error = %e, "interaction: could not re-serialize"),
            },
            _ => {}
        }
    }

    async fn send_ready(&mut self) {
        if let (Some(app_id), Some(bot_user_id)) = (self.app_id, self.me) {
            let guilds = self.guilds.clone();
            self.send_logic(ToLogic::Ready { app_id, bot_user_id, guilds }).await;
            self.push_snapshot(None).await;
        }
    }

    /// Replays a fresh `.lildeets-resume.json` once (docs/archive/bot/live.py's fields): rejoin and play.
    async fn try_resume(&mut self) {
        if self.resumed || self.me.is_none() {
            return;
        }
        self.resumed = true;
        let path = root().join(RESUME_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let _ = std::fs::remove_file(&path);
        let Ok(state) = serde_json::from_str::<Value>(&text) else { return };
        let at = state["at"].as_f64().unwrap_or(0.0);
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        if now - at > RESUME_MAX_AGE.as_secs_f64() {
            info!("resume: the hand-off is stale; ignored");
            return;
        }
        let paused_by_us = state["paused_by_us"].as_bool().unwrap_or(false);
        let streams: Vec<(u64, u64, Option<String>, f32)> = state["streams"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| Some((s["guild"].as_u64()?, s["channel"].as_u64()?, s["app"].as_str().map(str::to_owned), s["gain"].as_f64().unwrap_or(1.0) as f32)))
            .collect();
        info!(calls = streams.len(), paused_by_us, "resume: replaying the hand-off");
        // Each call goes through the rejoin path (retries, the "anyone there?" check), and
        // DeetsMusic plays again once a call hears it (`maybe_resume_dm`).
        self.dm_paused = paused_by_us;
        let now = Instant::now();
        for (guild, channel, app, gain) in streams {
            self.gains.insert(guild, gain);
            info!(guild, channel, app = app.as_deref().unwrap_or("-"), "resume: rejoining");
            self.lost.insert(guild, Lost { channel, app, since: now, next_try: now, tries: 0, rejoining: false });
        }
    }

    /// Once a second: joins that never completed, rejoins that are due, the network probe.
    async fn tick(&mut self) {
        self.timeouts().await;
        self.alone_check().await;
        self.rejoin_due().await;
        self.probe_if_down();
        if std::mem::take(&mut self.snapshot_stale) {
            self.push_snapshot(None).await;
        }
    }

    /// Leave-when-alone, as a safety net: `VoiceStateUpdate` catches the last person
    /// leaving, but not a call that ends up empty on its own (everyone left during a rejoin
    /// or a resume). Only while the gateway is up: the voice list is stale when it is down.
    async fn alone_check(&mut self) {
        if self.gateway_down.is_some() {
            return;
        }
        let alone: Vec<u64> = self
            .calls
            .iter()
            .filter(|(g, c)| c.connected && !self.pending.contains_key(*g))
            .filter(|(_, c)| c.channel.is_some_and(|ch| self.people_in(ch).is_empty()))
            .map(|(g, _)| *g)
            .collect();
        for g in alone {
            self.leave(0, g, "nobody is in the channel").await;
        }
    }

    async fn timeouts(&mut self) {
        let late: Vec<u64> = self.pending.iter().filter(|(_, p)| p.started.elapsed() > JOIN_TIMEOUT).map(|(g, _)| *g).collect();
        for g in late {
            if let Some(p) = self.pending.remove(&g) {
                warn!(guild = g, "voice: the join timed out");
                self.op4(g, None);
                self.calls.remove(&g);
                self.done(p.request_id, Some(format!("Couldn't join <#{}>: Discord did not answer.", p.channel))).await;
                self.settled(g);
            }
        }
    }
}

/// Waits until the song DeetsMusic plays now ends or pauses (checked each second, as
/// docs/archive/bot/live.py does).
async fn wait_for_boundary(bridge: &Bridge) {
    let start = bridge.now_playing().await.ok();
    if let Some(np) = start.as_ref().filter(|np| np.playing) {
        if let Some(first) = np.catalog_id.clone().or_else(|| np.title.clone()) {
            info!(title = np.title.as_deref().unwrap_or(""), "restart: waiting for the song to end");
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                match bridge.now_playing().await {
                    Ok(np) if np.playing && np.catalog_id.clone().or_else(|| np.title.clone()).as_ref() == Some(&first) => continue,
                    Ok(_) => break,
                    Err(e) => {
                        warn!(error = %e, "restart: the bridge stopped answering; restarting now");
                        break;
                    }
                }
            }
        }
    }
}

/// Pauses DeetsMusic if it plays. True when we paused it (so we play it again later).
async fn pause_if_playing(bridge: &Bridge) -> bool {
    match bridge.now_playing().await {
        Ok(np) if np.playing => match bridge.command("pause", None).await {
            Ok(_) => true,
            Err(e) => {
                warn!(error = %e, "could not pause DeetsMusic");
                false
            }
        },
        _ => false,
    }
}

/// Writes beside, flushes, then renames over (CLAUDE.md › Working style; rust-port-plan rule 11).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,songbird=warn,lildeets_host=debug".into()))
        .init();
    match run().await {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    }
}

/// Runs the bot; returns the exit code ([`RESTART_CODE`] for the song-boundary restart).
async fn run() -> Result<i32, String> {
    let token = token()?;
    // The logic skips the command sync when this record matches its own list. Another bot on
    // this token (the Python bot) may have synced since, so each host start syncs once.
    match std::fs::remove_file(root().join(SYNC_FILE)) {
        Ok(()) => info!("host: cleared the command-sync record; the logic will sync once"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(error = %e, "host: could not clear the command-sync record"),
    }
    let mut self_watch = SelfWatch::new();
    let mut exit_code = 0;
    // The OS picks a free port (several Deets apps run on this PC at once).
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| format!("ipc: bind: {e}"))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?.to_string();
    let source = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("lildeets-logic.exe");
    // Run copies left by a host that was killed (it removes its own on a clean stop).
    if let Ok(dir) = std::fs::read_dir(source.parent().unwrap_or(Path::new("."))) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("lildeets-logic.run-") && name.ends_with(".exe") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let mut sup = Supervisor { source, addr: addr.clone(), gen: 0, children: HashMap::new(), running_mtime: None, seen_mtime: None, next_try: Instant::now() };

    let (inbox_tx, mut inbox) = mpsc::unbounded_channel::<Inbox>();
    // Accept logic connections: read the first frame, hand it to the main loop.
    let tx = inbox_tx.clone();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move {
                let (mut reader, writer) = sock.into_split();
                match tokio::time::timeout(Duration::from_secs(10), lildeets_ipc::read::<_, ToHost>(&mut reader)).await {
                    Ok(Ok(Some(ToHost::Hello { token, version }))) => {
                        let _ = tx.send(Inbox::Hello { token, version, reader, writer });
                    }
                    _ => warn!("ipc: a connection sent no Hello; closed"),
                }
            });
        }
    });

    let mut shard = Shard::new(ShardId::ONE, token.clone(), INTENTS);
    let wanted = EventTypeFlags::READY
        | EventTypeFlags::RESUMED
        | EventTypeFlags::GUILD_CREATE
        | EventTypeFlags::VOICE_STATE_UPDATE
        | EventTypeFlags::VOICE_SERVER_UPDATE
        | EventTypeFlags::INTERACTION_CREATE;
    let mut host = Host {
        sender: shard.sender(),
        inbox: inbox_tx.clone(),
        me: None,
        app_id: None,
        guilds: Vec::new(),
        bots: HashSet::new(),
        voice: HashMap::new(),
        calls: HashMap::new(),
        gains: HashMap::new(),
        pending: HashMap::new(),
        logic: None,
        resumed: false,
        bridge: Arc::new(Bridge::default()),
        restarting: false,
        dm_paused: false,
        loaded: HashSet::new(),
        snapshot_stale: false,
        lost: HashMap::new(),
        driver_gen: 0,
        listening: None,
        readies: 0,
        gateway_down: None,
        last_probe: Instant::now(),
        last_replace: Instant::now() - REPLACE_EVERY,
    };
    info!(%addr, logic = %sup.source.display(), "host: starting");
    if let Err(e) = sup.spawn().await {
        error!("logic: {e}");
    }

    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let mut exe_check = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            ev = shard.next_event(wanted) => match ev {
                Some(Ok(event)) => host.gateway(event).await,
                Some(Err(e)) => warn!(error = %e, "gateway: a message could not be read"),
                None => return Err("gateway: the shard stopped".into()),
            },
            Some(msg) = inbox.recv() => match msg {
                Inbox::Hello { token, version, reader, writer } => {
                    let Some(gen) = sup.gen_for(&token) else {
                        warn!("ipc: a Hello with an unknown token; closed");
                        continue;
                    };
                    if version != PROTOCOL {
                        warn!(gen, version, "ipc: the logic speaks another protocol; closed");
                        continue;
                    }
                    info!(gen, "logic: connected");
                    host.logic = Some((gen, writer));
                    spawn_reader(gen, reader, inbox_tx.clone());
                    host.send_ready().await;
                    sup.retire_all_but(gen).await;
                }
                Inbox::FromLogic { gen, msg } => {
                    if host.logic.as_ref().is_some_and(|(g, _)| *g == gen) {
                        host.on_logic(msg).await;
                    }
                }
                Inbox::LogicGone { gen, error } => {
                    if host.logic.as_ref().is_some_and(|(g, _)| *g == gen) {
                        warn!(gen, error = error.as_deref().unwrap_or("-"), "logic: disconnected; starting it again in 2 s");
                        host.logic = None;
                        sup.next_try = Instant::now() + Duration::from_secs(2);
                    } else {
                        // An old logic we stopped after a swap: its socket resets (os error 10054).
                        debug!(gen, error = error.as_deref().unwrap_or("-"), "ipc: an old logic's connection ended");
                    }
                }
                Inbox::Connected { guild, gen, request_id, .. } if host.calls.get(&guild).is_none_or(|c| c.gen != gen) => {
                    debug!(guild, gen, "voice: a connect for an older call; dropped");
                    host.done(request_id, Some("Another join replaced this one.".into())).await;
                }
                Inbox::Connected { guild, request_id, channel, sound, result, .. } => match result {
                    Ok(()) => {
                        let call = host.call(guild);
                        call.connected = true;
                        call.channel = Some(channel);
                        match sound {
                            Some(sound) => host.start_sound(guild, request_id, sound).await,
                            None => {
                                host.done(request_id, None).await;
                                host.push_snapshot(None).await;
                                host.settled(guild);
                            }
                        }
                    }
                    Err(e) => {
                        error!(guild, error = %e, "voice: songbird could not connect");
                        host.op4(guild, None);
                        host.calls.remove(&guild);
                        host.done(request_id, Some(format!("Couldn't join <#{channel}>: {e}"))).await;
                        host.settled(guild);
                    }
                },
                Inbox::TapOpened { guild, gen, request_id, exe, result } => host.tap_opened(guild, gen, request_id, exe, result).await,
                Inbox::RestartNow { paused_by_us } => {
                    let state = host.resume_state(paused_by_us);
                    let calls = state["streams"].as_array().map_or(0, Vec::len);
                    match write_atomic(&root().join(RESUME_FILE), state.to_string().as_bytes()) {
                        Ok(()) => info!(calls, paused_by_us, "restart: hand-off written; restarting"),
                        Err(e) => error!(error = %e, "restart: could not write the hand-off; restarting without it"),
                    }
                    exit_code = RESTART_CODE;
                    break;
                }
                Inbox::VoiceLost { guild, gen, reason } => {
                    if host.calls.get(&guild).is_some_and(|c| c.gen == gen) {
                        host.lose_call(guild, &reason).await;
                    } else {
                        debug!(guild, gen, %reason, "voice: a disconnect from an old driver; ignored");
                    }
                }
                Inbox::LostPaused { paused } => {
                    host.dm_paused |= paused;
                    // Back already, faster than the pause: play again now (if a call hears it).
                    host.maybe_resume_dm("back in the call before the pause landed");
                }
                Inbox::Moved { guild, gen, result } => {
                    if host.calls.get(&guild).is_some_and(|c| c.gen == gen) {
                        match result {
                            Ok(()) => info!(guild, "voice: on the new voice server"),
                            Err(e) => host.lose_call(guild, &format!("the move to a new voice server failed: {e}")).await,
                        }
                    }
                }
                Inbox::ProbeOk => {
                    // The network answers but twilight may sit in a long retry wait: a new
                    // shard that resumes the same session connects now.
                    if host.gateway_down.is_some() && host.last_replace.elapsed() >= REPLACE_EVERY {
                        host.last_replace = Instant::now();
                        let mut config = twilight_gateway::ConfigBuilder::new(token.clone(), INTENTS);
                        if let (Some(session), Some(url)) = (shard.session().cloned(), shard.resume_url().map(str::to_owned)) {
                            config = config.session(session).resume_url(url);
                        }
                        shard = Shard::with_config(ShardId::ONE, config.build());
                        host.sender = shard.sender();
                        info!(down_secs = host.gateway_down.map_or(0, |d| d.elapsed().as_secs()), "gateway: the network answers; reconnecting now (a new shard resuming the session)");
                    }
                }
            },
            _ = ticker.tick() => host.tick().await,
            _ = exe_check.tick() => {
                // While .lildeets-hold exists, a multi-file change is in progress: rebuilds wait.
                let hold = held();
                if !hold && self_watch.as_mut().is_some_and(SelfWatch::rebuilt) {
                    host.request_restart();
                }
                let mtime = sup.mtime();
                let stable = mtime.is_some() && mtime == sup.seen_mtime;
                sup.seen_mtime = mtime;
                let rebuilt = !hold && stable && mtime != sup.running_mtime;
                let missing = host.logic.is_none() && Instant::now() >= sup.next_try;
                if rebuilt || missing {
                    if rebuilt {
                        info!("logic: lildeets-logic.exe changed; starting the new one beside the old");
                    } else {
                        warn!("logic: none connected; starting one");
                    }
                    if let Err(e) = sup.spawn().await {
                        error!("logic: {e}");
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("host: Ctrl+C; leaving every call");
                break;
            }
        }
    }
    let restarting = exit_code == RESTART_CODE;
    let guilds: Vec<u64> = host.calls.keys().copied().collect();
    for g in guilds {
        host.leave(0, g, if restarting { "the host is restarting" } else { "the host is stopping" }).await;
    }
    if !restarting {
        host.presence(None); // a restart keeps "Listening to" for the few seconds of the gap
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    sup.retire_all_but(u64::MAX).await;
    shard.close(twilight_gateway::CloseFrame::NORMAL);
    info!(restarting, "host: stopped");
    Ok(exit_code)
}

fn spawn_reader(gen: u64, mut reader: tokio::net::tcp::OwnedReadHalf, inbox: mpsc::UnboundedSender<Inbox>) {
    tokio::spawn(async move {
        let error = loop {
            match lildeets_ipc::read::<_, ToHost>(&mut reader).await {
                Ok(Some(msg)) => {
                    if inbox.send(Inbox::FromLogic { gen, msg }).is_err() {
                        return;
                    }
                }
                Ok(None) => break None,
                // A reset or a bad frame: the main loop decides whether it matters.
                Err(e) => break Some(e.to_string()),
            }
        };
        let _ = inbox.send(Inbox::LogicGone { gen, error });
    });
}
