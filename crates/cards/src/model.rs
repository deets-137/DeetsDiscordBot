//! The DeetsMusic bridge's replies, as far as the formats read them (architecture/BRIDGE.md).
//!
//! Every field is optional, because the Python bot reads each one with `dict.get` and a
//! default. The formats keep those defaults exactly; see each use.

use serde::{Deserialize, Deserializer};

/// One song: a search result, a row of Up Next, or the current song in `/queue`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Apple's catalog id. The bridge sends a string; the bot compares it as `str(...)`, so a
    /// number is accepted and turned into its digits.
    #[serde(default, deserialize_with = "string_or_number")]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist_name: Option<String>,
    #[serde(default)]
    pub album_name: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<f64>,
    /// `"explicit"`, `"clean"`, or absent (Apple calls nothing else clean).
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub artwork: Option<Artwork>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    /// `…/{w}x{h}bb.jpg`
    #[serde(default)]
    pub url_template: Option<String>,
}

/// `/now-playing`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub playing: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub artwork_template: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    pub catalog_id: Option<String>,
    /// Seconds.
    #[serde(default)]
    pub current_time: Option<f64>,
    /// Seconds.
    #[serde(default)]
    pub duration: Option<f64>,
    /// 0..1.
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub live: bool,
}

/// `/queue`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Queue {
    #[serde(default)]
    pub current: Option<Track>,
    #[serde(default)]
    pub upcoming: Vec<Track>,
    #[serde(default)]
    pub history: Vec<Track>,
}

/// `artwork()`: a template's `{w}` and `{h}` filled with `size`.
fn sized(template: &str, size: u32) -> String {
    template.replace("{w}", &size.to_string()).replace("{h}", &size.to_string())
}

impl Track {
    /// The artwork at `size` px (`artwork.urlTemplate`), if the song has one.
    pub fn artwork_url(&self, size: u32) -> Option<String> {
        let template = self.artwork.as_ref()?.url_template.as_deref().filter(|t| !t.is_empty())?;
        Some(sized(template, size))
    }
}

impl NowPlaying {
    /// The artwork at `size` px: `artworkTemplate`, else the fixed-size `artworkUrl`.
    pub fn artwork_url(&self, size: u32) -> Option<String> {
        let template = [&self.artwork_template, &self.artwork_url].into_iter().flatten().find(|t| !t.is_empty())?;
        Some(sized(template, size))
    }
}

fn string_or_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        S(String),
        N(serde_json::Number),
    }
    Ok(Option::<Id>::deserialize(d)?.map(|id| match id {
        Id::S(s) => s,
        Id::N(n) => n.to_string(),
    }))
}
