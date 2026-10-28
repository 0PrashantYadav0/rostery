//! Spotify's two history formats. The row structs name only the fields Rostery keeps, so the
//! rest of a row (IP address, username, device) is skipped by the parser and never held.

use crate::model::{Origin, Play, Source};
use serde::Deserialize;

/// Spotify counts a stream from 30 seconds, so anything shorter was skipped.
const SKIP_UNDER_MS: i64 = 30_000;

#[derive(Default)]
pub struct Parsed {
    pub plays: Vec<Play>,
    pub excluded_private: u64,
    pub rejected: u64,
}

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

/// `StreamingHistory_music_*.json`, from the regular Account data.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    end_time: Option<String>,
    artist_name: Option<String>,
    track_name: Option<String>,
    ms_played: Option<i64>,
}

pub fn parse_extended(bytes: &[u8]) -> serde_json::Result<Parsed> {
    let mut out = Parsed::default();
    for r in serde_json::from_slice::<Vec<Extended>>(bytes)? {
        if r.incognito_mode == Some(true) {
            out.excluded_private += 1;
            continue;
        }
        // A podcast row has no track, and its show plays the part of the artist.
        out.push(
            r.ts.as_deref(),
            r.master_metadata_album_artist_name.or(r.episode_show_name),
            r.master_metadata_track_name.or(r.episode_name),
            r.spotify_track_uri.or(r.spotify_episode_uri),
            r.ms_played,
            r.skipped == Some(true),
        );
    }
    Ok(out)
}

pub fn parse_account(bytes: &[u8]) -> serde_json::Result<Parsed> {
    let mut out = Parsed::default();
    for r in serde_json::from_slice::<Vec<Account>>(bytes)? {
        out.push(
            r.end_time.as_deref(),
            r.artist_name,
            r.track_name,
            None,
            r.ms_played,
            false,
        );
    }
    Ok(out)
}

impl Parsed {
    /// A row needs a readable time, a Creator and an Item. Anything else (a video row, an
    /// audiobook) is counted as rejected.
    fn push(
        &mut self,
        time: Option<&str>,
        creator: Option<String>,
        item: Option<String>,
        item_ref: Option<String>,
        ms_played: Option<i64>,
        skipped: bool,
    ) {
        let (Some(played_at), Some(creator), Some(item)) =
            (time.and_then(parse_utc), creator, item)
        else {
            self.rejected += 1;
            return;
        };
        self.plays.push(Play {
            source: Source::Spotify,
            origin: Origin::Export,
            played_at,
            duration_ms: ms_played,
            creator,
            creator_ref: None,
            item,
            item_ref,
            skipped: skipped || ms_played.is_some_and(|ms| ms < SKIP_UNDER_MS),
        });
    }
}

/// `2024-03-01T13:05:42Z` or `2024-03-01 13:05` as Unix seconds. Spotify writes UTC in both and
/// the account format has no seconds. Anything after the seconds (fraction, `Z`) is ignored.
pub(crate) fn parse_utc(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let digits = b.get(from..to)?;
        digits
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| digits.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let shape_ok = b.get(4) == Some(&b'-')
        && b.get(7) == Some(&b'-')
        && matches!(b.get(10), Some(b'T' | b' '))
        && b.get(13) == Some(&b':');
    if !shape_ok {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute) = (num(11, 13)?, num(14, 16)?);
    let second = if b.get(16) == Some(&b':') {
        num(17, 19)?
    } else {
        0
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    // Days since 1970-01-01 from the civil date (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}
