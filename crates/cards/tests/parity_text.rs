//! P1 (text half): every case in expected/cases.json and every file in expected/text/,
//! rebuilt from the same inputs as scripts/parity/render_expected.py.

mod support;

use std::collections::HashMap;

use lildeets_cards::model::{NowPlaying, Queue, Track};
use lildeets_cards::text::{self, Placement};
use serde_json::Value;

fn queue() -> Queue {
    serde_json::from_value(support::input("queue.json")).unwrap()
}

fn now_playing() -> NowPlaying {
    serde_json::from_value(support::input("now_playing.json")).unwrap()
}

/// Every search input, by its term.
fn searches() -> HashMap<String, Vec<Track>> {
    let dir = support::parity_dir().join("input");
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.starts_with("search_") && name.ends_with(".json") {
            let v = support::input(&name);
            out.insert(v["term"].as_str().unwrap().to_owned(), serde_json::from_value(v["songs"].clone()).unwrap());
        }
    }
    out
}

fn upcoming_by_id(id: &str) -> Track {
    queue().upcoming.into_iter().find(|t| t.catalog_id.as_deref() == Some(id)).unwrap_or_else(|| panic!("no {id} in queue.json"))
}

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

#[test]
fn mmss() {
    for c in support::cases("mmss") {
        assert_eq!(text::mmss(c["seconds"].as_f64()), str_of(&c["text"]), "mmss({})", c["seconds"]);
    }
}

#[test]
fn clip() {
    for c in support::cases("clip") {
        assert_eq!(text::clip(str_of(&c["text"]), c["n"].as_u64().unwrap() as usize), str_of(&c["out"]));
    }
}

#[test]
fn bar() {
    for c in support::cases("bar") {
        assert_eq!(text::bar(c["progress"].as_f64().unwrap()), str_of(&c["bar"]), "bar({})", c["progress"]);
    }
}

#[test]
fn rating_word() {
    for c in support::cases("rating_word") {
        let t = Track { content_rating: c["contentRating"].as_str().map(str::to_owned), ..Track::default() };
        assert_eq!(text::rating_word(&t), str_of(&c["word"]));
    }
}

#[test]
fn column_width() {
    for c in support::cases("column_width") {
        let tracks: Vec<Track> = c["durations_ms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| Track { duration_ms: d.as_f64(), ..Track::default() })
            .collect();
        assert_eq!(text::column_width(&tracks), c["width"].as_u64().unwrap() as usize);
    }
}

#[test]
fn tag_alone() {
    for c in support::cases("tag_alone") {
        assert_eq!(text::tag(&upcoming_by_id(str_of(&c["catalogId"])), None), str_of(&c["tag"]));
    }
}

#[test]
fn tag_column() {
    for c in support::cases("tag_column") {
        let w = c["width"].as_u64().unwrap() as usize;
        assert_eq!(text::tag(&upcoming_by_id(str_of(&c["catalogId"])), Some(w)), str_of(&c["tag"]));
    }
}

#[test]
fn confident() {
    let pools = searches();
    let cases = support::cases("confident");
    assert!(!cases.is_empty());
    for c in cases {
        let songs = &pools[str_of(&c["results_of"])];
        let term = str_of(&c["term"]);
        assert_eq!(text::confident(term, songs), c["confident"].as_bool().unwrap(), "confident({term:?}) over {}", c["results_of"]);
    }
}

#[test]
fn suggestions() {
    let dir = support::parity_dir().join("input");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_prefix("search_").and_then(|n| n.strip_suffix(".json")) else { continue };
        let songs: Vec<Track> = serde_json::from_value(support::input(&name)["songs"].clone()).unwrap();
        let rows: Vec<&Track> = songs.iter().take(25).filter(|t| t.catalog_id.is_some()).collect();
        let owned: Vec<Track> = rows.iter().map(|t| (*t).clone()).collect();
        let w = text::column_width(&owned);
        let out = rows.iter().map(|t| text::suggestion(t, w)).collect::<Vec<_>>().join("\n") + "\n";
        support::assert_bytes(out.as_bytes(), &format!("text/suggestions_{stem}.txt"));
        seen += 1;
    }
    assert_eq!(seen, 5);
}

#[test]
fn help() {
    let host_only = |a: &str| matches!(a, "play" | "volume" | "preview");
    support::assert_bytes((text::help_text(|_| true, true, true) + "\n").as_bytes(), "text/help_host.txt");
    support::assert_bytes((text::help_text(|a| !host_only(a), false, true) + "\n").as_bytes(), "text/help_guest_in_call.txt");
    support::assert_bytes((text::help_text(|_| false, false, false) + "\n").as_bytes(), "text/help_guest_no_stream.txt");
}

#[test]
fn added_lines() {
    let q = queue();
    let playing = now_playing();
    let paused = NowPlaying { playing: false, ..playing.clone() };
    let now = support::frozen_now() as f64;
    for (index, next_up, name) in [(3, false, "later"), (0, true, "next"), (5, false, "long")] {
        let t = q.upcoming[index].clone();
        let id = format!("song:{}", t.catalog_id.as_deref().unwrap());
        for (np, state) in [(&playing, "playing"), (&paused, "paused")] {
            let placement = Placement { upcoming: &q.upcoming, now_playing: np };
            let out = text::added_line("Deets", std::slice::from_ref(&t), next_up, &id, Some(placement), now) + "\n";
            support::assert_bytes(out.as_bytes(), &format!("text/added_{name}_{state}.txt"));
        }
    }
}

#[test]
fn added_line_without_the_bridge_ends_after_the_song() {
    let t = queue().upcoming[0].clone();
    let out = text::added_line("Deets", std::slice::from_ref(&t), false, "song:1", None, 0.0);
    assert_eq!(out, "➕ Deets added **White (feat. Frank Ocean) — Odd Future** `2:03 | Explicit`.");
}

#[test]
fn every_text_file_is_covered() {
    // A new file in expected/text/ must get a test here.
    let dir = support::parity_dir().join("expected/text");
    let names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    for n in &names {
        let known = n.starts_with("suggestions_") || n.starts_with("help_") || n.starts_with("added_");
        assert!(known, "expected/text/{n} has no Rust test");
    }
    assert_eq!(names.len(), 5 + 3 + 6);
}
