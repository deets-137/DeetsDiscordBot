//! P0: the parity loader reads the whole kit, and its comparisons behave.
//! The real parity tests (cards, text, cases) come with P1.

mod support;

use serde_json::json;

/// Every file under `tests/parity/`, as a path relative to it with `/` separators.
fn kit_files() -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
    }
    let root = support::parity_dir();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

#[test]
fn the_kit_is_there() {
    let files = kit_files();
    for must in ["expected/cases.json", "expected/commands.json", "expected/cards/np_playing.json", "input/queue.json"] {
        assert!(files.iter().any(|f| f == must), "parity kit: {must} is missing (found {files:?})");
    }
}

#[test]
fn every_json_file_parses_and_every_text_file_is_utf8() {
    for rel in kit_files() {
        if rel.ends_with(".json") {
            support::json(&rel);
        } else {
            support::text(&rel);
        }
    }
}

#[test]
fn frozen_now_is_the_documented_clock() {
    assert_eq!(support::frozen_now(), 1_794_340_000); // rust-port-plan.md §4
}

#[test]
fn every_case_list_is_present() {
    for name in ["bar", "clip", "column_width", "confident", "mmss", "rating_word", "tag_alone", "tag_column"] {
        assert!(!support::cases(name).is_empty(), "cases.json: `{name}` is empty");
    }
}

#[test]
fn the_links_files_keep_crlf() {
    // Python writes both /links files with \r\n on Windows; the kit must not lose them.
    for rel in ["expected/links/output.md", "expected/links/output.csv"] {
        let text = support::text(rel);
        assert!(text.contains("\r\n"), "{rel} has no \\r\\n: was the kit converted on checkout?");
        assert!(!text.replace("\r\n", "").contains('\n'), "{rel} has a bare \\n");
    }
}

#[test]
fn custom_ids_are_normalized_at_any_depth() {
    let mut value = json!([{ "custom_id": "dm:np:skip", "components": [{ "custom_id": "dm:queue:2", "label": "▶" }] }]);
    support::normalize_custom_ids(&mut value);
    assert_eq!(value, json!([{ "custom_id": "<custom_id>", "components": [{ "custom_id": "<custom_id>", "label": "▶" }] }]));
}

#[test]
fn a_card_matches_itself_with_other_custom_ids() {
    let mut card = support::expected("cards/np_playing.json");
    fn rename(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(map) => map.iter_mut().for_each(|(k, c)| {
                if k == "custom_id" {
                    *c = json!("dm:np:anything");
                } else {
                    rename(c)
                }
            }),
            serde_json::Value::Array(list) => list.iter_mut().for_each(rename),
            _ => {}
        }
    }
    rename(&mut card);
    support::assert_json(card, "cards/np_playing.json");
}

#[test]
#[should_panic(expected = "differs at $[0].accent_color")]
fn a_changed_card_fails_and_names_the_place() {
    let mut card = support::expected("cards/np_playing.json");
    card[0]["accent_color"] = json!(0);
    support::assert_json(card, "cards/np_playing.json");
}

#[test]
fn text_compares_byte_for_byte() {
    let want = support::bytes("expected/links/output.csv");
    support::assert_bytes(&want, "links/output.csv");
}

#[test]
#[should_panic(expected = "differs at byte")]
fn text_with_lf_instead_of_crlf_fails() {
    let lf = support::text("expected/links/output.csv").replace("\r\n", "\n");
    support::assert_bytes(lf.as_bytes(), "links/output.csv");
}
