//! `/links`: every hyperlink in a channel's history, as Markdown + CSV in a DM
//! (features/LINKS.md). The port of `journal/linkscan.py`; the journal keeps its own copy.
//!
//! Python details kept on purpose, because the files are compared byte for byte:
//! - The two patterns (`_MASKED_RE`, `_BARE_RE`) are matched by hand with `re`'s semantics:
//!   the same left-to-right scan, the same character classes, `\s` as Python's `str.isspace`.
//! - A message's time is discord.py's `created_at`: the **snowflake** time (ms precision),
//!   printed as `datetime.isoformat()` (`.123000` only when the milliseconds are not 0).
//! - The Markdown is written in text mode on Windows: every `\n` becomes `\r\n`.
//! - The CSV is the csv module's: `\r\n` rows, a field quoted only when it holds `,`, `"`,
//!   `\r` or `\n`, a `"` doubled.

use serde_json::Value;

// --- the message, as extraction reads it -------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Embed {
    pub url: Option<String>,
    pub title: Option<String>,
    pub image: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachment {
    pub url: String,
    pub filename: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub id: u64,
    pub content: String,
    pub jump_url: String,
    /// ISO 8601, UTC, as `datetime.isoformat()` prints it.
    pub timestamp: String,
    /// discord.py's `display_name`: the server nickname, else the global name, else the username.
    pub author: String,
    pub author_id: u64,
    pub embeds: Vec<Embed>,
    pub attachments: Vec<Attachment>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn id(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_str).and_then(|x| x.parse().ok()).unwrap_or(0)
}

impl Message {
    /// A message from Discord's API (`GET /channels/{id}/messages`). History replies carry no
    /// guild id, so the caller passes it (for the jump link; `None` in a DM: `@me`).
    pub fn from_discord(m: &Value, guild: Option<u64>) -> Message {
        let msg_id = id(m, "id");
        let channel = id(m, "channel_id");
        let author = m.get("author").cloned().unwrap_or(Value::Null);
        let display = m
            .get("member")
            .and_then(|mem| s(mem, "nick"))
            .or_else(|| s(&author, "global_name"))
            .or_else(|| s(&author, "username"))
            .unwrap_or_default();
        let embeds = m
            .get("embeds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|e| Embed {
                url: s(e, "url"),
                title: s(e, "title"),
                image: e.get("image").and_then(|i| s(i, "url")),
                thumbnail: e.get("thumbnail").and_then(|i| s(i, "url")),
            })
            .collect();
        let attachments = m
            .get("attachments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|a| Attachment { url: s(a, "url").unwrap_or_default(), filename: s(a, "filename").unwrap_or_default() })
            .collect();
        Message {
            id: msg_id,
            content: s(m, "content").unwrap_or_default(),
            jump_url: format!("https://discord.com/channels/{}/{channel}/{msg_id}", guild.map_or("@me".to_owned(), |g| g.to_string())),
            timestamp: snowflake_iso(msg_id),
            author: display,
            author_id: id(&author, "id"),
            embeds,
            attachments,
        }
    }
}

// --- time ---------------------------------------------------------------------------------

const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// (year, month, day) for days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Unix time (seconds + microseconds) as `datetime.isoformat()` prints a UTC time.
pub fn iso_utc(secs: i64, micros: u32) -> String {
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    let frac = if micros == 0 { String::new() } else { format!(".{micros:06}") };
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}{frac}+00:00", t / 3600, t / 60 % 60, t % 60)
}

/// The file-name stamp: `%Y%m%d-%H%M%S`.
pub fn stamp(secs: i64) -> String {
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!("{y:04}{mo:02}{d:02}-{:02}{:02}{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// A snowflake's creation time (discord.py's `created_at`), as `isoformat()`.
pub fn snowflake_iso(id: u64) -> String {
    let ms = (id >> 22) + DISCORD_EPOCH_MS;
    iso_utc((ms / 1000) as i64, (ms % 1000) as u32 * 1000)
}

// --- the patterns ------------------------------------------------------------------------

/// Python's `str.isspace`, which `\s` follows in a `str` pattern.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `https?://` at `i`: the index after it.
fn scheme(t: &[char], i: usize) -> Option<usize> {
    let at = |j: usize, c: char| t.get(j) == Some(&c);
    if !(at(i, 'h') && at(i + 1, 't') && at(i + 2, 't') && at(i + 3, 'p')) {
        return None;
    }
    let j = if at(i + 4, 's') && at(i + 5, ':') { i + 5 } else { i + 4 };
    (at(j, ':') && at(j + 1, '/') && at(j + 2, '/')).then_some(j + 3)
}

/// The longest run of `ok` chars from `i` (at least one): its end.
fn run(t: &[char], i: usize, ok: impl Fn(char) -> bool) -> Option<usize> {
    let end = t[i.min(t.len())..].iter().position(|&c| !ok(c)).map_or(t.len(), |n| i + n);
    (end > i).then_some(end)
}

/// `_MASKED_RE.findall`: `\[([^\]]+)\]\((https?://[^\s)]+)\)` → (text, url).
fn masked(t: &[char]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let hit = (|| {
            if t[i] != '[' {
                return None;
            }
            let text_end = run(t, i + 1, |c| c != ']')?;
            if t.get(text_end) != Some(&']') || t.get(text_end + 1) != Some(&'(') {
                return None;
            }
            let url_start = text_end + 2;
            let after_scheme = scheme(t, url_start)?;
            let url_end = run(t, after_scheme, |c| !py_space(c) && c != ')')?;
            (t.get(url_end) == Some(&')')).then(|| {
                let text: String = t[i + 1..text_end].iter().collect();
                let url: String = t[url_start..url_end].iter().collect();
                ((text, url), url_end + 1)
            })
        })();
        match hit {
            Some((m, next)) => {
                out.push(m);
                i = next;
            }
            None => i += 1,
        }
    }
    out
}

/// `_BARE_RE.findall`: `https?://[^\s<>()\[\]]+`.
fn bare(t: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let hit = scheme(t, i).and_then(|j| run(t, j, |c| !py_space(c) && !"<>()[]".contains(c)));
        match hit {
            Some(end) => {
                out.push(t[i..end].iter().collect());
                i = end;
            }
            None => i += 1,
        }
    }
    out
}

/// `_clean`: trailing punctuation the bare pattern swept up.
fn clean(url: &str) -> String {
    url.trim_end_matches(|c| ".,;:!?\"'".contains(c)).to_owned()
}

// --- records -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct LinkRecord {
    pub url: String,
    /// "text" | "embed" | "attachment"
    pub source: &'static str,
    pub author: String,
    pub author_id: u64,
    pub timestamp: String,
    pub message_id: u64,
    pub jump_url: String,
    /// The masked link's text, the embed's title, or the attachment's file name.
    pub link_text: String,
}

/// `extract_from_message`: masked links first (their urls are not counted again as bare),
/// then bare urls, then each embed's url / image / thumbnail, then attachments.
pub fn extract(m: &Message) -> Vec<LinkRecord> {
    let rec = |url: &str, source: &'static str, link_text: &str| LinkRecord {
        url: clean(url),
        source,
        author: m.author.clone(),
        author_id: m.author_id,
        timestamp: m.timestamp.clone(),
        message_id: m.id,
        jump_url: m.jump_url.clone(),
        link_text: link_text.to_owned(),
    };
    let chars: Vec<char> = m.content.chars().collect();
    let mut out = Vec::new();
    let mut masked_urls = Vec::new();
    for (text, url) in masked(&chars) {
        out.push(rec(&url, "text", text.trim_matches(py_space)));
        masked_urls.push(clean(&url));
    }
    for url in bare(&chars) {
        if !masked_urls.contains(&clean(&url)) {
            out.push(rec(&url, "text", ""));
        }
    }
    for e in &m.embeds {
        let title = e.title.as_deref().unwrap_or("");
        for candidate in [&e.url, &e.image, &e.thumbnail].into_iter().flatten().filter(|c| !c.is_empty()) {
            out.push(rec(candidate, "embed", title));
        }
    }
    for a in &m.attachments {
        out.push(rec(&a.url, "attachment", &a.filename));
    }
    out
}

// --- the files ---------------------------------------------------------------------------

/// The two files `/links` sends, ready to attach.
#[derive(Clone, Debug, PartialEq)]
pub struct Outputs {
    pub md_name: String,
    pub md: Vec<u8>,
    pub csv_name: String,
    pub csv: Vec<u8>,
}

/// `write_outputs`, without the disk: `label` is the channel's name; `now` is Unix time
/// (seconds, microseconds).
pub fn outputs(records: &[LinkRecord], label: &str, now: (i64, u32)) -> Outputs {
    let mut safe = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            safe.push(c);
        } else if !safe.ends_with('\u{0}') {
            safe.push('\u{0}'); // one dash per run of other characters
        }
    }
    let safe = safe.replace('\u{0}', "-");
    let safe = safe.trim_matches('-');
    let safe = if safe.is_empty() { "channel" } else { safe };
    let base = format!("links_{safe}_{}", stamp(now.0));

    // Markdown: deduped by url (first seen kept), in record order.
    let mut unique: Vec<&LinkRecord> = Vec::new();
    for r in records {
        if !unique.iter().any(|u| u.url == r.url) {
            unique.push(r);
        }
    }
    let mut lines = vec![
        format!("# Links from #{label}"),
        String::new(),
        format!("_Scanned {} — {} total, {} unique._", iso_utc(now.0, now.1), records.len(), unique.len()),
        String::new(),
    ];
    for r in &unique {
        let display = if r.link_text.is_empty() { &r.url } else { &r.link_text };
        lines.push(format!("- [{display}]({})", r.url));
        let day: String = r.timestamp.chars().take(10).collect();
        lines.push(format!("  - {} · {day} · [jump]({}) · `{}`", r.author, r.jump_url, r.source));
    }
    let md = (lines.join("\n") + "\n").replace('\n', "\r\n");

    // CSV: every record, no dedupe.
    let mut csv = String::from("url,source,link_text,author,timestamp,jump_url,message_id\r\n");
    for r in records {
        let row = [r.url.as_str(), r.source, &r.link_text, &r.author, &r.timestamp, &r.jump_url, &r.message_id.to_string()].map(csv_field);
        csv.push_str(&row.join(","));
        csv.push_str("\r\n");
    }
    Outputs { md_name: format!("{base}.md"), md: md.into_bytes(), csv_name: format!("{base}.csv"), csv: csv.into_bytes() }
}

/// One field as the csv module writes it (QUOTE_MINIMAL, `\r\n` line terminator).
fn csv_field(f: &str) -> String {
    if f.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", f.replace('"', "\"\""))
    } else {
        f.to_owned()
    }
}

/// The reply: "Found **10** links (8 unique) in #label."
pub fn summary(records: &[LinkRecord], label: &str) -> String {
    let mut urls: Vec<&str> = records.iter().map(|r| r.url.as_str()).collect();
    urls.sort_unstable();
    urls.dedup();
    format!("Found **{}** links ({} unique) in #{label}.", records.len(), urls.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn the_patterns_follow_python_re() {
        // Each expectation was taken from Python's re on the same text.
        assert_eq!(bare(&chars("see https://a.b/c, and http://x.y/(z)")), ["https://a.b/c,", "http://x.y/"]);
        assert_eq!(bare(&chars("<https://tenor.com/view/x>")), ["https://tenor.com/view/x"]);
        assert_eq!(bare(&chars("httpss://no https:/no https//no")), Vec::<String>::new());
        assert_eq!(masked(&chars("[a](https://u) [](https://v) [b] (https://w) [c](ftp://x) [d](https://y z)")), [("a".into(), "https://u".into())]);
        assert_eq!(masked(&chars("[[x](https://u)")), [("[x".into(), "https://u".into())]);
        assert_eq!(masked(&chars("[multi\nline](https://u)")), [("multi\nline".into(), "https://u".into())]);
        assert_eq!(clean("https://a.b/c!?'\""), "https://a.b/c");
    }

    #[test]
    fn snowflake_time_is_discord_pys_created_at() {
        // 175928847299117063 is Discord's documented example: 2016-04-30 11:18:25.796 UTC.
        assert_eq!(snowflake_iso(175_928_847_299_117_063), "2016-04-30T11:18:25.796000+00:00");
        assert_eq!(iso_utc(0, 0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso_utc(951_782_400, 0), "2000-02-29T00:00:00+00:00");
        assert_eq!(stamp(1_794_340_000), "20261110-194640");
    }

    #[test]
    fn csv_quotes_like_the_csv_module() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field(""), "");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn label_to_a_safe_file_name() {
        let name = |l: &str| outputs(&[], l, (1_794_340_000, 0)).md_name;
        assert_eq!(name("song-of-the-day"), "links_song-of-the-day_20261110-194640.md");
        assert_eq!(name("🎵 música!! 2"), "links_m-sica-2_20261110-194640.md");
        assert_eq!(name("!!!"), "links_channel_20261110-194640.md");
    }

    #[test]
    fn a_discord_message_reads_its_author_and_jump_link() {
        let m = serde_json::json!({
            "id": "175928847299117063", "channel_id": "2", "content": "x",
            "author": {"id": "11", "username": "ada", "global_name": "Ada L"},
            "member": {"nick": "Ada"},
            "embeds": [{"url": "https://e", "title": "T", "image": {"url": "https://i"}}],
            "attachments": [{"url": "https://cdn/f.mp3", "filename": "f.mp3"}]
        });
        let msg = Message::from_discord(&m, Some(1));
        assert_eq!(msg.author, "Ada");
        assert_eq!(msg.author_id, 11);
        assert_eq!(msg.jump_url, "https://discord.com/channels/1/2/175928847299117063");
        assert_eq!(msg.embeds[0].image.as_deref(), Some("https://i"));
        let no_member = serde_json::json!({"id": "1", "channel_id": "2", "author": {"id": "3", "username": "bo", "global_name": null}});
        let msg = Message::from_discord(&no_member, None);
        assert_eq!((msg.author.as_str(), msg.jump_url.as_str()), ("bo", "https://discord.com/channels/@me/2/1"));
    }
}
