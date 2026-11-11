//! One builder per Discord call the bot makes (rust-port-plan.md §5 P3). Pure: no network.
//!
//! The rate-limit [`Route`] of each request is its method and path with the ids replaced by
//! placeholders, plus its "major parameter" (the channel, the guild, or the webhook / the
//! interaction), as Discord groups buckets.

use serde::Serialize;
use serde_json::{json, Value};
use twilight_model::channel::message::component::Component;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// A rate-limit route: `key` names the route ("PATCH /channels/{channel}/messages/{message}"),
/// `major` is its major parameter ("" when it has none).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Route {
    pub key: String,
    pub major: String,
    /// Interaction and webhook-token routes are not under the bot's global limit.
    pub global: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    None,
    Json(Value),
    /// A message with files: `payload_json` plus `files[n]` parts.
    Multipart { payload: Value, files: Vec<File> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    /// Under `/api/v10`, with the query string.
    pub path: String,
    pub route: Route,
    /// Sends `Authorization: Bot <token>`. Off for the interaction-token routes, which carry
    /// their own token in the path and need no bot token.
    pub auth: bool,
    pub body: Body,
}

fn req(method: Method, path: String, key: &str, major: String, auth: bool, body: Body) -> Request {
    Request { method, path, route: Route { key: format!("{} {key}", method.as_str()), major, global: auth }, auth, body }
}

// --- message bodies --------------------------------------------------------------------

/// A message body: a new message, an edit, a follow-up, or an interaction answer's data.
/// Either text (`content`) or a Components V2 card (`components` + `IS_COMPONENTS_V2`, and
/// no `content`, which Discord refuses on a V2 message).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MessageBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<Component>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<MessageFlags>,
}

impl MessageBody {
    /// Plain text; `ephemeral`: only the person who used the command sees it.
    pub fn text(content: impl Into<String>, ephemeral: bool) -> Self {
        Self { content: Some(content.into()), components: None, flags: ephemeral.then_some(MessageFlags::EPHEMERAL) }
    }

    /// A card (`lildeets_cards::cards`).
    pub fn card(components: Vec<Component>, ephemeral: bool) -> Self {
        let mut flags = MessageFlags::IS_COMPONENTS_V2;
        if ephemeral {
            flags |= MessageFlags::EPHEMERAL;
        }
        Self { content: None, components: Some(components), flags: Some(flags) }
    }

    fn json(&self) -> Value {
        serde_json::to_value(self).expect("a message body serializes")
    }

    /// As an interaction answer's `data`.
    pub fn into_response_data(self) -> InteractionResponseData {
        InteractionResponseData { content: self.content, components: self.components, flags: self.flags, ..Default::default() }
    }
}

/// An interaction answer: `kind` with optional data.
pub fn response(kind: InteractionResponseType, data: Option<InteractionResponseData>) -> InteractionResponse {
    InteractionResponse { kind, data }
}

/// "Thinking…", visible to everyone or only to the asker (discord.py `defer(ephemeral=…)`).
pub fn deferred(ephemeral: bool) -> InteractionResponse {
    let data = ephemeral.then(|| InteractionResponseData { flags: Some(MessageFlags::EPHEMERAL), ..Default::default() });
    response(InteractionResponseType::DeferredChannelMessageWithSource, data)
}

// --- routes ----------------------------------------------------------------------------

/// `GET /applications/@me`: the owner and team ids (the host check).
pub fn get_current_application() -> Request {
    req(Method::Get, "/applications/@me".into(), "/applications/@me", String::new(), true, Body::None)
}

/// `GET /applications/{app}/activity-instances/{instance}`: where an Activity runs and who is
/// in it. The caller checks `instance` first: it goes into the path as it is.
pub fn get_activity_instance(app: u64, instance: &str) -> Request {
    req(
        Method::Get,
        format!("/applications/{app}/activity-instances/{instance}"),
        "/applications/{app}/activity-instances/{instance}",
        String::new(),
        true,
        Body::None,
    )
}

/// `GET /applications/{app}/commands`: the global command list as Discord holds it. The sync
/// reads it to keep the Activity's Launch command, which Discord made (commands.rs).
pub fn get_global_commands(app: u64) -> Request {
    req(Method::Get, format!("/applications/{app}/commands"), "/applications/{app}/commands", String::new(), true, Body::None)
}

/// `PUT /applications/{app}/commands`: the whole global command list (the last sync wins).
/// Raw JSON on purpose: the bot registers `tests/parity/expected/commands.json` exactly as
/// the Python bot does; twilight's `Command` would write its own defaults into it.
pub fn put_global_commands(app: u64, commands: Value) -> Request {
    req(Method::Put, format!("/applications/{app}/commands"), "/applications/{app}/commands", String::new(), true, Body::Json(commands))
}

/// `POST /interactions/{id}/{token}/callback`: answer an interaction (in 3 s).
pub fn interaction_callback(id: u64, token: &str, response: &InteractionResponse) -> Request {
    req(
        Method::Post,
        format!("/interactions/{id}/{token}/callback"),
        "/interactions/{id}/{token}/callback",
        id.to_string(),
        false,
        Body::Json(serde_json::to_value(response).expect("a response serializes")),
    )
}

/// `PATCH /webhooks/{app}/{token}/messages/@original`: edit the interaction's answer.
pub fn edit_original(app: u64, token: &str, body: &MessageBody) -> Request {
    req(
        Method::Patch,
        format!("/webhooks/{app}/{token}/messages/@original"),
        "/webhooks/{app}/{token}/messages/@original",
        format!("{app}:{token}"),
        false,
        Body::Json(body.json()),
    )
}

/// `GET /webhooks/{app}/{token}/messages/@original`: the interaction's answer (its message id
/// and channel, for the np card's watcher).
pub fn get_original(app: u64, token: &str) -> Request {
    req(
        Method::Get,
        format!("/webhooks/{app}/{token}/messages/@original"),
        "/webhooks/{app}/{token}/messages/@original",
        format!("{app}:{token}"),
        false,
        Body::None,
    )
}

/// `DELETE /webhooks/{app}/{token}/messages/@original`.
pub fn delete_original(app: u64, token: &str) -> Request {
    req(
        Method::Delete,
        format!("/webhooks/{app}/{token}/messages/@original"),
        "/webhooks/{app}/{token}/messages/@original",
        format!("{app}:{token}"),
        false,
        Body::None,
    )
}

/// `POST /webhooks/{app}/{token}`: a follow-up message (with files: `/links`' fallback).
pub fn followup(app: u64, token: &str, body: &MessageBody, files: Vec<File>) -> Request {
    req(
        Method::Post,
        format!("/webhooks/{app}/{token}"),
        "/webhooks/{app}/{token}",
        format!("{app}:{token}"),
        false,
        message_body(body.json(), files),
    )
}

/// `PATCH /channels/{channel}/messages/{message}`: the np card's edit, with the bot token
/// (the interaction token dies after 15 minutes).
pub fn edit_message(channel: u64, message: u64, body: &MessageBody) -> Request {
    req(
        Method::Patch,
        format!("/channels/{channel}/messages/{message}"),
        "/channels/{channel}/messages/{message}",
        channel.to_string(),
        true,
        Body::Json(body.json()),
    )
}

/// `POST /users/@me/channels {recipient_id}`: open a DM (`/links`).
pub fn create_dm(user: u64) -> Request {
    req(Method::Post, "/users/@me/channels".into(), "/users/@me/channels", String::new(), true, Body::Json(json!({"recipient_id": user.to_string()})))
}

/// `POST /channels/{channel}/messages`, with files as multipart (`/links`).
pub fn create_message(channel: u64, body: &MessageBody, files: Vec<File>) -> Request {
    req(Method::Post, format!("/channels/{channel}/messages"), "/channels/{channel}/messages", channel.to_string(), true, message_body(body.json(), files))
}

/// `GET /channels/{channel}/messages?after=…&limit=…` (1–100).
pub fn channel_messages_after(channel: u64, after: u64, limit: u8) -> Request {
    let limit = limit.clamp(1, 100);
    req(
        Method::Get,
        format!("/channels/{channel}/messages?after={after}&limit={limit}"),
        "/channels/{channel}/messages",
        channel.to_string(),
        true,
        Body::None,
    )
}

/// `GET /channels/{channel}/messages?limit=…[&before=…]` (1–100), newest first.
pub fn channel_messages_before(channel: u64, before: Option<u64>, limit: u8) -> Request {
    let limit = limit.clamp(1, 100);
    let path = match before {
        Some(b) => format!("/channels/{channel}/messages?before={b}&limit={limit}"),
        None => format!("/channels/{channel}/messages?limit={limit}"),
    };
    req(Method::Get, path, "/channels/{channel}/messages", channel.to_string(), true, Body::None)
}

/// `PUT /channels/{channel}/voice-status {status}`: the voice channel's status line
/// (≤ 500 characters; "" clears it). twilight-http has no route for it.
pub fn put_voice_status(channel: u64, status: &str) -> Request {
    req(
        Method::Put,
        format!("/channels/{channel}/voice-status"),
        "/channels/{channel}/voice-status",
        channel.to_string(),
        true,
        Body::Json(json!({"status": status})),
    )
}

/// A message body, as multipart when it carries files. Discord wants each file listed in
/// `attachments` as `{id, filename}`, `id` matching the `files[id]` part.
fn message_body(body: Value, files: Vec<File>) -> Body {
    if files.is_empty() {
        return Body::Json(body);
    }
    let mut payload = body;
    payload["attachments"] = Value::Array(files.iter().enumerate().map(|(i, f)| json!({"id": i, "filename": f.name})).collect());
    Body::Multipart { payload, files }
}

// --- multipart -------------------------------------------------------------------------

/// A boundary no file is likely to contain.
pub fn boundary() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("lildeets-{nanos:x}")
}

impl Body {
    /// The `multipart/form-data` bytes for `boundary` (empty for a non-multipart body).
    pub fn multipart_bytes(&self, boundary: &str) -> Vec<u8> {
        let Body::Multipart { payload, files } = self else { return Vec::new() };
        let mut out = Vec::new();
        out.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n").as_bytes());
        out.extend_from_slice(&serde_json::to_vec(payload).expect("a JSON value serializes"));
        out.extend_from_slice(b"\r\n");
        for (i, f) in files.iter().enumerate() {
            let name = f.name.replace(['"', '\r', '\n'], "_");
            out.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files[{i}]\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n")
                    .as_bytes(),
            );
            out.extend_from_slice(&f.bytes);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        out
    }
}
