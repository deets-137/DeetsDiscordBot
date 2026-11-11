//! P1 (cards): every file in expected/cards/, rebuilt with twilight-model 0.18 types from
//! the same inputs as scripts/parity/render_expected.py.

mod support;

use lildeets_cards::cards::{self, count, MAX_COMPONENTS};
use lildeets_cards::model::{NowPlaying, Queue, Track};
use serde_json::Value;

/// The kit's theme is moonlight; its accent is DeetsMusic's title blue (BRIDGE.md §4).
const ACCENT: u32 = 0xDFECFC;

fn json(c: &[twilight_model::channel::message::component::Component]) -> Value {
    serde_json::to_value(c).unwrap()
}

fn queue() -> Queue {
    serde_json::from_value(support::input("queue.json")).unwrap()
}

fn now_playing() -> NowPlaying {
    serde_json::from_value(support::input("now_playing.json")).unwrap()
}

fn now() -> f64 {
    support::frozen_now() as f64
}

#[test]
fn the_kit_is_moonlight() {
    let theme = support::input("settings_theme.json");
    let rows = theme["settings"].as_array().unwrap();
    let value = rows.iter().find(|r| r["key"] == "theme").unwrap()["value"].as_str().unwrap();
    assert_eq!(value, "moonlight");
}

#[test]
fn np_playing_paused_nothing() {
    let q = queue();
    let np = now_playing();
    let up = q.upcoming.first();
    support::assert_card(json(&cards::np_card(&np, up, None, ACCENT, now())), "cards/np_playing.json");
    let paused = NowPlaying { playing: false, ..np };
    support::assert_card(json(&cards::np_card(&paused, up, None, ACCENT, now())), "cards/np_paused.json");
    let nothing = NowPlaying { active: false, ..NowPlaying::default() };
    support::assert_card(json(&cards::np_card(&nothing, None, None, ACCENT, now())), "cards/np_nothing.json");
}

#[test]
fn queue_pages_in_layout_c() {
    let q = queue();
    for page in [1, 2, 3, 9] {
        let card = cards::queue_card(&q, page, ACCENT);
        assert!(count(&card) <= MAX_COMPONENTS, "page {page}: {} components", count(&card));
        support::assert_card(json(&card), &format!("cards/queue_page{page}.json"));
    }
    let empty = Queue::default();
    support::assert_card(json(&cards::queue_card(&empty, 1, ACCENT)), "cards/queue_empty.json");
}

#[test]
fn a_full_queue_page_is_38_components() {
    // CARDS.md §1: 38 with 10 rows; Discord's limit is 40.
    assert_eq!(count(&cards::queue_card(&queue(), 1, ACCENT)), 38);
}

#[test]
fn picker() {
    let songs: Vec<Track> = serde_json::from_value(support::input("search_humble.json")["songs"].clone()).unwrap();
    for next_up in [false, true] {
        let card = cards::picker_card("humble", &songs, next_up, ACCENT);
        support::assert_card(json(&card), &format!("cards/picker_humble_next{}.json", u8::from(next_up)));
    }
}

#[test]
fn custom_ids_route_without_state() {
    let v = json(&cards::picker_card("humble", &serde_json::from_value::<Vec<Track>>(support::input("search_humble.json")["songs"].clone()).unwrap(), true, ACCENT));
    let ids: Vec<&str> = v[0]["components"].as_array().unwrap().last().unwrap()["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["custom_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], "dm:pick:1:1:1440882165");
    let q = json(&cards::queue_card(&queue(), 2, ACCENT));
    let nav: Vec<&str> = q[0]["components"].as_array().unwrap().last().unwrap()["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["custom_id"].as_str().unwrap())
        .collect();
    assert_eq!(nav, ["dm:queue:1:prev", "dm:queue:3:next"]);
}

/// A one-page queue: both targets are page 1, and the ids must still differ (Discord
/// refuses a duplicated custom_id with a 400; seen in Moshi's, 2026-11-10).
#[test]
fn queue_nav_ids_are_unique_on_one_page() {
    let mut q = queue();
    q.upcoming.truncate(3);
    let card = json(&cards::queue_card(&q, 1, ACCENT));
    let nav: Vec<&str> = card[0]["components"].as_array().unwrap().last().unwrap()["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["custom_id"].as_str().unwrap())
        .collect();
    assert_eq!(nav, ["dm:queue:1:prev", "dm:queue:1:next"]);
}
