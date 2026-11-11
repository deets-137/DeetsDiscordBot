//! The text formats of `bot/dm.py` (and `mmss` from `bot/dm_bridge.py`), character for
//! character. The parity kit checks each one (tests/parity_text.rs).
//!
//! Python details kept on purpose:
//! - lengths and padding count Unicode code points (`len`, f-string `>` / `<`), as `char`s do;
//! - `round()` rounds a half to the even number: [`f64::round_ties_even`];
//! - a missing title in a suggestion prints `None` (Python's `str(None)`).

use crate::model::{NowPlaying, Track};

/// `_clip`: at most `n` characters; a longer text ends in "…" (U+2026).
pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// `_words`: the runs of `a-z0-9` in the lower-cased text.
pub fn words(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `mmss`: `M:SS` (minutes are not capped: `100:00`); `None` is "–:––" (en dashes).
pub fn mmss(seconds: Option<f64>) -> String {
    match seconds {
        None => "\u{2013}:\u{2013}\u{2013}".to_owned(),
        Some(s) => {
            let s = s.round_ties_even() as i64;
            format!("{}:{:02}", s.div_euclid(60), s.rem_euclid(60))
        }
    }
}

/// The owner, 2026-11-10: words, "clean / explicit / none", not badges.
pub const NO_RATING: &str = "None";

fn known_rating(t: &Track) -> Option<&'static str> {
    match t.content_rating.as_deref() {
        Some("explicit") => Some("Explicit"),
        Some("clean") => Some("Clean"),
        _ => None,
    }
}

/// `rating_word`: Explicit, Clean, or None.
pub fn rating_word(t: &Track) -> &'static str {
    known_rating(t).unwrap_or(NO_RATING)
}

/// `_secs`: the length in seconds (`durationMs or 0`).
pub fn secs(t: &Track) -> f64 {
    t.duration_ms.unwrap_or(0.0) / 1000.0
}

/// `tag`: `` `M:SS | Explicit` `` in a code span.
/// - `width: Some(w)` (a column: queue page, picker): the length right-aligned to `w`, the
///   word padded to 8, so every row is the same width in the monospace span.
/// - `width: None` (one-off: the add line, the np row): an unrated song is just `` `M:SS` ``.
pub fn tag(t: &Track, width: Option<usize>) -> String {
    let length = mmss(Some(secs(t)));
    match width {
        None => match known_rating(t) {
            Some(word) => format!("`{length} | {word}`"),
            None => format!("`{length}`"),
        },
        Some(w) => format!("`{length:>w$} | {:<8}`", rating_word(t)),
    }
}

/// Exactly one digit wide, in any font that has it.
pub const FIGURE_SPACE: char = '\u{2007}';

/// `_WORD_PAD`: makes "Clean" and "None" about as wide as "Explicit" in a proportional font,
/// from spaces Discord does not collapse (figure, hair, thin).
fn word_pad(word: &str) -> &'static str {
    match word {
        "Clean" => "\u{2007}\u{200A}",
        "None" => "\u{2007}\u{2009}",
        _ => "",
    }
}

/// `suggestion`: one autocomplete row, cut to 100 characters. The length leads, padded with
/// figure spaces to the query's `column_width`, then the word, then the song.
pub fn suggestion(t: &Track, width: usize) -> String {
    let word = rating_word(t);
    let length = mmss(Some(secs(t)));
    let pad: String = std::iter::repeat_n(FIGURE_SPACE, width.saturating_sub(length.chars().count())).collect();
    let title = t.title.as_deref().unwrap_or("None");
    let artist = t.artist_name.as_deref().unwrap_or("None");
    let album = t.album_name.as_deref().unwrap_or("");
    clip(&format!("{pad}{length} {word}{} {title} — {artist} · {album}", word_pad(word)), 100)
}

/// `column_width`: characters in the longest `M:SS` of the rows (4 for none).
pub fn column_width(tracks: &[Track]) -> usize {
    tracks.iter().map(|t| mmss(Some(secs(t))).chars().count()).max().unwrap_or(4)
}

/// `_line`: "Title — Artist" ("?" for a missing part).
pub fn line(t: &Track) -> String {
    format!("{} — {}", t.title.as_deref().unwrap_or("?"), t.artist_name.as_deref().unwrap_or("?"))
}

/// `_notices`: a write's `notices: [{kind, text}]` (DeetsMusic's warn and error toasts), one
/// `-# ⚠ …` line each, to append to the reply.
pub fn notices(reply: &serde_json::Value) -> String {
    reply
        .get("notices")
        .and_then(|n| n.as_array())
        .into_iter()
        .flatten()
        .filter_map(|n| n.get("text").and_then(|t| t.as_str()).filter(|t| !t.is_empty()))
        .map(|t| format!("\n-# ⚠ {t}"))
        .collect()
}

/// `confident`: the top result is a sure thing. Every typed word is in its title, artist or
/// album (or starts one of its words), and the runner-up does not also have every word.
pub fn confident(term: &str, songs: &[Track]) -> bool {
    let Some(top) = songs.first() else { return false };
    let want = words(term);
    if want.is_empty() {
        return false;
    }
    let hay = |t: &Track| -> Vec<String> {
        words(&format!(
            "{} {} {}",
            t.title.as_deref().unwrap_or(""),
            t.artist_name.as_deref().unwrap_or(""),
            t.album_name.as_deref().unwrap_or("")
        ))
    };
    let all_in = |h: &[String]| want.iter().all(|w| h.contains(w));
    let top_hay = hay(top);
    if !all_in(&top_hay) && !want.iter().all(|w| top_hay.iter().any(|h| h.starts_with(w.as_str()))) {
        return false;
    }
    songs.len() == 1 || !all_in(&hay(&songs[1]))
}

/// `_bar`: the scrubber, 18 cells, drawn once.
pub fn bar(progress: f64) -> String {
    const CELLS: i64 = 18;
    let k = ((progress * (CELLS - 1) as f64).round_ties_even() as i64).clamp(0, CELLS - 1);
    format!("{}●{}", "━".repeat(k as usize), "─".repeat((CELLS - 1 - k) as usize))
}

/// The actions in `bot/dm.py` `RULE`'s order.
pub const ACTIONS: [&str; 11] = ["np", "queue", "pause", "resume", "skip", "back", "add", "remove", "move", "play", "volume"];

/// `_row_heading`: a queue row in layout C (the owner, 2026-11-10; CARDS.md §1): the number
/// and title as a small heading, then artist · album, then the length and rating in small text.
pub fn queue_row(n: usize, t: &Track) -> String {
    format!(
        "### {n} · {}\n{} · {}\n-# {} · {}",
        clip(t.title.as_deref().unwrap_or("?"), 70),
        clip(t.artist_name.as_deref().unwrap_or(""), 50),
        clip(t.album_name.as_deref().unwrap_or(""), 60),
        mmss(Some(secs(t))),
        rating_word(t),
    )
}

/// `help_text`: what `/dm help` shows. `can(action)` is whether this person may do it now.
pub fn help_text(can: impl Fn(&str) -> bool, host: bool, session: bool) -> String {
    let mark = |action: &str| if can(action) { "✅" } else { "🔒" };
    let who = if host {
        "You're the host: everything is open to you."
    } else if session {
        "The host is playing DeetsMusic here. ✅ = you can, 🔒 = host only right now (DeetsMusic › Settings › Rooms)."
    } else {
        "Guests can use /dm while the host plays DeetsMusic into voice with `/vc play`."
    };
    format!(
        "### Lil Deets · DeetsMusic\n\
         -# {who}\n\
         {} `/dm np` — what's playing, with \u{23EE} \u{23EF} \u{23ED}\n\
         {} `/dm queue` — Up Next, 10 a page\n\
         {} `/dm pause` · `/dm resume`\n\
         {} `/dm skip` · `/dm back`\n\
         {} `/dm add` — search as you type; `next: True` plays it next\n\
         {} `/dm remove` · `/dm move` — edit Up Next by row number\n\
         {} `/dm play` — replace what's playing\n\
         {} `/dm volume` — the bot's volume in the call\n\
         -# The host starts and stops the stream with `/vc play` · `/vc stop` · `/vc leave`.",
        mark("np"),
        mark("queue"),
        mark("pause"),
        mark("skip"),
        mark("add"),
        mark("remove"),
        mark("play"),
        mark("volume"),
    )
}

/// Where an added song landed, read from the bridge after the add.
pub struct Placement<'a> {
    pub upcoming: &'a [Track],
    pub now_playing: &'a NowPlaying,
}

/// `_added`: the channel line for an add, with where it landed in Up Next and when it plays.
/// - `placement: None` is the Python bot's bridge-error path: the line ends there.
/// - `now` is the Unix time in seconds (the parity kit freezes it).
pub fn added_line(user: &str, tracks: &[Track], next_up: bool, song_id: &str, placement: Option<Placement<'_>>, now: f64) -> String {
    let what = tracks.first().map(line).unwrap_or_else(|| "that".to_owned());
    let tag_part = tracks.first().map(|t| format!(" {}", tag(t, None))).unwrap_or_default();
    let head = format!("➕ {user} added **{what}**{tag_part}");
    let Some(Placement { upcoming, now_playing }) = placement else {
        return head + ".";
    };
    let cid = song_id.strip_prefix("song:").unwrap_or(song_id);
    let rows: Vec<usize> = upcoming
        .iter()
        .enumerate()
        .filter(|(_, t)| t.catalog_id.as_deref().unwrap_or("None") == cid)
        .map(|(i, _)| i + 1)
        .collect();
    let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
        return head + ".";
    };
    let n = if next_up { first } else { last };
    let mut ahead = (now_playing.duration.unwrap_or(0.0) - now_playing.current_time.unwrap_or(0.0)).max(0.0);
    ahead += upcoming[..n - 1].iter().map(|t| t.duration_ms.unwrap_or(0.0)).sum::<f64>() / 1000.0;
    let when = if now_playing.playing {
        format!("plays <t:{}:R>", (now + ahead).trunc() as i64)
    } else {
        format!("{} of music ahead of it", mmss(Some(ahead)))
    };
    format!("{head} · #{n} of {} in Up Next · {when}.", upcoming.len())
}
