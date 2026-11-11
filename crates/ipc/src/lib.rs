//! Host ⇄ logic messages for Lil Deets' two processes (rust-port-plan.md §3.1–3.2).
//!
//! - **Transport.** The host binds `127.0.0.1:0` (the OS picks a free port: never a fixed
//!   one) and starts the logic with `LILDEETS_IPC=127.0.0.1:<port>` and a random 32-byte hex
//!   `LILDEETS_IPC_TOKEN`. The logic's first frame is [`ToHost::Hello`]; anything else, or a
//!   wrong token, closes the socket ([`accept`]).
//! - **Framing.** A 4-byte little-endian length, then that many bytes of UTF-8 JSON. One enum
//!   per direction, tagged by `"t"`. A frame over [`MAX_FRAME`] is refused (a gateway
//!   dispatch is the largest message; 4 MiB is far above any Discord payload).
//! - **Versions.** `Hello.version` is [`PROTOCOL`]. The host refuses a logic built for
//!   another protocol, so a half-rebuilt pair never talks past each other.
//!
//! Ids are `u64` (Discord snowflakes); the raw gateway dispatch travels as JSON untouched.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Bumped whenever a message changes shape. 3: the Activity's viewers, data and state.
pub const PROTOCOL: u32 = 3;
/// The largest frame either side accepts.
pub const MAX_FRAME: usize = 4 << 20;
/// The environment variables the host passes to the logic.
pub const ENV_ADDR: &str = "LILDEETS_IPC";
pub const ENV_TOKEN: &str = "LILDEETS_IPC_TOKEN";

/// One server's voice state, as the host sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuildVoice {
    pub guild: u64,
    /// The voice channel the bot is in, if any.
    pub bot_channel: Option<u64>,
    /// The app the bot streams (`deetsmusic.exe`), if any.
    pub tapping_app: Option<String>,
    /// The bot's own volume, 1.0 = as captured (`/dm volume`).
    pub gain: f32,
    pub dave_ready: bool,
    /// People (not bots) in the bot's channel.
    pub members_in_bot_channel: Vec<u64>,
    /// The bot lost its call here to a network drop and is getting back in. While so,
    /// `bot_channel` is `None` (it is not in the call). Defaults to false for an older host.
    #[serde(default)]
    pub rejoining: bool,
}

/// A person with the Activity open, checked by the bot (ACTIVITY.md §6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayViewer {
    /// The relay's id for that person's page.
    pub rid: String,
    pub user: u64,
    pub name: String,
    /// The voice channel of their Activity.
    pub channel: u64,
}

/// Host → logic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ToLogic {
    /// After the gateway's READY, and to every new logic at once.
    Ready { app_id: u64, bot_user_id: u64, guilds: Vec<u64> },
    /// A gateway dispatch the logic handles (INTERACTION_CREATE), as Discord sent it.
    Dispatch { event: Value },
    /// The voice state of every server: on every change, and when asked.
    VoiceSnapshot { request_id: Option<u64>, guilds: Vec<GuildVoice> },
    /// The answer to [`ToHost::WhereIs`].
    MemberVoice { request_id: u64, user: u64, guild: u64, channel: Option<u64> },
    /// The result of a voice op; `error` is a line to show the user.
    Done { request_id: u64, error: Option<String> },
    /// The Activity's relay asks: is `user` in the Activity instance `instance`, and in which
    /// channel? The logic asks Discord (it has the REST client) and answers with
    /// [`ToHost::RelayChecked`] and the same `rid` (ACTIVITY.md §6).
    RelayCheck { rid: String, user: u64, instance: String },
    /// Everyone with the Activity open now: on every change, and to every new logic at once
    /// (the host keeps the list; the logic is swapped while music plays).
    RelayViewers { viewers: Vec<RelayViewer> },
    /// A request from a person's page (deetsmusic-protocol `Request`), as it came.
    RelayData { rid: String, data: Value },
}

/// What a voice op plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sound {
    /// An app by exe name (`/vc play`).
    App { exe: String },
    /// A test tone (`/vc tone`).
    Tone,
}

/// Logic → host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ToHost {
    /// The first frame, and only the first.
    Hello { token: String, version: u32 },
    /// Which voice channel a member is in (the host caches voice states).
    WhereIs { request_id: u64, guild: u64, user: u64 },
    /// Join `channel` (or move there) and play `sound` (`voice.start_stream`).
    Play { request_id: u64, guild: u64, channel: u64, sound: Sound },
    /// Stop the stream, stay in the channel.
    Stop { request_id: u64, guild: u64 },
    /// Leave the voice channel.
    Leave { request_id: u64, guild: u64 },
    SetGain { request_id: u64, guild: u64, gain: f32 },
    /// "Listening to …" (gateway op 3); `None` clears it.
    Presence { listening: Option<String> },
    /// Ask for a [`ToLogic::VoiceSnapshot`] now.
    Snapshot { request_id: u64 },
    /// The answer to a [`ToLogic::RelayCheck`]: the instance's voice channel when Discord
    /// confirms the person is in it; else `None`, with a line for that person.
    RelayChecked { rid: String, channel: Option<u64>, reason: Option<String> },
    /// The state for every person whose call the bot is in (deetsmusic-protocol `Reply`).
    RelayState { data: Value },
    /// Something for one person's page only.
    RelayTo { rid: String, data: Value },
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    TooLarge(usize),
    BadJson(String),
    /// The first frame was not a Hello, or its token or version is wrong.
    Refused(&'static str),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Io(e) => write!(f, "ipc: {e}"),
            IpcError::TooLarge(n) => write!(f, "ipc: a {n}-byte frame is over the limit"),
            IpcError::BadJson(e) => write!(f, "ipc: a frame is not a message: {e}"),
            IpcError::Refused(why) => write!(f, "ipc: refused: {why}"),
        }
    }
}

impl std::error::Error for IpcError {}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::Io(e)
    }
}

/// Writes one message as a frame.
pub async fn write<W: AsyncWrite + Unpin, M: Serialize>(w: &mut W, msg: &M) -> Result<(), IpcError> {
    let body = serde_json::to_vec(msg).map_err(|e| IpcError::BadJson(e.to_string()))?;
    if body.len() > MAX_FRAME {
        return Err(IpcError::TooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_le_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

/// Reads one frame as a message. `Ok(None)` when the other side closed between frames.
pub async fn read<R: AsyncRead + Unpin, M: for<'de> Deserialize<'de>>(r: &mut R) -> Result<Option<M>, IpcError> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(IpcError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    serde_json::from_slice(&body).map(Some).map_err(|e| IpcError::BadJson(e.to_string()))
}

/// The host's side of a new connection: the first frame must be a `Hello` with this token
/// and [`PROTOCOL`]. The token is compared in constant time.
pub async fn accept<R: AsyncRead + Unpin>(r: &mut R, token: &str) -> Result<(), IpcError> {
    match read::<_, ToHost>(r).await? {
        Some(ToHost::Hello { token: got, version }) => {
            if !same(got.as_bytes(), token.as_bytes()) {
                return Err(IpcError::Refused("wrong token"));
            }
            if version != PROTOCOL {
                return Err(IpcError::Refused("another protocol version"));
            }
            Ok(())
        }
        Some(_) => Err(IpcError::Refused("the first frame was not a Hello")),
        None => Err(IpcError::Refused("closed before a Hello")),
    }
}

/// Equal bytes, in time that depends only on the lengths.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A random 32-byte token as 64 hex characters, from the OS's cryptographic random source
/// (`RtlGenRandom` on Windows, `/dev/urandom` elsewhere). No extra crate.
pub fn new_token() -> String {
    token_from(&os_random_32())
}

/// 32 bytes as lower-case hex.
pub fn token_from(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(windows)]
fn os_random_32() -> [u8; 32] {
    // RtlGenRandom (SystemFunction036) from advapi32: the OS CSPRNG, no extra crate.
    #[link(name = "advapi32")]
    extern "system" {
        #[link_name = "SystemFunction036"]
        fn rtl_gen_random(buf: *mut u8, len: u32) -> u8;
    }
    let mut out = [0u8; 32];
    // SAFETY: `out` is a valid, writable 32-byte buffer for the call's duration.
    let ok = unsafe { rtl_gen_random(out.as_mut_ptr(), out.len() as u32) };
    assert!(ok != 0, "the OS random source failed");
    out
}

#[cfg(not(windows))]
fn os_random_32() -> [u8; 32] {
    use std::io::Read;
    let mut out = [0u8; 32];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut out)).expect("the OS random source failed");
    out
}
