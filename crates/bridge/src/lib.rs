//! Lil Deets' client for DeetsMusic's loopback bridge: the Rust port of `docs/archive/bot/dm_bridge.py`.
//! architecture/BRIDGE.md is its doc. Read-only toward DeetsMusic's code: it only calls the
//! routes in BRIDGE.md §2, and **only those** (a guest must never reach any other).
//!
//! - Address: `127.0.0.1`, the first of ports 47825–47828 that answers. The port is cached;
//!   when the cached port fails, all four are tried again (DeetsMusic restarted elsewhere).
//! - Auth: `Authorization: Bearer <bridgeToken>` from
//!   `%APPDATA%\com.deetsmusic.app\settings.json`, read on the first call and re-read once
//!   when a call gets 401. The token is never logged and never copied anywhere.
//! - Caches, as the Python bot: search 2 min per term (at most 200 terms), Rooms rules 10 s,
//!   the theme accent 60 s.
//! - Every error is a [`BridgeError`] whose text is fit to show the user (DeetsMusic's own
//!   message when it sends one).
//!
//! One difference from Python, on purpose: a 2xx reply that is not JSON is a `BridgeError`
//! ("can't read"), where Python raised a bare `ValueError` into the command handler.
//!
//! No threads of its own; every call is async (reqwest on the caller's tokio runtime).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use lildeets_cards::model::{NowPlaying, Queue, Track};
use serde_json::{json, Value};
use tokio::time::Instant;
use tracing::debug;

/// The bridge's ports, in the order tried.
pub const PORTS: [u16; 4] = [47825, 47826, 47827, 47828];

/// DeetsMusic › Settings › Rooms: what a guest may do (value `"everyone"`, or host only).
pub const RULE_KEYS: [&str; 5] = [
    "roomGuests.playPause",
    "roomGuests.skip",
    "roomGuests.seek",
    "roomGuests.add",
    "roomGuests.changeQueue",
];

/// The accent per DeetsMusic theme: the theme's "go" light from its `src/styles/themes.css`
/// (moonlight's is near white, so its title blue instead). A new theme needs a row here.
pub const THEME_ACCENT: [(&str, u32); 6] = [
    ("lilac", 0x2FB089),
    ("green", 0x4C9A46),
    ("sepia", 0x6B8E4E),
    ("moonlight", 0xDFECFC),
    ("black-yellow", 0xFFD400),
    ("black-red", 0xFF4D45),
];
pub const DEFAULT_ACCENT: u32 = 0xD4537E;

const SEARCH_TTL: Duration = Duration::from_secs(120);
const SEARCH_MAX: usize = 200;
const RULES_TTL: Duration = Duration::from_secs(10);
const ACCENT_TTL: Duration = Duration::from_secs(60);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const SETTINGS_TIMEOUT: Duration = Duration::from_secs(5);

/// DeetsMusic's look: the ids it sets as `data-theme` and `data-skin`, and the skin settings
/// the Activity draws with (`skin_look`: only the keys in `SKIN_LOOK_KEYS`, ACTIVITY.md §14.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Appearance {
    pub theme: String,
    pub skin: String,
    pub skin_look: serde_json::Map<String, Value>,
}

/// The `GET /settings` rows that change how the Activity draws the host's skin (Settings › Skin
/// settings, and two Motion rows). Only these ever leave the PC; the keys and values are
/// DeetsMusic's own (deetsmusic-protocol `SkinLook`).
pub const SKIN_LOOK_KEYS: &[&str] = &[
    "fancyScrubber", "backgroundMotion", "albumWave",
    "oceanEdges", "oceanSand", "oceanCardOpacity", "oceanLight",
    "pressLight", "cyberLight",
    "glassFancy", "glassTint", "glassBacklight", "glassCanvasGlow", "glassCanvasDim", "glassCanvas", "glassTiles", "glassDiffusion", "glassAuroraColor",
    "pressVinyl", "pressVinylWhere", "pressVinylPlate", "pressVinylSpeed", "pressVinylFps",
    "metalMaterial", "metalGrain", "metalCoverChange", "metalPeek",
    "happySky", "happyCardOpacity",
];

/// A failure worth showing the user as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeError(pub String);

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BridgeError {}

fn not_running() -> BridgeError {
    BridgeError("DeetsMusic isn't running.".into())
}

fn unreadable() -> BridgeError {
    BridgeError("DeetsMusic sent a reply Lil Deets can't read.".into())
}

/// Where to find DeetsMusic. [`Config::default`] is the real one; tests point it elsewhere.
#[derive(Clone, Debug)]
pub struct Config {
    /// DeetsMusic's settings file (holds `bridgeToken`).
    pub settings: PathBuf,
    pub ports: Vec<u16>,
}

impl Default for Config {
    fn default() -> Self {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
        Self { settings: appdata.join("com.deetsmusic.app").join("settings.json"), ports: PORTS.to_vec() }
    }
}

#[derive(Default)]
struct SearchCache {
    hits: HashMap<String, (Instant, Vec<Track>)>,
    /// First-inserted first. Refreshing a term keeps its place, as a Python dict does.
    order: VecDeque<String>,
}

#[derive(Default)]
struct State {
    port: Option<u16>,
    token: Option<String>,
    rules: Option<(Instant, HashMap<String, Option<String>>)>,
    /// DeetsMusic's theme and skin ids, and when they were read.
    look: Option<(Instant, Appearance)>,
    search: SearchCache,
}

/// What one request to one port came to.
enum Outcome {
    Ok(Value),
    /// An HTTP error status, with DeetsMusic's `error` text if it sent one.
    Status(u16, Option<String>),
    /// No answer on this port (refused, reset, timed out): try the next.
    NoAnswer,
}

pub struct Bridge {
    http: reqwest::Client,
    config: Config,
    state: Mutex<State>,
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Bridge {
    pub fn new(config: Config) -> Self {
        // Loopback only: no proxy may ever see the bearer token.
        let http = reqwest::Client::builder().no_proxy().build().expect("reqwest client");
        Self { http, config, state: Mutex::new(State::default()) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // --- transport ---------------------------------------------------------------------

    async fn read_token(&self) -> Result<String, BridgeError> {
        let token = tokio::fs::read(&self.config.settings)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|v| v.get("bridgeToken").and_then(Value::as_str).map(str::to_owned))
            .filter(|t| !t.is_empty())
            .ok_or_else(|| BridgeError("DeetsMusic isn't set up on this PC (no bridge token).".into()))?;
        self.state().token = Some(token.clone());
        Ok(token)
    }

    async fn request(&self, port: u16, token: &str, path: &str, body: Option<&Value>, timeout: Duration) -> Result<Outcome, BridgeError> {
        let url = format!("http://127.0.0.1:{port}{path}");
        let builder = match body {
            Some(b) => self.http.post(url).body(serde_json::to_vec(b).expect("a JSON value serializes")),
            None => self.http.get(url),
        };
        let reply = builder
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(timeout)
            .send()
            .await;
        let reply = match reply {
            Ok(r) => r,
            Err(_) => return Ok(Outcome::NoAnswer),
        };
        let status = reply.status();
        let Ok(bytes) = reply.bytes().await else { return Ok(Outcome::NoAnswer) };
        if status.is_client_error() || status.is_server_error() {
            let msg = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
                .filter(|m| !m.is_empty());
            return Ok(Outcome::Status(status.as_u16(), msg));
        }
        if bytes.is_empty() {
            return Ok(Outcome::Ok(json!({})));
        }
        serde_json::from_slice(&bytes).map(Outcome::Ok).map_err(|_| unreadable())
    }

    /// One call: `GET path` (no body) or `POST path` with a JSON body.
    pub async fn call(&self, path: &str, body: Option<&Value>, timeout: Duration) -> Result<Value, BridgeError> {
        let cached = self.state().token.clone();
        let mut token = match cached {
            Some(t) => t,
            None => self.read_token().await?,
        };
        let cached_port = self.state().port;
        let mut ports = cached_port.map(|p| vec![p]).unwrap_or_else(|| self.config.ports.clone());
        for attempt in 0..2 {
            let mut reread = false;
            for &port in &ports {
                match self.request(port, &token, path, body, timeout).await? {
                    Outcome::Ok(v) => {
                        let mut st = self.state();
                        if st.port != Some(port) {
                            debug!(port, "bridge: DeetsMusic answers on this port");
                        }
                        st.port = Some(port);
                        return Ok(v);
                    }
                    Outcome::Status(401, _) if attempt == 0 => {
                        debug!("bridge: 401; reading the bridge token again");
                        token = self.read_token().await?;
                        reread = true;
                        break;
                    }
                    Outcome::Status(code, msg) => return Err(BridgeError(msg.unwrap_or_else(|| format!("DeetsMusic said {code}.")))),
                    Outcome::NoAnswer => continue,
                }
            }
            if !reread {
                let had_port = self.state().port.is_some();
                if had_port && attempt == 0 {
                    debug!("bridge: the cached port stopped answering; trying all four");
                    self.state().port = None;
                    ports = self.config.ports.clone();
                    continue;
                }
                return Err(not_running());
            }
        }
        Err(not_running())
    }

    fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, BridgeError> {
        serde_json::from_value(v).map_err(|_| unreadable())
    }

    // --- reads ---------------------------------------------------------------------------

    pub async fn now_playing(&self) -> Result<NowPlaying, BridgeError> {
        Self::parse(self.call("/now-playing", None, DEFAULT_TIMEOUT).await?)
    }

    pub async fn queue(&self) -> Result<Queue, BridgeError> {
        Self::parse(self.call("/queue", None, DEFAULT_TIMEOUT).await?)
    }

    /// Songs for a term, cached 2 minutes per term (autocomplete asks on every keystroke).
    /// The autocomplete passes a short `timeout` (its 2.5 s budget).
    pub async fn search_songs(&self, term: &str, timeout: Option<Duration>) -> Result<Vec<Track>, BridgeError> {
        let key = term.trim().to_lowercase();
        if let Some((at, songs)) = self.state().search.hits.get(&key) {
            if at.elapsed() < SEARCH_TTL {
                return Ok(songs.clone());
            }
        }
        let out = self.call("/search", Some(&json!({"term": term, "types": ["songs"]})), timeout.unwrap_or(DEFAULT_TIMEOUT)).await?;
        let songs: Vec<Track> = match out.get("songs") {
            Some(Value::Null) | None => Vec::new(),
            Some(v) => Self::parse(v.clone())?,
        };
        let mut st = self.state();
        let cache = &mut st.search;
        if cache.hits.insert(key.clone(), (Instant::now(), songs.clone())).is_none() {
            cache.order.push_back(key);
        }
        if cache.hits.len() > SEARCH_MAX {
            if let Some(oldest) = cache.order.pop_front() {
                cache.hits.remove(&oldest);
            }
        }
        Ok(songs)
    }

    /// The Rooms guest rules, cached 10 s. Unknown or unreadable → empty (host only).
    /// An empty answer is not cached, so the next check asks again.
    pub async fn rules(&self) -> HashMap<String, Option<String>> {
        if let Some((at, rules)) = &self.state().rules {
            if at.elapsed() < RULES_TTL && !rules.is_empty() {
                return rules.clone();
            }
        }
        let rules = match self.call("/settings?section=rooms", None, SETTINGS_TIMEOUT).await {
            Ok(out) => settings_rows(&out)
                .filter(|(k, _)| RULE_KEYS.contains(k))
                .map(|(k, v)| (k.to_owned(), v.and_then(Value::as_str).map(str::to_owned)))
                .collect(),
            Err(_) => HashMap::new(),
        };
        self.state().rules = Some((Instant::now(), rules.clone()));
        rules
    }

    /// The cards' accent: DeetsMusic's current theme colour, cached 60 s. On an error the
    /// last colour stays (the default before any answer).
    pub async fn accent(&self) -> u32 {
        theme_accent(&self.appearance().await.theme)
    }

    /// DeetsMusic's theme and skin ids (`GET /settings`, rows `theme` and `skin`), cached
    /// 60 s with the accent. On an error the last answer stays (empty ids before any).
    pub async fn appearance(&self) -> Appearance {
        if let Some((at, look)) = &self.state().look {
            if at.elapsed() < ACCENT_TTL {
                return look.clone();
            }
        }
        let mut look = self.state().look.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
        if let Ok(out) = self.call("/settings", None, SETTINGS_TIMEOUT).await {
            let row = |key: &str| settings_rows(&out).find(|(k, _)| *k == key).and_then(|(_, v)| v.and_then(Value::as_str)).unwrap_or("").to_owned();
            let skin_look = settings_rows(&out)
                .filter(|(k, _)| SKIN_LOOK_KEYS.contains(k))
                // A plain value only: a toggle, a number, or a short choice id.
                .filter_map(|(k, v)| v.filter(|v| v.is_boolean() || v.is_number() || v.as_str().is_some_and(|s| s.len() <= 32)).map(|v| (k.to_owned(), v.clone())))
                .collect();
            look = Appearance { theme: row("theme"), skin: row("skin"), skin_look };
        }
        self.state().look = Some((Instant::now(), look.clone()));
        look
    }

    // --- writes --------------------------------------------------------------------------

    /// `POST /command {kind[, value]}`: `play`, `pause`, `next`, `previous`, and `seek` with a
    /// value from 0 to 1 (a fraction of the song).
    pub async fn command(&self, kind: &str, value: Option<Value>) -> Result<Value, BridgeError> {
        let mut body = json!({"kind": kind});
        if let Some(v) = value {
            body["value"] = v;
        }
        self.call("/command", Some(&body), DEFAULT_TIMEOUT).await
    }

    /// `POST /queue {id, mode: next | later}`. `song_id` is `song:<catalogId>`.
    pub async fn enqueue(&self, song_id: &str, next_up: bool) -> Result<Value, BridgeError> {
        self.call("/queue", Some(&json!({"id": song_id, "mode": if next_up { "next" } else { "later" }})), DEFAULT_TIMEOUT).await
    }

    /// `POST /play {id[, keepQueue]}`: play the song now. Without `keep_queue` DeetsMusic
    /// replaces Up Next too; with it the song goes in at the top of Up Next and plays, and the
    /// rest of Up Next stays (DeetsMusic's `playTracksKeepQueue`).
    pub async fn play(&self, song_id: &str, keep_queue: bool) -> Result<Value, BridgeError> {
        let mut body = json!({"id": song_id});
        if keep_queue {
            body["keepQueue"] = json!(true);
        }
        self.call("/play", Some(&body), DEFAULT_TIMEOUT).await
    }

    /// `POST /queue/edit {action: remove | move, index[, to]}`.
    pub async fn queue_edit(&self, action: &str, index: i64, to: Option<i64>) -> Result<Value, BridgeError> {
        let mut body = json!({"action": action, "index": index});
        if let Some(to) = to {
            body["to"] = json!(to);
        }
        self.call("/queue/edit", Some(&body), DEFAULT_TIMEOUT).await
    }
}

/// The accent for a theme id (`DEFAULT_ACCENT` for one this table does not know).
pub fn theme_accent(theme: &str) -> u32 {
    THEME_ACCENT.iter().find(|(id, _)| *id == theme).map_or(DEFAULT_ACCENT, |(_, c)| *c)
}

/// `settings: [{key, value}]` rows of a `/settings` reply, as (key, value).
fn settings_rows(out: &Value) -> impl Iterator<Item = (&str, Option<&Value>)> {
    out.get("settings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r.get("key")?.as_str()?, r.get("value"))))
}
