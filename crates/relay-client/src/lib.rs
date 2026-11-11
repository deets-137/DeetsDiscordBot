//! The host's connection to the Activity's relay (docs/features/ACTIVITY.md §5, §6).
//!
//! The PC cannot be reached from the internet, so the bot connects **out** to the relay
//! Worker and keeps one WebSocket open. This crate knows the relay and the call. It knows
//! nothing of DeetsMusic.
//!
//! - **Only while the bot is in a call.** [`Relay::set_channels`] with a channel connects;
//!   with none it disconnects. An idle bot holds no socket.
//! - **It signs in** with `Authorization: Bearer <host secret>` (`LILDEETS_RELAY_SECRET`); the
//!   relay holds the same value as a Worker secret.
//! - **It reconnects by itself**, 2 s after the first failure and up to 60 s between tries,
//!   and sends the channels again on every connect.
//! - **It logs** when it arms, connects, drops and stops (target `lildeets_host`).
//!
//! The wire is `activity/relay/src/wire.ts`: one JSON object per text frame, tagged by `t`.
//! Discord ids travel as strings there and are `u64` here. Change both files together and
//! bump [`WIRE`].

use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use http::header::{HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_websockets::{ClientBuilder, Message};
use tracing::{debug, info, warn};

/// The relay's wire version (`WIRE` in wire.ts).
pub const WIRE: u32 = 2;
/// A text frame the relay answers with `pong` without waking its room.
const PING: &str = "ping";
const PING_EVERY: Duration = Duration::from_secs(30);
/// No frame at all for this long: the socket is dead, connect again.
const SILENCE_MAX: Duration = Duration::from_secs(75);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);

/// What the relay asks of the bot, or tells it.
#[derive(Clone, Debug, PartialEq)]
pub enum FromRelay {
    /// Is `user` in the Activity instance `instance`, and in which channel? Answer with
    /// [`Relay::checked`]. The bot asks Discord with its own token, which never leaves the PC.
    Check { rid: String, user: u64, instance: String },
    /// A checked person has the Activity open in `channel`. Sent again after each reconnect.
    Joined { rid: String, user: u64, name: String, channel: u64 },
    /// That person closed the Activity.
    Left { rid: String },
    /// A request from that person's page (deetsmusic-protocol `Request`), passed on as it came.
    Data { rid: String, data: serde_json::Value },
    /// The connection opened or closed: every person the relay named before is gone.
    Reset,
}

/// The answer to a [`FromRelay::Check`].
#[derive(Clone, Debug, PartialEq)]
pub enum Checked {
    /// The person is in the instance, and the instance is in this voice channel.
    In { channel: u64 },
    /// Not confirmed. The line is shown to that person.
    No { reason: String },
}

// --- the wire -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum WireIn {
    Check { rid: String, user: String, instance: String },
    Joined { rid: String, user: String, name: String, channel: String },
    Left { rid: String },
    Data { rid: String, data: serde_json::Value },
}

#[derive(Debug, Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum WireOut<'a> {
    Channels { channels: Vec<String> },
    Checked {
        rid: &'a str,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'a str>,
    },
    State { data: &'a serde_json::Value },
    To { rid: &'a str, data: &'a serde_json::Value },
}

fn channels_frame(channels: &[u64]) -> String {
    serde_json::to_string(&WireOut::Channels { channels: channels.iter().map(u64::to_string).collect() }).expect("plain JSON")
}

fn checked_frame(rid: &str, answer: &Checked) -> String {
    let out = match answer {
        Checked::In { channel } => WireOut::Checked { rid, ok: true, channel: Some(channel.to_string()), reason: None },
        Checked::No { reason } => WireOut::Checked { rid, ok: false, channel: None, reason: Some(reason) },
    };
    serde_json::to_string(&out).expect("plain JSON")
}

fn state_frame(data: &serde_json::Value) -> String {
    serde_json::to_string(&WireOut::State { data }).expect("plain JSON")
}

fn to_frame(rid: &str, data: &serde_json::Value) -> String {
    serde_json::to_string(&WireOut::To { rid, data }).expect("plain JSON")
}

/// A frame from the relay, or `None` for one this build does not know (a newer relay).
fn parse(text: &str) -> Option<FromRelay> {
    match serde_json::from_str::<WireIn>(text).ok()? {
        WireIn::Check { rid, user, instance } => Some(FromRelay::Check { rid, user: user.parse().ok()?, instance }),
        WireIn::Joined { rid, user, name, channel } => Some(FromRelay::Joined { rid, user: user.parse().ok()?, name, channel: channel.parse().ok()? }),
        WireIn::Left { rid } => Some(FromRelay::Left { rid }),
        WireIn::Data { rid, data } => Some(FromRelay::Data { rid, data }),
    }
}

// --- the handle -----------------------------------------------------------------------------

enum Cmd {
    Channels(Vec<u64>),
    Checked { rid: String, answer: Checked },
    /// A frame for the relay as it is (`state` or `to`). Dropped while not connected.
    Frame(String),
}

/// The handle the host keeps. Dropping it stops the task and closes the socket.
#[derive(Clone)]
pub struct Relay {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl Relay {
    /// Starts the task. `url` is the relay's host socket (`wss://<worker>/host/ws`). Nothing
    /// connects until [`Relay::set_channels`] names a channel.
    pub fn spawn(url: String, secret: String, events: mpsc::UnboundedSender<FromRelay>) -> Relay {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(url, secret, rx, events));
        Relay { tx }
    }

    /// The voice channels the bot is in now. Call it on every change; the same list twice
    /// sends nothing. An empty list closes the connection.
    pub fn set_channels(&self, mut channels: Vec<u64>) {
        channels.sort_unstable();
        channels.dedup();
        let _ = self.tx.send(Cmd::Channels(channels));
    }

    /// The answer to a [`FromRelay::Check`].
    pub fn checked(&self, rid: String, answer: Checked) {
        let _ = self.tx.send(Cmd::Checked { rid, answer });
    }

    /// The state, for every person whose call the bot is in (deetsmusic-protocol `Reply`).
    /// The relay keeps the last one for a person who comes in later; `null` forgets it.
    pub fn state(&self, data: &serde_json::Value) {
        let _ = self.tx.send(Cmd::Frame(state_frame(data)));
    }

    /// Something for one person only: a reply, or who they are.
    pub fn to(&self, rid: &str, data: &serde_json::Value) {
        let _ = self.tx.send(Cmd::Frame(to_frame(rid, data)));
    }
}

type Socket = tokio_websockets::WebSocketStream<tokio_websockets::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(url: &str, secret: &str) -> Result<Socket, String> {
    let uri: http::Uri = url.parse().map_err(|e| format!("the relay's address is not a URL: {e}"))?;
    let mut bearer = HeaderValue::from_str(&format!("Bearer {secret}")).map_err(|_| "the relay secret has a character a header cannot carry".to_owned())?;
    bearer.set_sensitive(true);
    let builder = ClientBuilder::from_uri(uri)
        .add_header(AUTHORIZATION, bearer)
        .and_then(|b| b.add_header(HeaderName::from_static("user-agent"), HeaderValue::from_static("lildeets-relay-client")))
        .map_err(|e| e.to_string())?;
    match tokio::time::timeout(CONNECT_TIMEOUT, builder.connect()).await {
        Ok(Ok((ws, _response))) => Ok(ws),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("no answer in 15 s".into()),
    }
}

/// Why a connection ended.
enum End {
    /// The bot left its last call: stay disconnected until a channel is named.
    Idle,
    /// The handle was dropped: the host is stopping.
    Stopped,
    /// The socket failed or closed: connect again.
    Lost(String),
}

async fn run(url: String, secret: String, mut cmds: mpsc::UnboundedReceiver<Cmd>, events: mpsc::UnboundedSender<FromRelay>) {
    let mut channels: Vec<u64> = Vec::new();
    let mut retry = RETRY_MIN;
    info!(target: "lildeets_host", "relay: armed; it connects when the bot is in a call");
    loop {
        // Idle: no call, no socket.
        while channels.is_empty() {
            match cmds.recv().await {
                Some(Cmd::Channels(c)) => channels = c,
                Some(Cmd::Checked { .. } | Cmd::Frame(_)) => {} // for a connection that is gone
                None => return info!(target: "lildeets_host", "relay: stopped"),
            }
        }
        let mut ws = match connect(&url, &secret).await {
            Ok(ws) => ws,
            Err(e) => {
                warn!(target: "lildeets_host", error = %e, retry_s = retry.as_secs(), "relay: could not connect");
                // Wait, but keep reading the channel list: a leave during the wait goes idle.
                let until = Instant::now() + retry;
                retry = (retry * 2).min(RETRY_MAX);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(until) => break,
                        cmd = cmds.recv() => match cmd {
                            Some(Cmd::Channels(c)) => { channels = c; if channels.is_empty() { break; } }
                            Some(Cmd::Checked { .. } | Cmd::Frame(_)) => {}
                            None => return info!(target: "lildeets_host", "relay: stopped"),
                        },
                    }
                }
                continue;
            }
        };
        info!(target: "lildeets_host", channels = channels.len(), "relay: connected");
        retry = RETRY_MIN;
        if events.send(FromRelay::Reset).is_err() {
            return info!(target: "lildeets_host", "relay: stopped");
        }
        let end = serve(&mut ws, &mut channels, &mut cmds, &events).await;
        let _ = events.send(FromRelay::Reset);
        match end {
            End::Idle => {
                let _ = ws.send(Message::close(None, "")).await;
                info!(target: "lildeets_host", "relay: disconnected; the bot is in no call");
            }
            End::Stopped => {
                let _ = ws.send(Message::close(None, "")).await;
                return info!(target: "lildeets_host", "relay: stopped");
            }
            End::Lost(why) => {
                warn!(target: "lildeets_host", error = %why, retry_s = retry.as_secs(), "relay: the connection dropped; connecting again");
                tokio::time::sleep(retry).await;
                retry = (retry * 2).min(RETRY_MAX);
            }
        }
    }
}

async fn serve(ws: &mut Socket, channels: &mut Vec<u64>, cmds: &mut mpsc::UnboundedReceiver<Cmd>, events: &mpsc::UnboundedSender<FromRelay>) -> End {
    if let Err(e) = ws.send(Message::text(channels_frame(channels))).await {
        return End::Lost(e.to_string());
    }
    let mut ping = tokio::time::interval_at(Instant::now() + PING_EVERY, PING_EVERY);
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            frame = ws.next() => match frame {
                Some(Ok(msg)) => {
                    heard = Instant::now();
                    if msg.is_close() {
                        let why = msg.as_close().map(|(code, reason)| format!("closed by the relay: {} {reason}", u16::from(code))).unwrap_or_else(|| "closed by the relay".into());
                        return End::Lost(why);
                    }
                    let Some(text) = msg.as_text() else { continue };
                    if text == "pong" { continue; }
                    match parse(text) {
                        Some(ev) => { if events.send(ev).is_err() { return End::Stopped; } }
                        None => debug!(target: "lildeets_host", "relay: a frame this build does not know; skipped"),
                    }
                }
                Some(Err(e)) => return End::Lost(e.to_string()),
                None => return End::Lost("the socket ended".into()),
            },
            cmd = cmds.recv() => match cmd {
                Some(Cmd::Channels(c)) => {
                    if c == *channels { continue; }
                    *channels = c;
                    if channels.is_empty() { return End::Idle; }
                    if let Err(e) = ws.send(Message::text(channels_frame(channels))).await { return End::Lost(e.to_string()); }
                }
                Some(Cmd::Checked { rid, answer }) => {
                    if let Err(e) = ws.send(Message::text(checked_frame(&rid, &answer))).await { return End::Lost(e.to_string()); }
                }
                Some(Cmd::Frame(text)) => {
                    if let Err(e) = ws.send(Message::text(text)).await { return End::Lost(e.to_string()); }
                }
                None => return End::Stopped,
            },
            _ = ping.tick() => {
                if heard.elapsed() > SILENCE_MAX { return End::Lost("no frame from the relay in 75 s".into()); }
                if let Err(e) = ws.send(Message::text(PING)).await { return End::Lost(e.to_string()); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_the_wire() {
        assert_eq!(channels_frame(&[10, 1234567890123456789]), r#"{"t":"channels","channels":["10","1234567890123456789"]}"#);
        assert_eq!(checked_frame("r1", &Checked::In { channel: 10 }), r#"{"t":"checked","rid":"r1","ok":true,"channel":"10"}"#);
        assert_eq!(checked_frame("r1", &Checked::No { reason: "no".into() }), r#"{"t":"checked","rid":"r1","ok":false,"reason":"no"}"#);
        assert_eq!(state_frame(&serde_json::json!({"kind": "state"})), r#"{"t":"state","data":{"kind":"state"}}"#);
        assert_eq!(state_frame(&serde_json::Value::Null), r#"{"t":"state","data":null}"#);
        assert_eq!(to_frame("r1", &serde_json::json!({"kind": "you", "host": true})), r#"{"t":"to","rid":"r1","data":{"host":true,"kind":"you"}}"#);
    }

    #[test]
    fn a_check_is_read_and_an_unknown_frame_is_skipped() {
        assert_eq!(
            parse(r#"{"t":"check","rid":"abc","user":"200000000000000001","instance":"i-1-gc-2-3"}"#),
            Some(FromRelay::Check { rid: "abc".into(), user: 200000000000000001, instance: "i-1-gc-2-3".into() })
        );
        assert_eq!(
            parse(r#"{"t":"joined","rid":"abc","user":"42","name":"Deets","channel":"10"}"#),
            Some(FromRelay::Joined { rid: "abc".into(), user: 42, name: "Deets".into(), channel: 10 })
        );
        assert_eq!(parse(r#"{"t":"left","rid":"abc"}"#), Some(FromRelay::Left { rid: "abc".into() }));
        assert_eq!(
            parse(r#"{"t":"data","rid":"abc","data":{"q":1,"search":"x"}}"#),
            Some(FromRelay::Data { rid: "abc".into(), data: serde_json::json!({"q": 1, "search": "x"}) })
        );
        assert_eq!(parse(r#"{"t":"later","x":1}"#), None);
        assert_eq!(parse(r#"{"t":"check","rid":"abc","user":"not-a-number","instance":"i"}"#), None);
        assert_eq!(parse("pong"), None);
    }

    /// A fake relay on a local port: checks the bearer, reads the channels, asks one check,
    /// reads the answer, then sees the close when the bot leaves its call.
    #[tokio::test]
    async fn connects_only_in_a_call_and_answers_a_check() {
        use tokio::net::TcpListener;
        use tokio_websockets::ServerBuilder;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let (request, mut ws) = ServerBuilder::new().accept(sock).await.unwrap();
            assert_eq!(request.headers().get("authorization").unwrap(), "Bearer s3cret");
            let first = ws.next().await.unwrap().unwrap();
            assert_eq!(first.as_text().unwrap(), r#"{"t":"channels","channels":["10"]}"#);
            ws.send(Message::text(r#"{"t":"check","rid":"r9","user":"42","instance":"i-9"}"#)).await.unwrap();
            let answer = ws.next().await.unwrap().unwrap();
            assert_eq!(answer.as_text().unwrap(), r#"{"t":"checked","rid":"r9","ok":true,"channel":"10"}"#);
            let to = ws.next().await.unwrap().unwrap();
            assert_eq!(to.as_text().unwrap(), r#"{"t":"to","rid":"r9","data":{"host":false,"kind":"you"}}"#);
            let moved = ws.next().await.unwrap().unwrap();
            assert_eq!(moved.as_text().unwrap(), r#"{"t":"channels","channels":["10","20"]}"#);
            let bye = ws.next().await.unwrap().unwrap();
            assert!(bye.is_close());
        });

        let (tx, mut events) = mpsc::unbounded_channel();
        let relay = Relay::spawn(format!("ws://127.0.0.1:{port}/host/ws"), "s3cret".into(), tx);
        relay.set_channels(vec![10]);
        let first = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap().unwrap();
        assert_eq!(first, FromRelay::Reset); // a new connection: nobody known yet
        let ev = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap().unwrap();
        let FromRelay::Check { rid, user, instance } = ev else { panic!("expected a check, got {ev:?}") };
        assert_eq!((rid.as_str(), user, instance.as_str()), ("r9", 42, "i-9"));
        relay.checked(rid.clone(), Checked::In { channel: 10 });
        relay.to(&rid, &serde_json::json!({"kind": "you", "host": false}));
        relay.set_channels(vec![10]); // the same list: no frame
        relay.set_channels(vec![20, 10]);
        relay.set_channels(vec![]); // the bot left its last call: the socket closes
        tokio::time::timeout(Duration::from_secs(5), server).await.unwrap().unwrap();
    }
}
