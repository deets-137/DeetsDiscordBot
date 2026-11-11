//! The host ⇄ logic protocol over real loopback sockets.

use lildeets_ipc::{accept, new_token, read, write, GuildVoice, IpcError, Sound, ToHost, ToLogic, MAX_FRAME, PROTOCOL};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

async fn pair() -> (TcpStream, TcpStream) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let (client, server) = tokio::join!(TcpStream::connect(addr), l.accept());
    (client.unwrap(), server.unwrap().0)
}

#[tokio::test]
async fn the_handshake_then_messages_both_ways() {
    let token = new_token();
    let (mut logic, mut host) = pair().await;
    write(&mut logic, &ToHost::Hello { token: token.clone(), version: PROTOCOL }).await.unwrap();
    accept(&mut host, &token).await.unwrap();

    let ready = ToLogic::Ready { app_id: 1, bot_user_id: 2, guilds: vec![3, 4] };
    let dispatch = ToLogic::Dispatch { event: json!({"t": "INTERACTION_CREATE", "d": {"id": "9", "token": "abc", "data": {"name": "dm"}}}) };
    let snap = ToLogic::VoiceSnapshot {
        request_id: Some(7),
        guilds: vec![GuildVoice { guild: 3, bot_channel: Some(5), tapping_app: Some("deetsmusic.exe".into()), gain: 1.0, dave_ready: true, members_in_bot_channel: vec![11, 12] }],
    };
    for m in [&ready, &dispatch, &snap] {
        write(&mut host, m).await.unwrap();
    }
    for want in [ready, dispatch, snap] {
        assert_eq!(read::<_, ToLogic>(&mut logic).await.unwrap(), Some(want));
    }

    let play = ToHost::Play { request_id: 8, guild: 3, channel: 5, sound: Sound::App { exe: "deetsmusic.exe".into() } };
    write(&mut logic, &play).await.unwrap();
    assert_eq!(read::<_, ToHost>(&mut host).await.unwrap(), Some(play));

    drop(logic);
    assert_eq!(read::<_, ToHost>(&mut host).await.unwrap(), None, "a close between frames is a clean end");
}

#[tokio::test]
async fn the_wire_format_is_tagged_json() {
    let tone = ToHost::Play { request_id: 1, guild: 2, channel: 3, sound: Sound::Tone };
    assert_eq!(serde_json::to_value(tone).unwrap(), json!({"t": "Play", "request_id": 1, "guild": 2, "channel": 3, "sound": {"kind": "tone"}}));
    let v = serde_json::to_value(ToHost::Presence { listening: None }).unwrap();
    assert_eq!(v, json!({"t": "Presence", "listening": null}));
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (mut logic, mut host) = pair().await;
    write(&mut logic, &ToHost::Hello { token: "0".repeat(64), version: PROTOCOL }).await.unwrap();
    assert!(matches!(accept(&mut host, &new_token()).await, Err(IpcError::Refused("wrong token"))));
}

#[tokio::test]
async fn another_version_is_refused() {
    let token = new_token();
    let (mut logic, mut host) = pair().await;
    write(&mut logic, &ToHost::Hello { token: token.clone(), version: PROTOCOL + 1 }).await.unwrap();
    assert!(matches!(accept(&mut host, &token).await, Err(IpcError::Refused("another protocol version"))));
}

#[tokio::test]
async fn anything_before_hello_is_refused() {
    let (mut logic, mut host) = pair().await;
    write(&mut logic, &ToHost::Snapshot { request_id: 1 }).await.unwrap();
    assert!(matches!(accept(&mut host, "x").await, Err(IpcError::Refused("the first frame was not a Hello"))));
}

#[tokio::test]
async fn an_oversized_frame_is_refused_before_it_is_read() {
    let (mut logic, mut host) = pair().await;
    logic.write_all(&((MAX_FRAME as u32) + 1).to_le_bytes()).await.unwrap();
    assert!(matches!(read::<_, ToHost>(&mut host).await, Err(IpcError::TooLarge(_))));
}

#[test]
fn tokens_are_64_hex_and_differ() {
    let (a, b) = (new_token(), new_token());
    assert_eq!(a.len(), 64);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}
