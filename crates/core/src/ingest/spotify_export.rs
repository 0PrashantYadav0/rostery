//! Spotify's two history formats. The row structs name only the fields Rostery keeps, so the
//! rest of a row (IP address, username, device) is skipped by the parser and never held.

use crate::model::{Origin, Play, Source};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::value::RawValue;

/// Spotify counts a stream from 30 seconds, so anything shorter was skipped.
const SKIP_UNDER_MS: i64 = 30_000;
/// Longer than any real stream. At this cap the stored total needs billions of rows to overflow.
const MAX_MS_PLAYED: i64 = 30 * 24 * 3_600_000;
/// 2006-01-01, the year Spotify was founded. Nothing earlier can be a real Play.
const EARLIEST_PLAY: i64 = 1_136_073_600;
/// A file value decides which API Plays get deleted, so a Play from the future is never believed.
/// A day of slack covers a computer whose clock runs a little behind.
const CLOCK_SLACK: i64 = 86_400;

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

/// The file must be a list. Each row is read on its own, so a row of the wrong shape costs that
/// row and not the file. `RawValue` only marks where a row is, which keeps the fields a row
/// struct does not name out of memory as parsed data.
fn each_row<T: DeserializeOwned>(
    bytes: &[u8],
    mut read: impl FnMut(&mut Parsed, T),
) -> serde_json::Result<Parsed> {
    let mut out = Parsed::default();
    for raw in serde_json::from_slice::<Vec<&RawValue>>(bytes)? {
        match serde_json::from_str(raw.get()) {
            Ok(row) => read(&mut out, row),
            Err(_) => out.rejected += 1,
        }
    }
    Ok(out)
}

pub fn parse_extended(bytes: &[u8], now: i64) -> serde_json::Result<Parsed> {
    each_row(bytes, |out, r: Extended| {
        if r.incognito_mode == Some(true) {
            out.excluded_private += 1;
            return;
        }
        // A podcast row has no track, and its show plays the part of the artist.
        out.keep(play(
            r.ts.as_deref(),
            r.master_metadata_album_artist_name.or(r.episode_show_name),
            r.master_metadata_track_name.or(r.episode_name),
            r.spotify_track_uri.or(r.spotify_episode_uri),
            r.ms_played,
            r.skipped == Some(true),
            now,
        ));
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

impl Parsed {
    fn keep(&mut self, play: Option<Play>) {
        match play {
            Some(play) => self.plays.push(play),
            None => self.rejected += 1,
        }
    }
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
    let played_at = parse_utc(time?).filter(|t| (EARLIEST_PLAY..=now + CLOCK_SLACK).contains(t))?;
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
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last_day = match month {
        2 => 28 + i64::from(leap),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=12).contains(&month)
        || !(1..=last_day).contains(&day)
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
