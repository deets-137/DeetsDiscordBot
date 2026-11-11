//! Voice spike (rust-rewrite.md §7 step 1; rust-port-plan.md §6).
//!
//! Settles one question on a real call: does songbird 0.6's `driver`, fed by our own glue on
//! twilight-gateway 0.18, connect with DAVE and play sound? It is a throwaway: P5 builds the
//! real host.
//!
//! It runs on **Lil Deets' own token** (`DISCORD_TOKEN` in `.env`, the line the Python bot
//! reads; the owner, 2026-11-10). That is safe only while the live bot is not in a call: one
//! bot user can be in one voice channel per server, so a join would pull the live bot out of
//! its call. So the spike:
//! - never joins in a server where Lil Deets is already in a voice channel;
//! - stops, without sending a leave, if Lil Deets' voice state moves away from the spike's
//!   channel (the Python bot took the connection back);
//! - registers no commands (CLAUDE.md › Never: one command list per application).
//!
//! The Python bot ignores voice events in a server where it has no voice connection
//! (`bot/voice.py` `on_voice_state_update`), so it does not react to the spike.
//!
//! What it does:
//! 1. Connects to the gateway (intents: guilds, guild voice states). It registers no commands.
//! 2. Waits until a person (not a bot) is in the voice channel given by `--channel <id>`
//!    (required: it never joins a call by itself; on 2026-11-10 an auto-follow version
//!    joined the owner's call with friends).
//! 3. Sends op 4 (join, self-deafened), collects our VOICE_STATE_UPDATE (session id) and
//!    VOICE_SERVER_UPDATE (token, endpoint), and hands them to `songbird::Driver::connect`.
//! 4. Plays a 440 Hz tone for 3 s, then the app given by `--exe` (default `deetsmusic.exe`)
//!    through deets-airplay's per-process capture, if it is running.
//! 5. Leaves when it is the only one left in the channel, after `--secs` (default 180), or on
//!    Ctrl+C.
//!
//! ```text
//! cargo run -p lildeets-voice-spike --release -- --channel <voice channel id> [--exe <name.exe>] [--secs <n>]
//! ```
//!
//! Threads: deets-airplay's capture thread (lives as long as the capture); songbird's own
//! threads; the tokio runtime. Nothing else.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use songbird::input::core::io::MediaSource;
use songbird::input::{Input, RawAdapter};
use songbird::{Config, ConnectionInfo, Driver};
use tracing::{debug, error, info, warn};
use twilight_gateway::{Event, EventTypeFlags, Intents, MessageSender, Shard, ShardId, StreamExt as _};
use twilight_model::gateway::payload::outgoing::UpdateVoiceState;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, UserMarker};
use twilight_model::id::Id;

const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BLOCK_FRAMES: usize = 960; // 20 ms

/// Which call to join. The spike never picks one by itself.
#[derive(Clone, Debug)]
enum Pick {
    /// `--channel <id>`: this voice channel, once a person is in it.
    Channel(Id<ChannelMarker>),
    /// `--guild <name>`: the server whose name contains this (one server only), and there
    /// the one voice channel with a person in it. With two or more, it lists them and waits.
    Guild(String),
}

struct Args {
    pick: Pick,
    exe: String,
    secs: u64,
    /// `--clear`: the owner said the bot's voice state on the picked server is a leftover;
    /// send one leave there before joining (2026-11-10, after the first run's state stayed).
    clear: bool,
}

fn args() -> Result<Args, String> {
    let mut pick = None;
    let mut out = Args { pick: Pick::Guild(String::new()), exe: "deetsmusic.exe".into(), secs: 180, clear: false };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--clear" {
            out.clear = true;
            continue;
        }
        let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
        match flag.as_str() {
            "--channel" => pick = Some(Pick::Channel(value.parse().map_err(|_| "--channel wants a voice channel id")?)),
            "--guild" if !value.trim().is_empty() => pick = Some(Pick::Guild(value.trim().to_lowercase())),
            "--exe" => out.exe = value,
            "--secs" => out.secs = value.parse().map_err(|_| "--secs wants a number")?,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    out.pick = pick.ok_or("--channel <id> or --guild <name> is required: the spike never picks a call by itself")?;
    Ok(out)
}

/// `DISCORD_TOKEN` from the environment, else from the repo's `.env` (the Python bot's own
/// line, so the two never drift). A copy lives in `Documents/Deets' Secrets/`. Never logged.
fn token() -> Result<String, String> {
    if let Ok(t) = std::env::var("DISCORD_TOKEN") {
        return Ok(t);
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("DISCORD_TOKEN="))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
        .find(|v| !v.is_empty())
        .ok_or_else(|| "DISCORD_TOKEN is not set (env or .env)".into())
}

// ---------------------------------------------------------------------------------------
// Sources: f32 LE stereo bytes for songbird's RawAdapter.

/// A 440 Hz tone at -12 dB for `secs`, then end of stream.
struct Tone {
    frame: u64,
    total: u64,
    pending: Vec<u8>,
}

impl Tone {
    fn new(secs: u64) -> Self {
        Self { frame: 0, total: secs * RATE as u64, pending: Vec::new() }
    }
}

impl Read for Tone {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            let n = (self.total - self.frame).min(BLOCK_FRAMES as u64);
            for i in 0..n {
                let t = (self.frame + i) as f32 / RATE as f32;
                let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.25;
                for _ in 0..CHANNELS {
                    self.pending.extend_from_slice(&s.to_le_bytes());
                }
            }
            self.frame += n;
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n) // 0 once the tone is over: the end of the track
    }
}

impl Seek for Tone {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

impl MediaSource for Tone {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

// The app's sound: deets_tap::TapStream (crates/tap, P2), the spike's own `Tap` moved there.

// ---------------------------------------------------------------------------------------
// The glue: twilight's gateway on one side, songbird's driver on the other.

#[derive(Default)]
struct World {
    me: Option<Id<UserMarker>>,
    bots: HashSet<Id<UserMarker>>,
    /// Who is in which voice channel: user → (guild, channel).
    voice: HashMap<Id<UserMarker>, (Id<GuildMarker>, Id<ChannelMarker>)>,
    guild_names: HashMap<Id<GuildMarker>, String>,
    channel_names: HashMap<Id<ChannelMarker>, String>,
}

impl World {
    fn set(&mut self, user: Id<UserMarker>, guild: Option<Id<GuildMarker>>, channel: Option<Id<ChannelMarker>>) {
        match (guild, channel) {
            (Some(g), Some(c)) => {
                self.voice.insert(user, (g, c));
            }
            _ => {
                self.voice.remove(&user);
            }
        }
    }
    fn is_person(&self, u: &Id<UserMarker>) -> bool {
        !self.bots.contains(u) && Some(*u) != self.me
    }
    /// The voice channels in `guild` with a person in them.
    fn occupied(&self, guild: Id<GuildMarker>) -> Vec<Id<ChannelMarker>> {
        let mut out: Vec<_> = self.voice.iter().filter(|(u, (g, _))| *g == guild && self.is_person(u)).map(|(_, (_, c))| *c).collect();
        out.sort();
        out.dedup();
        out
    }
    /// The call to join, or why not yet (a line to log). Never while Lil Deets (this same
    /// bot user) is already in a voice channel on that server.
    fn target(&self, pick: &Pick) -> Result<(Id<GuildMarker>, Id<ChannelMarker>), String> {
        let found = match pick {
            Pick::Channel(channel) => self
                .voice
                .iter()
                .find(|(u, (_, c))| c == channel && self.is_person(u))
                .map(|(_, v)| *v)
                .ok_or_else(|| format!("waiting for a person in channel {channel}"))?,
            Pick::Guild(name) => {
                let guilds: Vec<_> = self.guild_names.iter().filter(|(_, n)| n.to_lowercase().contains(name.as_str())).map(|(g, _)| *g).collect();
                let [guild] = guilds[..] else {
                    return Err(format!("{} servers match {name:?}; need exactly one", guilds.len()));
                };
                match self.occupied(guild)[..] {
                    [] => return Err("waiting for a person in a voice channel there".into()),
                    [channel] => (guild, channel),
                    ref many => {
                        let list: Vec<String> = many
                            .iter()
                            .map(|c| format!("{} ({c})", self.channel_names.get(c).map_or("?", String::as_str)))
                            .collect();
                        return Err(format!("{} voice channels have people: {}. Not picking one; use --channel <id>", many.len(), list.join(", ")));
                    }
                }
            }
        };
        let busy = self.me.and_then(|me| self.voice.get(&me)).is_some_and(|(g, _)| *g == found.0);
        if busy {
            return Err("Lil Deets is already in a call on that server; not joining".into());
        }
        Ok(found)
    }
    /// Lil Deets' own voice channel in a server, as the gateway last reported it.
    fn my_channel(&self, guild: Id<GuildMarker>) -> Option<Id<ChannelMarker>> {
        self.me.and_then(|me| self.voice.get(&me)).filter(|(g, _)| *g == guild).map(|(_, c)| *c)
    }
    fn people_in(&self, channel: Id<ChannelMarker>) -> usize {
        self.voice.iter().filter(|(u, (_, c))| *c == channel && self.is_person(u)).count()
    }
}

enum Stage {
    Waiting,
    Joining { guild: Id<GuildMarker>, channel: Id<ChannelMarker>, session: Option<String>, server: Option<(String, String)> },
    Connected { guild: Id<GuildMarker>, channel: Id<ChannelMarker> },
}

fn nz<T>(id: Id<T>) -> NonZeroU64 {
    id.into_nonzero()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,songbird=debug,davey=info,lildeets_voice_spike=debug".into()),
        )
        .init();
    if let Err(e) = run().await {
        error!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args = args()?;
    let token = token()?;
    let mut shard = Shard::new(ShardId::ONE, token, Intents::GUILDS | Intents::GUILD_VOICE_STATES);
    let sender = shard.sender();
    let wanted = EventTypeFlags::READY | EventTypeFlags::GUILD_CREATE | EventTypeFlags::VOICE_STATE_UPDATE | EventTypeFlags::VOICE_SERVER_UPDATE;

    let mut world = World::default();
    let mut stage = Stage::Waiting;
    let mut driver = Driver::new(Config::default());
    let deadline = tokio::time::sleep(Duration::from_secs(args.secs));
    tokio::pin!(deadline);
    let mut deadline_armed = false;
    info!(pick = ?args.pick, "spike: connecting to the gateway");
    let mut last_reason = String::new();
    let mut cleared = false;

    loop {
        let event = tokio::select! {
            ev = shard.next_event(wanted) => ev,
            _ = tokio::signal::ctrl_c() => { info!("spike: Ctrl+C"); break; }
            _ = &mut deadline, if deadline_armed => { info!(secs = args.secs, "spike: time is up"); break; }
        };
        let event = match event {
            Some(Ok(e)) => e,
            Some(Err(e)) => {
                warn!(error = %e, "gateway: a message could not be read");
                continue;
            }
            None => return Err("gateway: the shard stopped".into()),
        };

        match event {
            Event::Ready(ready) => {
                world.me = Some(ready.user.id);
                info!(user = %ready.user.name, guilds = ready.guilds.len(), "gateway: ready");
            }
            Event::GuildCreate(gc) => {
                if let twilight_model::gateway::payload::incoming::GuildCreate::Available(g) = &*gc {
                    world.bots.extend(g.members.iter().filter(|m| m.user.bot).map(|m| m.user.id));
                    world.guild_names.insert(g.id, g.name.clone());
                    for c in &g.channels {
                        if let Some(name) = &c.name {
                            world.channel_names.insert(c.id, name.clone());
                        }
                    }
                    for vs in &g.voice_states {
                        world.set(vs.user_id, Some(g.id), vs.channel_id);
                    }
                    info!(guild = %g.name, in_voice = g.voice_states.len(), "gateway: guild");
                }
            }
            Event::VoiceStateUpdate(vsu) => {
                let vs = &vsu.0;
                if let Some(m) = &vs.member {
                    if m.user.bot {
                        world.bots.insert(m.user.id);
                    }
                }
                world.set(vs.user_id, vs.guild_id, vs.channel_id);
                if Some(vs.user_id) == world.me {
                    if let Stage::Joining { session, .. } = &mut stage {
                        debug!("voice: our VOICE_STATE_UPDATE (session id)");
                        *session = Some(vs.session_id.clone());
                    }
                }
            }
            Event::VoiceServerUpdate(vsu) => {
                if let Stage::Joining { server, .. } = &mut stage {
                    match &vsu.endpoint {
                        Some(endpoint) => {
                            debug!(%endpoint, "voice: VOICE_SERVER_UPDATE (token, endpoint)");
                            *server = Some((vsu.token.clone(), endpoint.clone()));
                        }
                        None => warn!("voice: the voice server has no endpoint yet; waiting for the next update"),
                    }
                }
            }
            _ => {}
        }

        // Move the stages on.
        match &mut stage {
            Stage::Waiting => {
                if args.clear && !cleared {
                    if let (Pick::Guild(name), Some(me)) = (&args.pick, world.me) {
                        let leftover = world.voice.get(&me).copied().filter(|(g, _)| {
                            world.guild_names.get(g).is_some_and(|n| n.to_lowercase().contains(name.as_str()))
                        });
                        if let Some((guild, channel)) = leftover {
                            info!(%guild, %channel, "voice: --clear: one leave for the leftover voice state");
                            send(&sender, UpdateVoiceState::new(guild, None::<Id<ChannelMarker>>, false, false));
                            world.voice.remove(&me);
                            cleared = true;
                        }
                    }
                }
                match world.target(&args.pick) {
                    Ok((guild, channel)) => {
                        let name = world.channel_names.get(&channel).cloned().unwrap_or_default();
                        info!(%guild, %channel, %name, "voice: joining (op 4, self-deafened)");
                        send(&sender, UpdateVoiceState::new(guild, Some(channel), true, false));
                        stage = Stage::Joining { guild, channel, session: None, server: None };
                    }
                    Err(reason) if reason != last_reason => {
                        info!("spike: {reason}");
                        last_reason = reason;
                    }
                    Err(_) => {}
                }
            }
            Stage::Joining { guild, channel, session: Some(session), server: Some((token, endpoint)) } => {
                let (guild, channel) = (*guild, *channel);
                let me = world.me.ok_or("no READY before the voice events")?;
                let info = ConnectionInfo {
                    channel_id: nz(channel).into(),
                    endpoint: endpoint.clone(),
                    guild_id: nz(guild).into(),
                    session_id: session.clone(),
                    token: token.clone(),
                    user_id: nz(me).into(),
                };
                info!("voice: handing the session to songbird's driver");
                let started = std::time::Instant::now();
                match driver.connect(info).await {
                    Ok(()) => info!(ms = started.elapsed().as_millis() as u64, "voice: CONNECTED (voice gateway, UDP, DAVE handled by songbird)"),
                    Err(e) => {
                        error!(error = %e, "voice: songbird could not connect");
                        send(&sender, UpdateVoiceState::new(guild, None::<Id<ChannelMarker>>, false, false));
                        return Err(format!("songbird connect failed: {e}"));
                    }
                }
                play(&mut driver, &args.exe).await;
                deadline.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(args.secs));
                deadline_armed = true;
                stage = Stage::Connected { guild, channel };
            }
            Stage::Connected { guild, channel } => {
                if world.my_channel(*guild) != Some(*channel) {
                    // Someone else (the Python bot) moved this bot user: give the call back.
                    warn!("voice: Lil Deets' voice state moved away from the spike's channel; stopping without a leave");
                    driver.leave();
                    stage = Stage::Waiting;
                    break;
                }
                if world.people_in(*channel) == 0 {
                    info!("voice: nobody is left in the channel; leaving");
                    break;
                }
            }
            _ => {}
        }
    }

    driver.leave();
    if let Stage::Connected { guild, .. } | Stage::Joining { guild, .. } = stage {
        send(&sender, UpdateVoiceState::new(guild, None::<Id<ChannelMarker>>, false, false));
        tokio::time::sleep(Duration::from_millis(500)).await; // let op 4 go out before the close
    }
    shard.close(twilight_gateway::CloseFrame::NORMAL);
    info!("spike: done");
    Ok(())
}

fn send(sender: &MessageSender, cmd: UpdateVoiceState) {
    if let Err(e) = sender.command(&cmd) {
        error!(error = %e, "gateway: op 4 could not be sent");
    }
}

/// The tone for 3 s, then the app's sound if it is running.
async fn play(driver: &mut Driver, exe: &str) {
    info!("play: 440 Hz tone for 3 s");
    let tone = driver.play_only_input(Input::from(RawAdapter::new(Tone::new(3), RATE, CHANNELS as u32)));
    tokio::time::sleep(Duration::from_millis(3_200)).await;
    if let Ok(state) = tone.get_info().await {
        debug!(?state.playing, position_ms = state.position.as_millis() as u64, "play: the tone's track state");
    }

    let pid = match deets_tap::resolve_exe(exe) {
        Ok(pid) => pid,
        Err(e) => {
            warn!(exe, error = %e, "play: the tone was the whole test");
            return;
        }
    };
    match deets_tap::TapStream::start(pid) {
        Ok(tap) => {
            info!(exe, pid, note = %tap.format_note(), "play: streaming the app");
            let track = driver.play_only_input(Input::from(RawAdapter::new(tap, RATE, CHANNELS as u32)));
            let track = Arc::new(track);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                match track.get_info().await {
                    Ok(state) => info!(?state.playing, position_ms = state.position.as_millis() as u64, "play: the app's track after 5 s"),
                    Err(e) => warn!(error = %e, "play: the app's track ended early"),
                }
            });
        }
        Err(e) => warn!(exe, pid, error = %e, "play: the capture did not start"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use songbird::input::codecs::{get_codec_registry, get_probe};

    /// The first run (2026-11-10) failed in the call with "no compatible track found":
    /// songbird ships no PCM decoder. This parses a RawAdapter input exactly as the mixer
    /// does, offline, and decodes one packet.
    #[tokio::test]
    async fn raw_f32_pcm_is_playable() {
        let input = Input::from(RawAdapter::new(Tone::new(1), RATE, CHANNELS as u32));
        let mut input = input.make_playable_async(get_codec_registry(), get_probe()).await.expect("songbird could not parse raw f32 PCM");
        let parsed = input.parsed_mut().expect("parsed");
        let packet = parsed.format.next_packet().expect("a packet");
        let decoded = parsed.decoder.decode(&packet).expect("decode");
        assert_eq!(decoded.spec().rate, RATE);
        assert_eq!(decoded.spec().channels.count(), CHANNELS);
        assert_eq!(decoded.frames(), BLOCK_FRAMES);
    }
}
