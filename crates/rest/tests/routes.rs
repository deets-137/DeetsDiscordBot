//! P3: every request the bot makes, built with no network; the rate limiter driven by
//! recorded headers.

use std::time::{Duration, Instant};

use lildeets_rest::routes;
use lildeets_rest::{Body, File, MessageBody, Method, RateLimiter, Request};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;
use twilight_model::http::interaction::InteractionResponseType;

const COMPONENTS_V2: u64 = 1 << 15;

fn check(r: &Request, method: Method, path: &str, route: &str, major: &str, auth: bool) {
    assert_eq!(r.method, method, "{path}");
    assert_eq!(r.path, path);
    assert_eq!(r.route.key, format!("{} {route}", method.as_str()));
    assert_eq!(r.route.major, major, "{path}: major parameter");
    assert_eq!(r.auth, auth, "{path}: bot token");
    assert_eq!(r.route.global, auth, "{path}: under the global limit");
}

#[test]
fn application_and_commands() {
    check(&routes::get_current_application(), Method::Get, "/applications/@me", "/applications/@me", "", true);
    let r = routes::put_global_commands(42, json!([{"name": "vc"}]));
    check(&r, Method::Put, "/applications/42/commands", "/applications/{app}/commands", "", true);
    assert_eq!(r.body, Body::Json(json!([{"name": "vc"}])));
}

#[test]
fn interaction_callbacks_carry_their_type() {
    use InteractionResponseType as T;
    for (kind, n) in [(T::ChannelMessageWithSource, 4), (T::DeferredChannelMessageWithSource, 5), (T::DeferredUpdateMessage, 6), (T::UpdateMessage, 7), (T::ApplicationCommandAutocompleteResult, 8)] {
        let r = routes::interaction_callback(9, "tok", &routes::response(kind, None));
        check(&r, Method::Post, "/interactions/9/tok/callback", "/interactions/{id}/{token}/callback", "9", false);
        assert_eq!(r.body, Body::Json(json!({"type": n})));
    }
    let r = routes::interaction_callback(9, "tok", &routes::deferred(true));
    assert_eq!(r.body, Body::Json(json!({"type": 5, "data": {"flags": 64}})));
    assert_eq!(routes::interaction_callback(9, "tok", &routes::deferred(false)).body, Body::Json(json!({"type": 5})));
    let answer = routes::response(T::ChannelMessageWithSource, Some(MessageBody::text("Only the host can do that.", true).into_response_data()));
    assert_eq!(routes::interaction_callback(9, "tok", &answer).body, Body::Json(json!({"type": 4, "data": {"content": "Only the host can do that.", "flags": 64}})));
}

#[test]
fn interaction_token_routes() {
    let card = MessageBody::card(vec![], false);
    let r = routes::edit_original(1, "t", &card);
    check(&r, Method::Patch, "/webhooks/1/t/messages/@original", "/webhooks/{app}/{token}/messages/@original", "1:t", false);
    assert_eq!(r.body, Body::Json(json!({"components": [], "flags": COMPONENTS_V2})));
    check(&routes::delete_original(1, "t"), Method::Delete, "/webhooks/1/t/messages/@original", "/webhooks/{app}/{token}/messages/@original", "1:t", false);
    let r = routes::followup(1, "t", &MessageBody::text("hi", true), vec![]);
    check(&r, Method::Post, "/webhooks/1/t", "/webhooks/{app}/{token}", "1:t", false);
    assert_eq!(r.body, Body::Json(json!({"content": "hi", "flags": 64})));
}

#[test]
fn channel_routes() {
    let r = routes::edit_message(5, 6, &MessageBody::card(vec![], false));
    check(&r, Method::Patch, "/channels/5/messages/6", "/channels/{channel}/messages/{message}", "5", true);
    let r = routes::create_dm(77);
    check(&r, Method::Post, "/users/@me/channels", "/users/@me/channels", "", true);
    assert_eq!(r.body, Body::Json(json!({"recipient_id": "77"})));
    let r = routes::channel_messages_after(5, 0, 100);
    check(&r, Method::Get, "/channels/5/messages?after=0&limit=100", "/channels/{channel}/messages", "5", true);
    assert_eq!(routes::channel_messages_after(5, 9, 0).path, "/channels/5/messages?after=9&limit=1");
    assert_eq!(routes::channel_messages_after(5, 9, 250).path, "/channels/5/messages?after=9&limit=100");
    let r = routes::channel_messages_before(5, None, 100);
    check(&r, Method::Get, "/channels/5/messages?limit=100", "/channels/{channel}/messages", "5", true);
    assert_eq!(routes::channel_messages_before(5, Some(42), 7).path, "/channels/5/messages?before=42&limit=7");
    let r = routes::put_voice_status(5, "♪ Pink + White — Frank Ocean");
    check(&r, Method::Put, "/channels/5/voice-status", "/channels/{channel}/voice-status", "5", true);
    assert_eq!(r.body, Body::Json(json!({"status": "♪ Pink + White — Frank Ocean"})));
}

#[test]
fn a_card_has_the_v2_flag_and_no_content() {
    let card = lildeets_cards::cards::np_card(&Default::default(), None, None, 0xDFECFC, 0.0);
    let v = serde_json::to_value(MessageBody::card(card, false)).unwrap();
    assert_eq!(v["flags"], json!(COMPONENTS_V2));
    assert!(v.get("content").is_none());
    assert_eq!(v["components"][0]["type"], json!(17), "a container");
    assert_eq!(serde_json::to_value(MessageBody::card(vec![], true)).unwrap()["flags"], json!(32768 + 64));
}

#[test]
fn a_message_with_files_is_multipart_with_attachments_listed() {
    let files = vec![
        File { name: "song-of-the-day.md".into(), bytes: b"# links\r\n".to_vec() },
        File { name: "song-of-the-day.csv".into(), bytes: b"a,b\r\n".to_vec() },
    ];
    let r = routes::create_message(5, &MessageBody::text("Found **2** links", false), files);
    check(&r, Method::Post, "/channels/5/messages", "/channels/{channel}/messages", "5", true);
    let bytes = r.body.multipart_bytes("B");
    let want = concat!(
        "--B\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n",
        r#"{"attachments":[{"filename":"song-of-the-day.md","id":0},{"filename":"song-of-the-day.csv","id":1}],"content":"Found **2** links"}"#,
        "\r\n",
        "--B\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"song-of-the-day.md\"\r\nContent-Type: application/octet-stream\r\n\r\n# links\r\n\r\n",
        "--B\r\nContent-Disposition: form-data; name=\"files[1]\"; filename=\"song-of-the-day.csv\"\r\nContent-Type: application/octet-stream\r\n\r\na,b\r\n\r\n",
        "--B--\r\n",
    );
    assert_eq!(String::from_utf8(bytes).unwrap(), want);
    // The ephemeral fallback (followup with files) is multipart too.
    let r = routes::followup(1, "t", &MessageBody::text("x", true), vec![File { name: "a\".md".into(), bytes: vec![] }]);
    assert!(String::from_utf8(r.body.multipart_bytes("B")).unwrap().contains("filename=\"a_.md\""), "a quote cannot break the header");
}

// --- the rate limiter ----------------------------------------------------------------------

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.insert(HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
    }
    h
}

#[test]
fn an_empty_bucket_waits_for_its_reset() {
    let mut rl = RateLimiter::default();
    let t0 = Instant::now();
    let edit = routes::edit_message(5, 6, &MessageBody::default()).route;
    assert_eq!(rl.acquire(&edit, t0), None, "unknown bucket: go");
    // Recorded from a real message edit: 5 per 5 s per channel.
    rl.record(&edit, &headers(&[("x-ratelimit-bucket", "abc"), ("x-ratelimit-limit", "5"), ("x-ratelimit-remaining", "1"), ("x-ratelimit-reset-after", "4.2")]), t0);
    assert_eq!(rl.acquire(&edit, t0), None, "one left: go, and it is taken");
    assert_eq!(rl.acquire(&edit, t0), Some(Duration::from_secs_f64(4.2)));
    let later = t0 + Duration::from_millis(4300);
    assert_eq!(rl.acquire(&edit, later), None, "after the reset: go");
}

#[test]
fn buckets_are_separate_per_channel_and_shared_by_hash() {
    let mut rl = RateLimiter::default();
    let t0 = Instant::now();
    let in5 = routes::edit_message(5, 6, &MessageBody::default()).route;
    let in7 = routes::edit_message(7, 8, &MessageBody::default()).route;
    let empty = headers(&[("x-ratelimit-bucket", "abc"), ("x-ratelimit-remaining", "0"), ("x-ratelimit-reset-after", "2")]);
    rl.record(&in5, &empty, t0);
    assert!(rl.acquire(&in5, t0).is_some());
    assert_eq!(rl.acquire(&in7, t0), None, "another channel has its own bucket");
}

#[test]
fn a_bucket_429_waits_retry_after() {
    let mut rl = RateLimiter::default();
    let t0 = Instant::now();
    let r = routes::put_voice_status(5, "x").route;
    let wait = rl.on_429(&r, &headers(&[("x-ratelimit-bucket", "vs"), ("x-ratelimit-scope", "user")]), &json!({"message": "You are being rate limited.", "retry_after": 1.5, "global": false}), t0);
    assert_eq!(wait, Duration::from_secs_f64(1.5));
    assert_eq!(rl.acquire(&r, t0 + Duration::from_millis(500)), Some(Duration::from_secs(1)));
    assert_eq!(rl.acquire(&routes::create_dm(1).route, t0), None, "other routes go on");
}

#[test]
fn a_global_429_stops_bot_routes_but_not_interaction_routes() {
    let mut rl = RateLimiter::default();
    let t0 = Instant::now();
    let r = routes::create_dm(1).route;
    rl.on_429(&r, &headers(&[("x-ratelimit-global", "true"), ("retry-after", "3")]), &json!({"retry_after": 2.0, "global": true}), t0);
    assert_eq!(rl.acquire(&routes::edit_message(5, 6, &MessageBody::default()).route, t0), Some(Duration::from_secs(2)));
    assert_eq!(rl.acquire(&routes::followup(1, "t", &MessageBody::default(), vec![]).route, t0), None, "interaction tokens are exempt");
    assert_eq!(rl.acquire(&routes::interaction_callback(1, "t", &routes::deferred(false)).route, t0), None);
    assert_eq!(rl.acquire(&r, t0 + Duration::from_secs(2)), None, "over after retry_after");
}

#[test]
fn retry_after_falls_back_to_the_header() {
    let mut rl = RateLimiter::default();
    let wait = rl.on_429(&routes::create_dm(1).route, &headers(&[("retry-after", "0.25")]), &json!(null), Instant::now());
    assert_eq!(wait, Duration::from_millis(250));
}
