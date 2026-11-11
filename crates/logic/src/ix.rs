//! One interaction, read from the INTERACTION_CREATE the host forwards (raw JSON), and the
//! ways to answer it. Read by hand from JSON: the logic needs a dozen fields, and a field
//! Discord adds later can never make an interaction unreadable.

use lildeets_rest::routes::{self, deferred, response, MessageBody};
use lildeets_rest::{File, Rest, RestError};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Map, Value};
use tracing::{info, warn};
use twilight_model::channel::message::component::Component;
use twilight_model::http::interaction::{InteractionResponseData, InteractionResponseType as T};

fn id(v: &Value) -> Option<u64> {
    v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_u64())
}

/// Interaction types (Discord's numbers).
pub const COMMAND: u64 = 2;
pub const COMPONENT: u64 = 3;
pub const AUTOCOMPLETE: u64 = 4;

pub struct Ix {
    pub raw: Value,
    pub id: u64,
    pub app_id: u64,
    pub token: String,
    pub kind: u64,
    pub guild: Option<u64>,
    pub channel: Option<u64>,
    pub channel_name: Option<String>,
    pub user: u64,
    /// discord.py's `display_name`: the server nickname, else the global name, else the username.
    pub display: String,
}

impl Ix {
    pub fn parse(raw: Value) -> Option<Ix> {
        let member = raw.get("member");
        let user = member.and_then(|m| m.get("user")).or_else(|| raw.get("user"))?.clone();
        let s = |v: Option<&Value>| v.and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
        let display = s(member.and_then(|m| m.get("nick")))
            .or_else(|| s(user.get("global_name")))
            .or_else(|| s(user.get("username")))
            .unwrap_or_default();
        Some(Ix {
            id: id(raw.get("id")?)?,
            app_id: id(raw.get("application_id")?)?,
            token: raw.get("token")?.as_str()?.to_owned(),
            kind: raw.get("type")?.as_u64()?,
            guild: raw.get("guild_id").and_then(id),
            channel: raw.get("channel_id").and_then(id).or_else(|| raw.get("channel").and_then(|c| c.get("id")).and_then(id)),
            channel_name: s(raw.get("channel").and_then(|c| c.get("name"))),
            user: id(user.get("id")?)?,
            display,
            raw,
        })
    }

    pub fn data(&self) -> &Value {
        &self.raw["data"]
    }

    /// (command, subcommand, the subcommand's options by name). `/links` has no subcommand.
    pub fn command(&self) -> (String, String, Map<String, Value>) {
        let d = self.data();
        let name = d["name"].as_str().unwrap_or("").to_owned();
        let top = d["options"].as_array().cloned().unwrap_or_default();
        let (sub, opts) = match top.first() {
            Some(o) if o["type"].as_u64() == Some(1) => (o["name"].as_str().unwrap_or("").to_owned(), o["options"].as_array().cloned().unwrap_or_default()),
            _ => (String::new(), top),
        };
        let map = opts.into_iter().filter_map(|o| Some((o["name"].as_str()?.to_owned(), o))).collect();
        (name, sub, map)
    }

    pub fn custom_id(&self) -> &str {
        self.data()["custom_id"].as_str().unwrap_or("")
    }
}

/// An option's value.
pub fn opt<'a>(opts: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    opts.get(name).map(|o| &o["value"])
}

/// The option being typed into (autocomplete): its value as text.
pub fn focused(opts: &Map<String, Value>) -> String {
    opts.values().find(|o| o["focused"].as_bool() == Some(true)).and_then(|o| o["value"].as_str()).unwrap_or("").to_owned()
}

fn log(what: &str, r: Result<Value, RestError>) -> Result<Value, RestError> {
    if let Err(e) = &r {
        warn!(what, error = %e, "interaction: Discord refused an answer");
    }
    r
}

/// The answers, as discord.py makes them.
///
/// **A refused reply is rescinded** (the owner, 2026-11-10: "Delete + private note"). When a
/// public "is thinking…" message is up and the reply that should replace it fails, the
/// thinking message is deleted and only the asker gets a short private note. Without this
/// the channel keeps a "thinking" message for good (two in Moshi's, 2026-11-10).
pub struct Answer<'a> {
    pub rest: &'a Rest,
    pub ix: &'a Ix,
    /// A public "is thinking…" message is up and no reply has replaced it yet.
    thinking: AtomicBool,
}

impl<'a> Answer<'a> {
    pub fn new(rest: &'a Rest, ix: &'a Ix) -> Self {
        Self { rest, ix, thinking: AtomicBool::new(false) }
    }

    /// After a reply that should replace the thinking message: on success it is gone; when
    /// Discord refused it, rescind it once. A network failure is not rescinded: the delete
    /// and the note would fail the same way, and "Discord refused it" would be wrong.
    async fn after_reply(&self, r: Result<Value, RestError>) -> Result<Value, RestError> {
        match &r {
            Ok(_) => self.thinking.store(false, Ordering::Relaxed),
            Err(RestError::Status { .. }) if self.thinking.swap(false, Ordering::Relaxed) => self.rescind().await,
            Err(_) => {}
        }
        r
    }

    async fn rescind(&self) {
        let what = match self.ix.kind {
            COMMAND => {
                let (name, sub, _) = self.ix.command();
                format!("`/{}`", if sub.is_empty() { name } else { format!("{name} {sub}") })
            }
            _ => "that".to_owned(),
        };
        let deleted = self.delete().await.is_ok();
        let note = MessageBody::text(format!("Couldn't post the reply to {what}: Discord refused it."), true);
        let told = log("rescind note", self.rest.send(&routes::followup(self.ix.app_id, &self.ix.token, &note, vec![])).await).is_ok();
        info!(deleted, told, user = self.ix.user, "interaction: a refused reply; rescinded the thinking message");
    }

    async fn callback(&self, kind: T, data: Option<InteractionResponseData>) -> Result<Value, RestError> {
        log("callback", self.rest.send(&routes::interaction_callback(self.ix.id, &self.ix.token, &response(kind, data))).await)
    }

    /// `response.send_message(text, ephemeral=…)`.
    pub async fn text(&self, content: impl Into<String>, ephemeral: bool) -> Result<Value, RestError> {
        self.callback(T::ChannelMessageWithSource, Some(MessageBody::text(content, ephemeral).into_response_data())).await
    }

    /// `response.send_message(view=…, ephemeral=…)`.
    pub async fn card(&self, components: Vec<Component>, ephemeral: bool) -> Result<Value, RestError> {
        self.callback(T::ChannelMessageWithSource, Some(MessageBody::card(components, ephemeral).into_response_data())).await
    }

    /// `response.defer(ephemeral=…, thinking=True)`.
    pub async fn defer(&self, ephemeral: bool) -> Result<Value, RestError> {
        let r = log("defer", self.rest.send(&routes::interaction_callback(self.ix.id, &self.ix.token, &deferred(ephemeral))).await);
        self.thinking.store(r.is_ok() && !ephemeral, Ordering::Relaxed);
        r
    }

    /// A button's `response.defer()`: no visible change yet.
    pub async fn defer_update(&self) -> Result<Value, RestError> {
        self.callback(T::DeferredUpdateMessage, None).await
    }

    /// `response.edit_message(view=…)`: replace the message the button is on.
    pub async fn update(&self, components: Vec<Component>) -> Result<Value, RestError> {
        self.callback(T::UpdateMessage, Some(MessageBody::card(components, false).into_response_data())).await
    }

    /// Autocomplete choices (at most 25).
    pub async fn choices(&self, choices: Vec<(String, String)>) -> Result<Value, RestError> {
        let list: Vec<Value> = choices.into_iter().take(25).map(|(name, value)| json!({"name": name, "value": value})).collect();
        let body = json!({"type": 8, "data": {"choices": list}});
        let req = lildeets_rest::Request {
            body: lildeets_rest::Body::Json(body),
            ..routes::interaction_callback(self.ix.id, &self.ix.token, &response(T::ApplicationCommandAutocompleteResult, None))
        };
        log("autocomplete", self.rest.send(&req).await)
    }

    /// `followup.send(text, ephemeral=…)`.
    pub async fn follow_text(&self, content: impl Into<String>, ephemeral: bool) -> Result<Value, RestError> {
        self.after_reply(log("followup", self.rest.send(&routes::followup(self.ix.app_id, &self.ix.token, &MessageBody::text(content, ephemeral), vec![])).await)).await
    }

    /// `followup.send(view=…, ephemeral=…)`.
    pub async fn follow_card(&self, components: Vec<Component>, ephemeral: bool) -> Result<Value, RestError> {
        self.after_reply(log("followup", self.rest.send(&routes::followup(self.ix.app_id, &self.ix.token, &MessageBody::card(components, ephemeral), vec![])).await)).await
    }

    /// `followup.send(text, files=…, ephemeral=…)`.
    pub async fn follow_files(&self, content: impl Into<String>, files: Vec<File>, ephemeral: bool) -> Result<Value, RestError> {
        self.after_reply(log("followup", self.rest.send(&routes::followup(self.ix.app_id, &self.ix.token, &MessageBody::text(content, ephemeral), files)).await)).await
    }

    /// `edit_original_response(view=…)`.
    pub async fn edit_card(&self, components: Vec<Component>) -> Result<Value, RestError> {
        self.after_reply(log("edit", self.rest.send(&routes::edit_original(self.ix.app_id, &self.ix.token, &MessageBody::card(components, false))).await)).await
    }

    /// `delete_original_response()`.
    pub async fn delete(&self) -> Result<Value, RestError> {
        log("delete", self.rest.send(&routes::delete_original(self.ix.app_id, &self.ix.token)).await)
    }

    /// `original_response()`: (channel id, message id).
    pub async fn original(&self) -> Option<(u64, u64)> {
        let m = self.rest.send(&routes::get_original(self.ix.app_id, &self.ix.token)).await.ok()?;
        Some((id(&m["channel_id"])?, id(&m["id"])?))
    }
}
