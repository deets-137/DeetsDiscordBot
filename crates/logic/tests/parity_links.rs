//! P6 (the /links part): records, Markdown, CSV and file names, byte for byte against
//! tests/parity/expected/links/ (made by scripts/parity/render_expected.py `render_links`).

mod support;

use lildeets_logic::links::{self, Attachment, Embed, LinkRecord, Message};
use serde_json::{json, Value};

/// The kit's made-up messages, as `render_links`' `to_message` builds them.
fn messages() -> Vec<Message> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    support::input("links_messages.json")
        .as_array()
        .unwrap()
        .iter()
        .map(|m| Message {
            id: m["id"].as_u64().unwrap(),
            content: s(m, "content").unwrap_or_default(),
            jump_url: s(m, "jump_url").unwrap(),
            // isoformat() of an aware UTC datetime prints back the same text.
            timestamp: s(m, "created_at").unwrap(),
            author: s(m, "author").unwrap(),
            author_id: m["author_id"].as_u64().unwrap(),
            embeds: m["embeds"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| Embed { url: s(e, "url"), title: s(e, "title"), image: s(e, "image"), thumbnail: s(e, "thumbnail") })
                .collect(),
            attachments: m["attachments"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| Attachment { url: s(a, "url").unwrap(), filename: s(a, "filename").unwrap() })
                .collect(),
        })
        .collect()
}

fn records() -> Vec<LinkRecord> {
    messages().iter().flat_map(links::extract).collect()
}

#[test]
fn records_match() {
    let rows: Vec<Value> = records()
        .iter()
        .map(|r| {
            json!({
                "url": r.url, "source": r.source, "link_text": r.link_text, "author": r.author,
                "timestamp": r.timestamp, "jump_url": r.jump_url, "message_id": r.message_id, "author_id": r.author_id,
            })
        })
        .collect();
    support::assert_json(Value::Array(rows), "links/records.json");
}

#[test]
fn files_match_byte_for_byte() {
    let out = links::outputs(&records(), "song-of-the-day", (support::frozen_now(), 0));
    support::assert_bytes(format!("{}\n{}\n", out.md_name, out.csv_name).as_bytes(), "links/output_name.txt");
    support::assert_bytes(&out.md, "links/output.md");
    support::assert_bytes(&out.csv, "links/output.csv");
}

#[test]
fn summary_counts_unique_urls() {
    assert_eq!(links::summary(&records(), "song-of-the-day"), "Found **10** links (8 unique) in #song-of-the-day.");
}
