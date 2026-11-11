//! P4: the bridge client against stand-in DeetsMusic servers on loopback (no DeetsMusic,
//! no network). Each test gets its own settings file and its own ports.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use deetsmusic_bridge::{Bridge, BridgeError, Config, DEFAULT_ACCENT};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Debug)]
struct Req {
    method: String,
    path: String,
    auth: Option<String>,
    content_type: Option<String>,
    body: Option<Value>,
}

type Handler = Arc<dyn Fn(&Req) -> (u16, String) + Send + Sync>;

/// A stand-in bridge: answers every request with `handler` and records it.
struct Stub {
    port: u16,
    log: Arc<Mutex<Vec<Req>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Stub {
    async fn start(handler: impl Fn(&Req) -> (u16, String) + Send + Sync + 'static) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let task = tokio::spawn({
            let log = log.clone();
            async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else { return };
                    let (log, handler) = (log.clone(), handler.clone());
                    tokio::spawn(async move {
                        let Some(req) = read_request(&mut sock).await else { return };
                        log.lock().unwrap().push(req.clone());
                        let (code, body) = handler(&req);
                        let head = format!(
                            "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = sock.write_all(head.as_bytes()).await;
                        let _ = sock.write_all(body.as_bytes()).await;
                        let _ = sock.shutdown().await;
                    });
                }
            }
        });
        Stub { port, log, task }
    }

    fn requests(&self) -> Vec<Req> {
        self.log.lock().unwrap().clone()
    }

    /// Stops answering: the port refuses from now on.
    fn stop(&self) {
        self.task.abort();
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Req> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let header = |name: &str| {
        head.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_owned()))
    };
    let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    while buf.len() < head_end + len {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = (len > 0).then(|| serde_json::from_slice(&buf[head_end..head_end + len]).unwrap());
    Some(Req { method, path, auth: header("authorization"), content_type: header("content-type"), body })
}

/// A port nobody listens on.
fn dead_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// A fresh settings file with this token (or none).
fn settings(name: &str, token: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("deetsmusic-bridge-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    let body = match token {
        Some(t) => json!({"bridgeToken": t, "theme": "irrelevant"}),
        None => json!({"theme": "x"}),
    };
    std::fs::write(&path, body.to_string()).unwrap();
    path
}

fn bridge(name: &str, token: Option<&str>, ports: Vec<u16>) -> Bridge {
    Bridge::new(Config { settings: settings(name, token), ports })
}

fn ok(body: Value) -> (u16, String) {
    (200, body.to_string())
}

const T: Duration = Duration::from_secs(5);

#[tokio::test]
async fn finds_the_first_port_that_answers_and_keeps_it() {
    let a = Stub::start(|_| ok(json!({"n": 1}))).await;
    let b = Stub::start(|_| ok(json!({"n": 2}))).await;
    let br = bridge("first", Some("tok"), vec![dead_port(), a.port, b.port]);
    assert_eq!(br.call("/now-playing", None, T).await.unwrap(), json!({"n": 1}));
    assert_eq!(br.call("/now-playing", None, T).await.unwrap(), json!({"n": 1}));
    assert_eq!(a.requests().len(), 2);
    assert!(b.requests().is_empty(), "the cached port is used alone");
    let r = &a.requests()[0];
    assert_eq!((r.method.as_str(), r.path.as_str()), ("GET", "/now-playing"));
    assert_eq!(r.auth.as_deref(), Some("Bearer tok"));
    assert_eq!(r.content_type.as_deref(), Some("application/json"));
    assert_eq!(r.body, None);
}

#[tokio::test]
async fn fails_over_when_the_cached_port_stops_answering() {
    let a = Stub::start(|_| ok(json!({"from": "a"}))).await;
    let b = Stub::start(|_| ok(json!({"from": "b"}))).await;
    let br = bridge("failover", Some("tok"), vec![a.port, b.port]);
    assert_eq!(br.call("/queue", None, T).await.unwrap()["from"], "a");
    a.stop();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(br.call("/queue", None, T).await.unwrap()["from"], "b");
    assert_eq!(br.call("/queue", None, T).await.unwrap()["from"], "b");
    assert_eq!(b.requests().len(), 2);
}

#[tokio::test]
async fn nobody_answers() {
    let br = bridge("none", Some("tok"), vec![dead_port(), dead_port()]);
    assert_eq!(br.call("/queue", None, T).await, Err(BridgeError("DeetsMusic isn't running.".into())));
}

#[tokio::test]
async fn no_token_means_not_set_up() {
    let a = Stub::start(|_| ok(json!({}))).await;
    let br = bridge("notoken", None, vec![a.port]);
    assert_eq!(br.call("/queue", None, T).await, Err(BridgeError("DeetsMusic isn't set up on this PC (no bridge token).".into())));
    assert!(a.requests().is_empty(), "nothing is sent without a token");
}

#[tokio::test]
async fn a_401_reads_the_token_again_once() {
    let path = settings("401", Some("old"));
    let p = path.clone();
    let a = Stub::start(move |r| match r.auth.as_deref() {
        Some("Bearer old") => {
            // DeetsMusic rotated its token: the settings file now holds the new one.
            std::fs::write(&p, json!({"bridgeToken": "new"}).to_string()).unwrap();
            (401, json!({"error": "bad token"}).to_string())
        }
        Some("Bearer new") => ok(json!({"ok": true})),
        _ => (400, String::new()),
    })
    .await;
    let br = Bridge::new(Config { settings: path, ports: vec![a.port] });
    assert_eq!(br.call("/queue", None, T).await.unwrap(), json!({"ok": true}));
    let auths: Vec<_> = a.requests().into_iter().map(|r| r.auth.unwrap()).collect();
    assert_eq!(auths, ["Bearer old", "Bearer new"]);
}

#[tokio::test]
async fn a_second_401_is_an_error_with_deetsmusics_message() {
    let a = Stub::start(|_| (401, json!({"error": "Bridge token rejected."}).to_string())).await;
    let br = bridge("401twice", Some("tok"), vec![a.port]);
    assert_eq!(br.call("/queue", None, T).await, Err(BridgeError("Bridge token rejected.".into())));
    assert_eq!(a.requests().len(), 2);
}

#[tokio::test]
async fn an_error_status_shows_deetsmusics_own_message() {
    let a = Stub::start(|r| match r.path.as_str() {
        "/play" => (404, json!({"error": "That song isn't in the catalog."}).to_string()),
        _ => (500, "not json".into()),
    })
    .await;
    let br = bridge("status", Some("tok"), vec![a.port]);
    assert_eq!(br.play("song:1").await, Err(BridgeError("That song isn't in the catalog.".into())));
    assert_eq!(br.call("/queue", None, T).await, Err(BridgeError("DeetsMusic said 500.".into())));
}

#[tokio::test]
async fn a_reply_that_is_not_json_is_an_error() {
    let a = Stub::start(|_| (200, "<html>".into())).await;
    let br = bridge("notjson", Some("tok"), vec![a.port]);
    assert_eq!(br.call("/queue", None, T).await, Err(BridgeError("DeetsMusic sent a reply Lil Deets can't read.".into())));
}

#[tokio::test]
async fn an_empty_reply_is_an_empty_object() {
    let a = Stub::start(|_| (200, String::new())).await;
    let br = bridge("empty", Some("tok"), vec![a.port]);
    assert_eq!(br.command("pause", None).await.unwrap(), json!({}));
}

#[tokio::test]
async fn reads_parse_into_the_models() {
    let kit = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/input");
    let np: Value = serde_json::from_slice(&std::fs::read(kit.join("now_playing.json")).unwrap()).unwrap();
    let q: Value = serde_json::from_slice(&std::fs::read(kit.join("queue.json")).unwrap()).unwrap();
    let (np2, q2) = (np.clone(), q.clone());
    let a = Stub::start(move |r| match r.path.as_str() {
        "/now-playing" => ok(np2.clone()),
        "/queue" => ok(q2.clone()),
        _ => (404, String::new()),
    })
    .await;
    let br = bridge("models", Some("tok"), vec![a.port]);
    let now = br.now_playing().await.unwrap();
    assert!(now.playing && now.active);
    assert_eq!(now.title.as_deref(), Some("Pink + White"));
    assert_eq!(now.current_time, Some(58.0));
    let queue = br.queue().await.unwrap();
    assert_eq!(queue.upcoming.len(), q["upcoming"].as_array().unwrap().len());
    assert_eq!(queue.current.unwrap().catalog_id.as_deref(), Some("1146195714"));
}

#[tokio::test]
async fn search_is_cached_per_term() {
    let a = Stub::start(|r| ok(json!({"songs": [{"catalogId": "1", "title": r.body.as_ref().unwrap()["term"]}]}))).await;
    let br = bridge("search", Some("tok"), vec![a.port]);
    let first = br.search_songs("Humble ", None).await.unwrap();
    let again = br.search_songs("  humble", None).await.unwrap();
    assert_eq!(first, again, "the same term, any case or spaces, is one request");
    br.search_songs("dna", None).await.unwrap();
    let reqs = a.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/search");
    assert_eq!(reqs[0].body, Some(json!({"term": "Humble ", "types": ["songs"]})), "the term goes as typed");
}

#[tokio::test]
async fn search_with_no_songs_key_is_empty() {
    let a = Stub::start(|_| ok(json!({}))).await;
    let br = bridge("search-empty", Some("tok"), vec![a.port]);
    assert!(br.search_songs("zzzz", None).await.unwrap().is_empty());
}

#[tokio::test]
async fn rules_keep_only_the_room_keys_and_cache_a_real_answer() {
    let a = Stub::start(|_| {
        ok(json!({"settings": [
            {"key": "roomGuests.skip", "value": "everyone"},
            {"key": "roomGuests.add", "value": "host"},
            {"key": "roomGuests.seek"},
            {"key": "theme", "value": "lilac"}
        ]}))
    })
    .await;
    let br = bridge("rules", Some("tok"), vec![a.port]);
    let rules = br.rules().await;
    assert_eq!(rules.len(), 3);
    assert_eq!(rules["roomGuests.skip"].as_deref(), Some("everyone"));
    assert_eq!(rules["roomGuests.seek"], None);
    br.rules().await;
    assert_eq!(a.requests().len(), 1, "cached for 10 s");
    assert_eq!(a.requests()[0].path, "/settings?section=rooms");
}

#[tokio::test]
async fn rules_that_fail_are_host_only_and_asked_again() {
    let br = bridge("rules-fail", Some("tok"), vec![dead_port()]);
    assert!(br.rules().await.is_empty());
    let a = Stub::start(|_| ok(json!({"settings": []}))).await;
    let br = bridge("rules-empty", Some("tok"), vec![a.port]);
    br.rules().await;
    br.rules().await;
    assert_eq!(a.requests().len(), 2, "an empty answer is not cached");
}

#[tokio::test]
async fn accent_follows_the_theme_and_falls_back() {
    let a = Stub::start(|_| ok(json!({"settings": [{"key": "theme", "value": "moonlight"}]}))).await;
    let br = bridge("accent", Some("tok"), vec![a.port]);
    assert_eq!(br.accent().await, 0xDFECFC);
    assert_eq!(br.accent().await, 0xDFECFC);
    assert_eq!(a.requests().len(), 1, "cached for 60 s");

    let b = Stub::start(|_| ok(json!({"settings": [{"key": "theme", "value": "a-new-theme"}]}))).await;
    assert_eq!(bridge("accent-new", Some("tok"), vec![b.port]).accent().await, DEFAULT_ACCENT);
    assert_eq!(bridge("accent-down", Some("tok"), vec![dead_port()]).accent().await, DEFAULT_ACCENT);
}

#[tokio::test]
async fn writes_send_the_documented_bodies() {
    let a = Stub::start(|_| ok(json!({"ok": true, "notices": [{"kind": "warn", "text": "Queue is long"}]}))).await;
    let br = bridge("writes", Some("tok"), vec![a.port]);
    let reply = br.command("next", None).await.unwrap();
    assert_eq!(lildeets_cards::text::notices(&reply), "\n-# ⚠ Queue is long");
    br.command("volume", Some(json!(0.5))).await.unwrap();
    br.enqueue("song:42", true).await.unwrap();
    br.enqueue("song:42", false).await.unwrap();
    br.play("song:7").await.unwrap();
    br.queue_edit("remove", 3, None).await.unwrap();
    br.queue_edit("move", 3, Some(1)).await.unwrap();
    let sent: Vec<(String, String, Value)> = a.requests().into_iter().map(|r| (r.method, r.path, r.body.unwrap())).collect();
    let want = [
        ("/command", json!({"kind": "next"})),
        ("/command", json!({"kind": "volume", "value": 0.5})),
        ("/queue", json!({"id": "song:42", "mode": "next"})),
        ("/queue", json!({"id": "song:42", "mode": "later"})),
        ("/play", json!({"id": "song:7"})),
        ("/queue/edit", json!({"action": "remove", "index": 3})),
        ("/queue/edit", json!({"action": "move", "index": 3, "to": 1})),
    ];
    assert_eq!(sent.len(), want.len());
    for ((method, path, body), (wpath, wbody)) in sent.iter().zip(want) {
        assert_eq!((method.as_str(), path.as_str(), body), ("POST", wpath, &wbody));
    }
}

#[tokio::test]
async fn a_slow_port_counts_as_no_answer() {
    // A listener that accepts and never replies: the call times out and moves on.
    let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    let _hold = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = silent.accept().await {
            held.push(s);
        }
    });
    let b = Stub::start(|_| ok(json!({"from": "b"}))).await;
    let br = bridge("slow", Some("tok"), vec![silent_port, b.port]);
    assert_eq!(br.call("/queue", None, Duration::from_millis(300)).await.unwrap()["from"], "b");
}
