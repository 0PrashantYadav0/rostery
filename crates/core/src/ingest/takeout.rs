//! Google Takeout's activity format: `watch-history.json` and its siblings, under whatever name
//! the person's language gives them. Titles are translated too, so a watch is told by its link
//! wherever that can decide. The row struct names only the fields Rostery keeps.

use super::{each_row, looks_like_id, played_at, Parsed};
use crate::model::{Origin, Play, Source};
use serde::Deserialize;

/// April 2005, the month of YouTube's first video. Nothing earlier can be a real Play.
const EARLIEST_WATCH: i64 = 1_112_313_600;

/// Activity that can link to a video without being a watch. Only the title says so.
// ponytail: English only, so a translated like with a video link counts as a watch. Those rows
// come from My Activity, not the watch history the how-to asks for. Add translations if real files
// show them.
const NOT_WATCHES: [&str; 12] = [
    "Liked ",
    "Disliked ",
    "Subscribed to ",
    "Unsubscribed from ",
    "Commented on ",
    "Shared ",
    "Saved ",
    "Searched for ",
    "Viewed ",
    "Visited ",
    "Voted on ",
    "Answered ",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
    title: Option<String>,
    title_url: Option<String>,
    subtitles: Option<Vec<Named>>,
    details: Option<Vec<Named>>,
    products: Option<Vec<String>>,
    time: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    name: Option<String>,
    url: Option<String>,
}

pub fn parse(bytes: &[u8], now: i64) -> serde_json::Result<Parsed> {
    each_row(bytes, |out, r: Activity| {
        // Takeout lists other products the same way, and a Chrome visit can link to YouTube too.
        let elsewhere = r
            .products
            .is_some_and(|p| !p.iter().any(|p| p.contains("YouTube")));
        if elsewhere {
            return;
        }
        // The word before "Google Ads" is translated, and the brand is not.
        let ad = r
            .details
            .iter()
            .flatten()
            .any(|d| d.name.as_deref().is_some_and(|n| n.contains("Google Ads")));
        if ad {
            out.excluded_ads += 1;
            return;
        }
        // A removed video has no link at all. Any other link is to something that is not a video.
        let item_ref = match r.title_url.as_deref() {
            Some(url) => match video_id(url) {
                Some(id) => Some(id.to_owned()),
                None => return,
            },
            None => None,
        };
        let title = r.title;
        let not_watch = |t: &String| NOT_WATCHES.iter().any(|verb| t.starts_with(verb));
        if title.as_ref().is_some_and(not_watch) {
            return;
        }
        out.keep(watch(r.time, title, item_ref, r.subtitles, now));
    })
}

/// A watch needs a believable time and a title. A deleted video has no `subtitles`, so it has no
/// Creator, and it stays a Play.
fn watch(
    time: Option<String>,
    title: Option<String>,
    item_ref: Option<String>,
    subtitles: Option<Vec<Named>>,
    now: i64,
) -> Option<Play> {
    let title = title?;
    // A title in another language has no prefix Rostery can know, so it stays whole.
    let item = title.strip_prefix("Watched ").unwrap_or(&title);
    if item.is_empty() {
        return None;
    }
    let channel = subtitles.into_iter().flatten().next();
    let (name, url) = channel.map_or((None, None), |c| (c.name, c.url));
    let name = name.unwrap_or_default();
    Some(Play {
        source: Source::Youtube,
        origin: Origin::Export,
        played_at: played_at(&time?, EARLIEST_WATCH, now)?,
        duration_ms: None,
        // YouTube Music's auto-made channels are all named "<artist> - Topic".
        creator: name.strip_suffix(" - Topic").unwrap_or(&name).to_owned(),
        creator_ref: url.as_deref().and_then(channel_ref),
        item: item.to_owned(),
        item_ref,
        skipped: false,
    })
}

/// The video a YouTube link plays, from `watch?v=ID` or `shorts/ID`.
fn video_id(url: &str) -> Option<&str> {
    let path = youtube_path(url)?.split('#').next()?;
    let id = match path.strip_prefix("watch?") {
        Some(query) => query.split('&').find_map(|pair| pair.strip_prefix("v="))?,
        None => path.strip_prefix("shorts/")?.split(['?', '/']).next()?,
    };
    looks_like_id(id).then_some(id)
}

/// The channel a YouTube link names, in one form. Only the ID comes from the file, so what is
/// stored is safe to use as a link.
fn channel_ref(url: &str) -> Option<String> {
    let id = youtube_path(url)?.strip_prefix("channel/")?;
    looks_like_id(id).then(|| format!("https://www.youtube.com/channel/{id}"))
}

/// What follows the host of a link to YouTube itself.
fn youtube_path(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    matches!(
        host,
        "www.youtube.com" | "youtube.com" | "m.youtube.com" | "music.youtube.com"
    )
    .then_some(path)
}
