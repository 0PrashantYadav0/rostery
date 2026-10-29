//! Spotify's two history formats. The row structs name only the fields Rostery keeps, so the
//! rest of a row (IP address, username, device) is skipped by the parser and never held.

use super::{each_row, played_at, Parsed};
use crate::model::{Origin, Play, Source};
use serde::Deserialize;

/// Spotify counts a stream from 30 seconds, so anything shorter was skipped.
const SKIP_UNDER_MS: i64 = 30_000;
/// Longer than any real stream. At this cap the stored total needs billions of rows to overflow.
const MAX_MS_PLAYED: i64 = 30 * 24 * 3_600_000;
/// 2006-01-01, the year Spotify was founded. Nothing earlier can be a real Play.
const EARLIEST_PLAY: i64 = 1_136_073_600;

/// `Streaming_History_Audio_*.json`, from the Extended streaming history.
#[derive(Deserialize)]
struct Extended {
    ts: Option<String>,
    ms_played: Option<i64>,
    master_metadata_album_artist_name: Option<String>,
    master_metadata_track_name: Option<String>,
    spotify_track_uri: Option<String>,
    episode_show_name: Option<String>,
    episode_name: Option<String>,
    spotify_episode_uri: Option<String>,
    skipped: Option<bool>,
    incognito_mode: Option<bool>,
}

/// `StreamingHistory_music_*.json` and `StreamingHistory_podcast_*.json`, from the regular
/// Account data. A podcast row names its show and episode where a music row has artist and track.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    end_time: Option<String>,
    #[serde(alias = "podcastName")]
    artist_name: Option<String>,
    #[serde(alias = "episodeName")]
    track_name: Option<String>,
    ms_played: Option<i64>,
}

pub fn parse_extended(bytes: &[u8], now: i64) -> serde_json::Result<Parsed> {
    each_row(bytes, |out, r: Extended| {
        // A podcast row has no track, and its show plays the part of the artist.
        let play = play(
            r.ts.as_deref(),
            r.master_metadata_album_artist_name.or(r.episode_show_name),
            r.master_metadata_track_name.or(r.episode_name),
            r.spotify_track_uri.or(r.spotify_episode_uri),
            r.ms_played,
            r.skipped == Some(true),
            now,
        );
        if r.incognito_mode == Some(true) {
            out.excluded_private += 1;
            out.private.extend(play);
        } else {
            out.keep(play);
        }
    })
}

pub fn parse_account(bytes: &[u8], now: i64) -> serde_json::Result<Parsed> {
    each_row(bytes, |out, r: Account| {
        out.keep(play(
            r.end_time.as_deref(),
            r.artist_name,
            r.track_name,
            None,
            r.ms_played,
            false,
            now,
        ));
    })
}

/// A Play needs a believable time, a Creator and an Item, and any duration it has must be in
/// range. Anything else (a video row, an audiobook, a forged value) is `None`.
fn play(
    time: Option<&str>,
    creator: Option<String>,
    item: Option<String>,
    item_ref: Option<String>,
    ms_played: Option<i64>,
    skipped: bool,
    now: i64,
) -> Option<Play> {
    let played_at = played_at(time?, EARLIEST_PLAY, now)?;
    if ms_played.is_some_and(|ms| !(0..=MAX_MS_PLAYED).contains(&ms)) {
        return None;
    }
    Some(Play {
        source: Source::Spotify,
        origin: Origin::Export,
        played_at,
        duration_ms: ms_played,
        creator: creator?,
        creator_ref: None,
        item: item?,
        item_ref,
        skipped: skipped || ms_played.is_some_and(|ms| ms < SKIP_UNDER_MS),
    })
}
