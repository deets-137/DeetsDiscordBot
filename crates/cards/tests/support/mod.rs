//! The parity loader: reads the kit in `tests/parity/` (rust-port-plan.md §4).
//!
//! The kit is made from the Python bot by `scripts/parity/render_expected.py`. A Rust test
//! builds the same output from the same input and compares:
//! - JSON: equal after every `custom_id` is replaced with `"<custom_id>"` on both sides
//!   (discord.py makes random ids; the Rust bot makes fixed ones, and neither is compared).
//! - Text: equal byte for byte. The files keep the line endings Python wrote (`/links` files
//!   end lines with `\r\n`), and `.gitattributes` keeps git from changing them.
//!
//! This file lives in `crates/cards/tests/support/`. Another crate that needs the kit copies
//! it into its own `tests/support/` (a test-only helper is not worth a crate).
#![allow(dead_code)] // each test binary uses a different part of this module

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The replacement for every `custom_id` value, as the kit stores it.
pub const CUSTOM_ID: &str = "<custom_id>";

/// `tests/parity/` at the repo root.
pub fn parity_dir() -> PathBuf {
    // Every crate sits at crates/<name>, two levels under the repo root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity")
}

/// The bytes of one kit file, by its path under `tests/parity/` (e.g. `"expected/links/output.csv"`).
pub fn bytes(rel: &str) -> Vec<u8> {
    let path = parity_dir().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("parity kit: cannot read {}: {e}", path.display()))
}

/// One kit file as UTF-8 text, with its line endings as stored.
pub fn text(rel: &str) -> String {
    String::from_utf8(bytes(rel)).unwrap_or_else(|e| panic!("parity kit: {rel} is not UTF-8: {e}"))
}

/// One kit file as JSON.
pub fn json(rel: &str) -> Value {
    serde_json::from_slice(&bytes(rel)).unwrap_or_else(|e| panic!("parity kit: {rel} is not JSON: {e}"))
}

/// `tests/parity/input/<name>`.
pub fn input(name: &str) -> Value {
    json(&format!("input/{name}"))
}

/// `tests/parity/expected/<rel>` as JSON.
pub fn expected(rel: &str) -> Value {
    json(&format!("expected/{rel}"))
}

/// The cases in `expected/cases.json` for one function (`"clip"`, `"mmss"`, `"bar"`, …).
pub fn cases(name: &str) -> Vec<Value> {
    match expected("cases.json").get(name) {
        Some(Value::Array(list)) => list.clone(),
        other => panic!("parity kit: cases.json has no list `{name}` (found {other:?})"),
    }
}

/// The clock the kit was rendered at (`cases.json.frozen_now`, Unix seconds).
pub fn frozen_now() -> i64 {
    expected("cases.json")["frozen_now"]
        .as_i64()
        .expect("parity kit: cases.json.frozen_now is not an integer")
}

/// Replaces every `custom_id` value, at any depth, with [`CUSTOM_ID`].
pub fn normalize_custom_ids(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "custom_id" {
                    *child = Value::String(CUSTOM_ID.to_owned());
                } else {
                    normalize_custom_ids(child);
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(normalize_custom_ids),
        _ => {}
    }
}

/// Asserts that `actual` matches `expected/<rel>`, with `custom_id`s normalized on both sides.
/// On a failure, prints both sides in full and the first path where they differ.
pub fn assert_json(actual: Value, rel: &str) {
    let mut actual = actual;
    let mut want = expected(rel);
    normalize_custom_ids(&mut actual);
    normalize_custom_ids(&mut want);
    if actual != want {
        let at = first_difference(&actual, &want, String::from("$")).unwrap_or_default();
        panic!(
            "parity: expected/{rel} differs at {at}\n--- expected\n{}\n--- actual\n{}",
            serde_json::to_string_pretty(&want).unwrap(),
            serde_json::to_string_pretty(&actual).unwrap(),
        );
    }
}

/// Asserts that `actual` equals `expected/<rel>` byte for byte.
/// On a failure, prints the first differing byte offset and both texts with escapes visible.
pub fn assert_bytes(actual: &[u8], rel: &str) {
    let want = bytes(&format!("expected/{rel}"));
    if actual != want.as_slice() {
        let at = actual.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(actual.len().min(want.len()));
        panic!(
            "parity: expected/{rel} differs at byte {at} (lengths {} expected, {} actual)\n--- expected\n{:?}\n--- actual\n{:?}",
            want.len(),
            actual.len(),
            String::from_utf8_lossy(&want),
            String::from_utf8_lossy(actual),
        );
    }
}

/// The JSON path of the first place where `a` and `b` differ, or `None` if they are equal.
fn first_difference(a: &Value, b: &Value, path: String) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => first_difference(p, q, format!("{path}.{k}")),
                _ => Some(format!("{path}.{k} (present on one side only)")),
            })
        }
        (Value::Array(x), Value::Array(y)) => {
            if let Some(found) = x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_difference(p, q, format!("{path}[{i}]"))) {
                return Some(found);
            }
            (x.len() != y.len()).then(|| format!("{path} (length {} vs {})", x.len(), y.len()))
        }
        _ => (a != b).then_some(path),
    }
}
