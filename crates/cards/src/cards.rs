//! The Components V2 cards (features/CARDS.md), built with twilight-model 0.18 types: the
//! port of `docs/archive/bot/dm.py` `np_view`, `queue_view` and `picker_view`. Pure: the caller passes
//! the bridge's replies, the accent and the time.
//!
//! Each builder returns the message's top-level `components` (one container). Send them
//! with `IS_COMPONENTS_V2` and no `content` (`lildeets_rest::routes::components_v2`).
//!
//! custom_ids are fixed strings (discord.py made random ones), so a click is routed by its
//! id alone, from any process, after any restart:
//! - `dm:np:<back|pause|resume|skip|queue|open>` (`open`: the Activity, 2026-11-11)
//! - `dm:queue:<page>:<prev|next>` (the page to show; the direction keeps the ids unique)
//! - `dm:pick:<n>:<next 0|1>:<catalogId>`

use twilight_model::channel::message::component::{
    ActionRow, Button, ButtonStyle, Component, Container, Section, Separator, SeparatorSpacingSize, TextDisplay, Thumbnail,
    UnfurledMediaItem,
};

use crate::model::{NowPlaying, Queue, Track};
use crate::text::{bar, clip, column_width, line, mmss, queue_row, tag};

/// Up Next rows per queue page.
pub const PAGE: usize = 10;
/// Discord's limit: components per message, nested ones counted.
pub const MAX_COMPONENTS: usize = 40;

fn text(content: impl Into<String>) -> Component {
    Component::TextDisplay(TextDisplay { content: content.into(), id: None })
}

/// `_section`: the text with the artwork beside it, or the text alone.
fn section(content: String, art: Option<String>) -> Component {
    match art {
        Some(url) => Component::Section(Section {
            accessory: Box::new(Component::Thumbnail(Thumbnail {
                description: Some(None),
                id: None,
                media: UnfurledMediaItem { content_type: None, height: None, proxy_url: None, url, width: None },
                spoiler: Some(false),
            })),
            components: vec![text(content)],
            id: None,
        }),
        None => text(content),
    }
}

fn button(custom_id: String, label: &str, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style: ButtonStyle::Secondary,
        url: None,
    })
}

fn row(buttons: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { components: buttons, id: None })
}

fn container(parts: Vec<Component>, accent: u32) -> Vec<Component> {
    vec![Component::Container(Container { accent_color: Some(Some(accent)), components: parts, id: None, spoiler: Some(false) })]
}

/// `np_view`: what is playing, with ⏮ ⏯ ⏭ and Queue.
/// - `where_`: the call it streams into, as a channel mention (`<#id>`), when the bot is
///   streaming DeetsMusic in this server.
/// - `up_next`: the first song of Up Next.
/// - `now`: Unix time in seconds (the "ends in" stamp).
/// - `open`: add the "Open in call" button, which opens the Activity (his call 2A,
///   2026-11-11). Only when the relay is set up; the parity kit (the Python card) has none.
pub fn np_card(np: &NowPlaying, up_next: Option<&Track>, where_: Option<&str>, accent: u32, now: f64, open: bool) -> Vec<Component> {
    let Some(title) = np.title.as_deref().filter(|t| np.active && !t.is_empty()) else {
        return container(vec![text("Nothing is playing in DeetsMusic.")], accent);
    };
    let state = if np.playing { "Now playing" } else { "Paused" };
    let where_ = where_.map(|w| format!(" in {w}")).unwrap_or_default();
    let dur = np.duration.unwrap_or(0.0);
    let at = np.current_time.unwrap_or(0.0);
    let ends = if np.playing && dur != 0.0 {
        format!(" · ends <t:{}:R>", (now + dur - at).trunc() as i64)
    } else if dur != 0.0 {
        format!(" · paused at {}", mmss(Some(at)))
    } else {
        String::new()
    };
    let up = up_next.map(|t| format!(" · up next: {}", line(t))).unwrap_or_default();
    let body = format!(
        "-# {state}{where_}\n### {title}\n{} · {}\n`{}` {} `{}`\n-# {}{ends}{up}",
        np.artist.as_deref().unwrap_or(""),
        np.album.as_deref().unwrap_or(""),
        mmss(Some(at)),
        bar(np.progress.unwrap_or(0.0)),
        mmss(Some(dur)),
        mmss(Some(dur)),
    );
    let (play_id, play_label) = if np.playing { ("pause", "⏸") } else { ("resume", "▶") };
    let mut buttons = vec![
        button("dm:np:back".into(), "⏮", false),
        button(format!("dm:np:{play_id}"), play_label, false),
        button("dm:np:skip".into(), "⏭", false),
        button("dm:np:queue".into(), "Queue", false),
    ];
    if open {
        buttons.push(button("dm:np:open".into(), "Open in call", false));
    }
    let buttons = row(buttons);
    container(vec![section(body, np.artwork_url(300)), buttons], accent)
}

/// The pages Up Next fills (at least 1).
pub fn queue_pages(q: &Queue) -> usize {
    q.upcoming.len().div_ceil(PAGE).max(1)
}

/// `queue_view`: the current song, then one page of Up Next in layout C, then ◀ ▶.
/// `page` is clamped to 1..=pages.
pub fn queue_card(q: &Queue, page: usize, accent: u32) -> Vec<Component> {
    let pages = queue_pages(q);
    let page = page.clamp(1, pages);
    let total_ms: f64 = q.upcoming.iter().map(|t| t.duration_ms.unwrap_or(0.0)).sum();
    let total_s = (total_ms / 1000.0).floor() as i64;
    let (h, m) = ((total_s / 60) / 60, (total_s / 60) % 60);
    let length = if h > 0 { format!("{h} h {m} m") } else { format!("{m} m") };
    let head = match &q.current {
        Some(cur) => format!(
            "-# Now playing\n**{}** · {} {}",
            cur.title.as_deref().unwrap_or("None"),
            cur.artist_name.as_deref().unwrap_or(""),
            tag(cur, None)
        ),
        None => "-# Nothing playing".to_owned(),
    } + &format!("\n-# Up Next · {} songs · {length} · page {page} of {pages}", q.upcoming.len());
    let mut parts = vec![
        section(head, q.current.as_ref().and_then(|c| c.artwork_url(300))),
        Component::Separator(Separator { divider: Some(true), id: None, spacing: Some(SeparatorSpacingSize::Small) }),
    ];
    let start = (page - 1) * PAGE;
    for (i, t) in q.upcoming.iter().enumerate().skip(start).take(PAGE) {
        parts.push(section(queue_row(i + 1, t), t.artwork_url(120)));
    }
    if q.upcoming.is_empty() {
        parts.push(text("Up Next is empty. Add a song with `/dm add`."));
    }
    parts.push(row(vec![
        // The direction keeps the two ids apart: on a one-page queue both targets are page 1,
        // and Discord refuses a message with a duplicated custom_id (400, 2026-11-10).
        button(format!("dm:queue:{}:prev", page.saturating_sub(1).max(1)), "◀", page <= 1),
        button(format!("dm:queue:{}:next", (page + 1).min(pages)), "▶", page >= pages),
    ]));
    container(parts, accent)
}

/// `picker_view`: the top 5 for a term, with buttons 1–5. Only the asker sees it.
pub fn picker_card(term: &str, songs: &[Track], next_up: bool, accent: u32) -> Vec<Component> {
    let top = &songs[..songs.len().min(5)];
    let width = column_width(top);
    let mut parts = vec![text(format!("Which one? “{}”", clip(term, 80)))];
    let mut buttons = Vec::new();
    for (i, t) in top.iter().enumerate() {
        let n = i + 1;
        parts.push(section(
            format!(
                "`{n}` {} **{}** · {}\n-# {}",
                tag(t, Some(width)),
                clip(t.title.as_deref().unwrap_or("?"), 80),
                t.artist_name.as_deref().unwrap_or(""),
                t.album_name.as_deref().unwrap_or(""),
            ),
            t.artwork_url(120),
        ));
        let id = format!("dm:pick:{n}:{}:{}", u8::from(next_up), t.catalog_id.as_deref().unwrap_or(""));
        buttons.push(button(id, &n.to_string(), false));
    }
    parts.push(row(buttons));
    container(parts, accent)
}

/// `/dm help`: the help text in a container, with the accent when the bridge gave one.
pub fn help_card(help: String, accent: Option<u32>) -> Vec<Component> {
    vec![Component::Container(Container { accent_color: accent.map(Some), components: vec![text(help)], id: None, spoiler: Some(false) })]
}

/// A V2 message that is one line of text (the picker after a pick: "Picked …").
pub fn line_card(line: String) -> Vec<Component> {
    vec![text(line)]
}

/// Components in a message, nested ones counted, as Discord counts them for its 40 limit.
pub fn count(components: &[Component]) -> usize {
    components
        .iter()
        .map(|c| {
            1 + match c {
                Component::Container(x) => count(&x.components),
                Component::ActionRow(x) => count(&x.components),
                Component::Section(x) => count(&x.components) + count(std::slice::from_ref(&x.accessory)),
                _ => 0,
            }
        })
        .sum()
}
