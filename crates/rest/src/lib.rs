//! Lil Deets' Discord REST client, hand-rolled (rust-rewrite.md §3: exactly our routes, the
//! voice-status route twilight-http lacks, one rate-limit layer).
//!
//! - [`routes`]: one pure builder per call the bot makes. Each returns a [`Request`]: method,
//!   path, the rate-limit route, and the body. Unit-tested with no network.
//! - [`ratelimit`]: per-route buckets from `X-RateLimit-*` headers, the global limit, and 429s.
//!   Pure: the clock is passed in, so recorded headers drive the tests.
//! - [`Rest`]: sends a `Request` (waiting on the limiter first, retrying after a 429).
//!
//! Bodies are twilight-model 0.18 types: `InteractionResponse` for answers, [`MessageBody`]
//! (content / components / flags) for messages. The one raw JSON body is the command list,
//! which is registered exactly as the parity kit holds it.
//!
//! No threads of its own; every call is async on the caller's tokio runtime.

pub mod ratelimit;
pub mod routes;

use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::Value;
use tracing::{debug, warn};

pub use ratelimit::RateLimiter;
pub use routes::{Body, File, MessageBody, Method, Request, Route};

pub const API: &str = "https://discord.com/api/v10";
/// Discord asks every bot for `DiscordBot ($url, $version)`.
pub const UA: &str = concat!("DiscordBot (https://github.com/deets-137/DeetsDiscordBot, ", env!("CARGO_PKG_VERSION"), ")");
/// Tries per request: the first, and up to two after a 429.
const TRIES: usize = 3;

#[derive(Debug)]
pub enum RestError {
    /// Discord answered with an error status. `body` is its JSON (`{code, message, errors}`).
    Status { status: u16, body: Value },
    /// Still rate limited after every try.
    RateLimited { retry_after: Duration },
    /// No answer (connection, TLS, timeout).
    Transport(String),
    /// A 2xx reply that is not JSON.
    /// A 2xx reply that is not JSON, or not the shape we read it as (serde's message).
    Unreadable(String),
}

impl std::fmt::Display for RestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestError::Status { status, body } => {
                write!(f, "Discord said {status}: {}", body.get("message").and_then(Value::as_str).unwrap_or("(no message)"))
            }
            RestError::RateLimited { retry_after } => write!(f, "still rate limited ({retry_after:?})"),
            RestError::Transport(e) => write!(f, "no answer from Discord: {e}"),
            RestError::Unreadable(e) => write!(f, "Discord sent a reply we can't read: {e}"),
        }
    }
}

impl std::error::Error for RestError {}

pub struct Rest {
    http: reqwest::Client,
    /// `Bot <token>`. Never logged.
    auth: HeaderValue,
    base: String,
    limiter: Mutex<RateLimiter>,
}

impl Rest {
    pub fn new(token: &str) -> Self {
        Self::with_base(token, API)
    }

    /// For tests: a stand-in server instead of discord.com.
    pub fn with_base(token: &str, base: &str) -> Self {
        let mut auth = HeaderValue::from_str(&format!("Bot {token}")).expect("a token is header-safe");
        auth.set_sensitive(true);
        Self {
            http: reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("reqwest client"),
            auth,
            base: base.trim_end_matches('/').to_owned(),
            limiter: Mutex::new(RateLimiter::default()),
        }
    }

    fn limiter(&self) -> std::sync::MutexGuard<'_, RateLimiter> {
        self.limiter.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends `req`. Returns the reply's JSON, or `Value::Null` for an empty reply (204).
    pub async fn send(&self, req: &Request) -> Result<Value, RestError> {
        let mut last_wait = Duration::ZERO;
        for attempt in 0..TRIES {
            loop {
                let wait = self.limiter().acquire(&req.route, std::time::Instant::now());
                match wait {
                    Some(d) => {
                        debug!(route = %req.route.key, wait_ms = d.as_millis() as u64, "rest: waiting for the rate limit");
                        tokio::time::sleep(d).await;
                    }
                    None => break,
                }
            }
            let reply = self.build(req).send().await.map_err(|e| RestError::Transport(e.without_url().to_string()))?;
            let status = reply.status().as_u16();
            let headers = reply.headers().clone();
            let bytes = reply.bytes().await.map_err(|e| RestError::Transport(e.without_url().to_string()))?;
            let now = std::time::Instant::now();
            if status == 429 {
                let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                last_wait = self.limiter().on_429(&req.route, &headers, &body, now);
                warn!(route = %req.route.key, attempt, retry_ms = last_wait.as_millis() as u64, "rest: 429");
                tokio::time::sleep(last_wait).await;
                continue;
            }
            self.limiter().record(&req.route, &headers, now);
            if !(200..300).contains(&status) {
                return Err(RestError::Status { status, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) });
            }
            if bytes.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_slice(&bytes).map_err(|e| RestError::Unreadable(e.to_string()));
        }
        Err(RestError::RateLimited { retry_after: last_wait })
    }

    /// [`Rest::send`], with the reply read as `T` (a twilight-model type).
    pub async fn send_as<T: serde::de::DeserializeOwned>(&self, req: &Request) -> Result<T, RestError> {
        serde_json::from_value(self.send(req).await?).map_err(|e| RestError::Unreadable(e.to_string()))
    }

    /// Who may start `/vc` (with `VOICE_USER_IDS`): the application's team members, or its
    /// owner when it has no team (`bot/voice.py` `is_host`).
    pub async fn owners(&self) -> Result<Vec<u64>, RestError> {
        let app: twilight_model::oauth::Application = self.send_as(&routes::get_current_application()).await?;
        Ok(match (app.team, app.owner) {
            (Some(team), _) => team.members.iter().map(|m| m.user.id.get()).collect(),
            (None, Some(owner)) => vec![owner.id.get()],
            (None, None) => Vec::new(),
        })
    }

    /// Open (or find) the DM channel with `user`: its id (`/links`).
    pub async fn dm_channel(&self, user: u64) -> Result<u64, RestError> {
        let channel: twilight_model::channel::Channel = self.send_as(&routes::create_dm(user)).await?;
        Ok(channel.id.get())
    }

    fn build(&self, req: &Request) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base, req.path);
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        if req.auth {
            headers.insert(AUTHORIZATION, self.auth.clone());
        }
        let mut builder = self.http.request(method, url);
        match &req.body {
            Body::None => {}
            Body::Json(v) => {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                builder = builder.body(serde_json::to_vec(v).expect("a JSON value serializes"));
            }
            Body::Multipart { .. } => {
                let boundary = routes::boundary();
                headers.insert(CONTENT_TYPE, HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap());
                builder = builder.body(req.body.multipart_bytes(&boundary));
            }
        }
        builder.headers(headers)
    }

    /// `/links`: a channel's messages, listed oldest first. With no `limit`, all of them;
    /// with a `limit`, the **newest** `limit` (the owner, 2026-11-10; the Python bot's
    /// `collect_links(..., newest=True)`). 100 a page.
    pub async fn history(&self, channel: u64, limit: Option<usize>) -> Result<Vec<Value>, RestError> {
        let Some(limit) = limit else { return self.history_from_first(channel).await };
        let mut out: Vec<Value> = Vec::new();
        let mut before = None;
        while out.len() < limit {
            let want = (limit - out.len()).min(100);
            let page = self.send(&routes::channel_messages_before(channel, before, want as u8)).await?;
            let page = page.as_array().cloned().unwrap_or_default();
            let got = page.len();
            before = page.iter().filter_map(message_id).min();
            out.extend(page);
            if got < want || before.is_none() {
                break;
            }
        }
        out.sort_by_key(|m| message_id(m).unwrap_or(0));
        Ok(out)
    }

    /// Every message from the channel's first one forward (discord.py's
    /// `history(limit=None, oldest_first=True)`), oldest first.
    async fn history_from_first(&self, channel: u64) -> Result<Vec<Value>, RestError> {
        const PAGE: usize = 100;
        let mut out = Vec::new();
        let mut after = 0u64;
        loop {
            let page = self.send(&routes::channel_messages_after(channel, after, PAGE as u8)).await?;
            let mut page = page.as_array().cloned().unwrap_or_default();
            let got = page.len();
            // Discord returns a page newest first; oldest first is the reverse.
            page.sort_by_key(|m| message_id(m).unwrap_or(0));
            if let Some(last) = page.last().and_then(message_id) {
                after = last;
            }
            out.extend(page);
            if got < PAGE {
                break;
            }
        }
        Ok(out)
    }
}

/// A message's snowflake id (Discord sends ids as strings).
fn message_id(m: &Value) -> Option<u64> {
    m.get("id").and_then(Value::as_str).and_then(|s| s.parse().ok())
}
