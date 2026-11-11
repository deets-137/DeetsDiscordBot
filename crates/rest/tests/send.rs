//! P3: `Rest::send` against a stand-in Discord on loopback: the headers it sends, a 429
//! retried, errors, and `/links`' oldest-first paging.

use std::sync::{Arc, Mutex};

use lildeets_rest::{routes, Rest, RestError, UA};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Debug)]
struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Req {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// (status, extra headers, body)
type Reply = (u16, Vec<(&'static str, String)>, String);

async fn stub(handler: impl Fn(&Req, usize) -> Reply + Send + Sync + 'static) -> (String, Arc<Mutex<Vec<Req>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let log = Arc::new(Mutex::new(Vec::<Req>::new()));
    let handler = Arc::new(handler);
    let l = log.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let (l, handler) = (l.clone(), handler.clone());
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let end = loop {
                    let n = sock.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                let mut lines = head.lines();
                let mut first = lines.next().unwrap().split(' ');
                let (method, path) = (first.next().unwrap().to_owned(), first.next().unwrap().to_owned());
                let headers: Vec<(String, String)> =
                    lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())).collect();
                let len: usize = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).map_or(0, |(_, v)| v.parse().unwrap());
                while buf.len() < end + len {
                    let n = sock.read(&mut chunk).await.unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                }
                let req = Req { method, path, headers, body: buf[end..end + len].to_vec() };
                let n = {
                    let mut log = l.lock().unwrap();
                    log.push(req.clone());
                    log.len()
                };
                let (status, extra, body) = handler(&req, n);
                let mut head = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
                if !body.is_empty() {
                    head.push_str("Content-Type: application/json\r\n");
                }
                for (k, v) in extra {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                sock.write_all(head.as_bytes()).await.unwrap();
                sock.write_all(body.as_bytes()).await.unwrap();
                let _ = sock.shutdown().await;
            });
        }
    });
    (base, log)
}

#[tokio::test]
async fn sends_the_bot_token_only_where_it_belongs() {
    let (base, log) = stub(|_, _| (200, vec![], json!({"id": "1"}).to_string())).await;
    let rest = Rest::with_base("secret-token", &base);
    rest.send(&routes::create_dm(77)).await.unwrap();
    rest.send(&routes::interaction_callback(9, "itok", &routes::deferred(false))).await.unwrap();
    let reqs = log.lock().unwrap().clone();
    assert_eq!(reqs[0].header("authorization"), Some("Bot secret-token"));
    assert_eq!(reqs[0].header("user-agent"), Some(UA));
    assert_eq!(reqs[0].header("content-type"), Some("application/json"));
    assert_eq!(serde_json::from_slice::<Value>(&reqs[0].body).unwrap(), json!({"recipient_id": "77"}));
    assert_eq!((reqs[1].method.as_str(), reqs[1].path.as_str()), ("POST", "/interactions/9/itok/callback"));
    assert_eq!(reqs[1].header("authorization"), None, "an interaction route carries its own token");
}

#[tokio::test]
async fn an_empty_reply_is_null_and_files_go_as_multipart() {
    let (base, log) = stub(|_, _| (204, vec![], String::new())).await;
    let rest = Rest::with_base("t", &base);
    let files = vec![routes::File { name: "a.csv".into(), bytes: b"x,y\r\n".to_vec() }];
    assert_eq!(rest.send(&routes::create_message(5, &lildeets_rest::MessageBody::text("hi", false), files)).await.unwrap(), Value::Null);
    let req = log.lock().unwrap()[0].clone();
    let ct = req.header("content-type").unwrap().to_owned();
    let boundary = ct.strip_prefix("multipart/form-data; boundary=").expect("multipart");
    let body = String::from_utf8(req.body).unwrap();
    assert!(body.starts_with(&format!("--{boundary}\r\n")));
    assert!(body.ends_with(&format!("--{boundary}--\r\n")));
    assert!(body.contains("x,y\r\n"), "the file bytes go as they are");
}

#[tokio::test]
async fn a_429_is_waited_out_and_retried() {
    let (base, log) = stub(|_, n| {
        if n == 1 {
            (429, vec![("X-RateLimit-Scope", "user".into())], json!({"message": "You are being rate limited.", "retry_after": 0.05, "global": false}).to_string())
        } else {
            (200, vec![("X-RateLimit-Bucket", "vs".into()), ("X-RateLimit-Remaining", "4".into()), ("X-RateLimit-Reset-After", "5".into())], "{}".into())
        }
    })
    .await;
    let rest = Rest::with_base("t", &base);
    let started = std::time::Instant::now();
    rest.send(&routes::put_voice_status(5, "♪")).await.unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(50));
    assert_eq!(log.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn owners_are_the_team_or_the_owner() {
    // Trimmed from Discord's documented GET /applications/@me reply.
    let user = |id: &str| json!({"id": id, "username": "u", "discriminator": "0", "global_name": null, "avatar": null});
    let team = json!({
        "id": "9", "name": "Deets", "description": "", "icon": null, "bot_public": false, "bot_require_code_grant": false,
        "verify_key": "k", "flags": 0, "owner": user("1"),
        "team": {"id": "8", "name": "t", "icon": null, "owner_user_id": "2", "members": [
            {"membership_state": 2, "permissions": ["*"], "team_id": "8", "user": user("2"), "role": "admin"},
            {"membership_state": 2, "permissions": ["*"], "team_id": "8", "user": user("3"), "role": "developer"}
        ]}
    });
    let mut solo = team.clone();
    solo["team"] = Value::Null;
    let replies = Arc::new(Mutex::new(vec![solo.to_string(), team.to_string()]));
    let r = replies.clone();
    let (base, _) = stub(move |_, _| (200, vec![], r.lock().unwrap().pop().unwrap())).await;
    let rest = Rest::with_base("t", &base);
    assert_eq!(rest.owners().await.unwrap(), [2, 3], "a team: its members");
    assert_eq!(rest.owners().await.unwrap(), [1], "no team: the owner");
}

#[tokio::test]
async fn dm_channel_reads_the_channel_id() {
    let (base, log) = stub(|_, _| (200, vec![], json!({"id": "555", "type": 1, "recipients": []}).to_string())).await;
    let rest = Rest::with_base("t", &base);
    assert_eq!(rest.dm_channel(77).await.unwrap(), 555);
    assert_eq!(log.lock().unwrap()[0].path, "/users/@me/channels");
}

#[tokio::test]
async fn three_429s_give_up() {
    let (base, log) = stub(|_, _| (429, vec![], json!({"retry_after": 0.01}).to_string())).await;
    let rest = Rest::with_base("t", &base);
    assert!(matches!(rest.send(&routes::create_dm(1)).await, Err(RestError::RateLimited { .. })));
    assert_eq!(log.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn an_error_status_keeps_discords_body() {
    let (base, _) = stub(|_, _| (403, vec![], json!({"code": 50007, "message": "Cannot send messages to this user"}).to_string())).await;
    let rest = Rest::with_base("t", &base);
    match rest.send(&routes::create_message(5, &lildeets_rest::MessageBody::text("x", false), vec![])).await {
        Err(RestError::Status { status: 403, body }) => assert_eq!(body["code"], 50007),
        other => panic!("{other:?}"),
    }
}

/// A channel of 250 messages, ids 1..=250, answering `after` / `before` / `limit` as
/// Discord does: each page newest first.
fn channel_of_250(r: &Req) -> Reply {
    let q: Vec<(String, u64)> = r
        .path
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (k.to_owned(), v.parse().unwrap())
        })
        .collect();
    let get = |k: &str| q.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
    let limit = get("limit").unwrap() as usize;
    let mut page: Vec<u64> = match (get("after"), get("before")) {
        (Some(after), _) => (after + 1..=250).take(limit).collect(),
        (None, before) => (1..before.unwrap_or(251)).rev().take(limit).collect::<Vec<_>>().into_iter().rev().collect(),
    };
    page.reverse();
    (200, vec![], Value::Array(page.into_iter().map(|id| json!({"id": id.to_string()})).collect()).to_string())
}

fn ids(v: Vec<Value>) -> Vec<u64> {
    v.into_iter().map(|m| m["id"].as_str().unwrap().parse().unwrap()).collect()
}

#[tokio::test]
async fn history_without_a_limit_is_everything_oldest_first() {
    let (base, log) = stub(|r, _| channel_of_250(r)).await;
    let rest = Rest::with_base("t", &base);
    assert_eq!(ids(rest.history(5, None).await.unwrap()), (1..=250).collect::<Vec<_>>());
    let paths: Vec<String> = log.lock().unwrap().iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, ["/channels/5/messages?after=0&limit=100", "/channels/5/messages?after=100&limit=100", "/channels/5/messages?after=200&limit=100"]);
}

#[tokio::test]
async fn history_with_a_limit_is_the_newest_listed_oldest_first() {
    // The owner, 2026-11-10: `/links limit:N` reads the last N messages.
    let (base, log) = stub(|r, _| channel_of_250(r)).await;
    let rest = Rest::with_base("t", &base);
    assert_eq!(ids(rest.history(5, Some(150)).await.unwrap()), (101..=250).collect::<Vec<_>>());
    let paths: Vec<String> = log.lock().unwrap().iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, ["/channels/5/messages?limit=100", "/channels/5/messages?before=151&limit=50"]);

    log.lock().unwrap().clear();
    assert_eq!(ids(rest.history(5, Some(3)).await.unwrap()), [248, 249, 250]);
    assert_eq!(ids(rest.history(5, Some(1000)).await.unwrap()), (1..=250).collect::<Vec<_>>(), "a limit past the start is everything");
}
