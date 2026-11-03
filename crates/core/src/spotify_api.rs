//! Spotify's Web API: the last 50 Plays, and top artists with their genres. Opt-in, after consent that
//! quotes Spotify's Developer Policy (ADR 0002). An Export holds far more and is the recommended path.

use crate::error::{Error, Result};
use crate::ingest::parse_utc;
use crate::model::{Affinity, Origin, Play, Source};
use crate::oauth::{OAuthConfig, REDIRECT_PORT};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const API: &str = "https://api.spotify.com";
pub const TOP_ARTIST: &str = "top_artist";
/// The `origin` of an Interest that came from Spotify's genres.
pub const INTERESTS_ORIGIN: &str = "spotify_api";
/// Where the next sync starts, in Spotify's milliseconds.
const CURSOR: &str = "spotify_after";
const LONGEST_WAIT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub plays_added: u64,
    pub affinities: u64,
    pub interests: u64,
    /// Real history arrived, so the demo data went, as it does on an import.
    #[serde(default)]
    pub demo_cleared: bool,
}

/// Which Connections hold a token in the keychain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Connections {
    pub spotify: bool,
    pub youtube: bool,
}

/// Nothing is opened without the consent that quotes Spotify's policy, and a Client ID that looks like
/// one, since a typo would only show up as Spotify's own error page.
pub fn check_connect(client_id: &str, consent: bool) -> Result<()> {
    if !consent {
        return Err(Error::Connection(
            "Rostery will not connect to Spotify until you tick the box saying you have read what \
             Spotify's Developer Policy says."
                .into(),
        ));
    }
    let id = client_id.trim();
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Connection(
            "That does not look like a Spotify Client ID, which is 32 letters and digits. Copy it from \
             your app's settings in the Spotify developer dashboard."
                .into(),
        ));
    }
    Ok(())
}

pub fn oauth_config(client_id: &str) -> OAuthConfig {
    OAuthConfig {
        host: "Spotify".into(),
        auth_url: "https://accounts.spotify.com/authorize".into(),
        token_url: "https://accounts.spotify.com/api/token".into(),
        client_id: client_id.trim().into(),
        client_secret: None,
        scopes: vec!["user-read-recently-played".into(), "user-top-read".into()],
        extra_auth_params: vec![],
        port: REDIRECT_PORT,
    }
}

/// A GET that waits out one rate limit, for as long as Spotify asks, up to 30 seconds.
fn get(api: &str, path: &str, token: &str) -> Result<Value> {
    let agent = crate::deploy::agent();
    for attempt in 0..2 {
        let mut response = agent
            .get(format!("{}{path}", api.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {token}"))
            .call()
            .map_err(|e| {
                Error::Connection(format!(
                    "Rostery could not reach Spotify: {e}. Check your connection, then try again."
                ))
            })?;
        let status = response.status().as_u16();
        let wait = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok())
            .map_or(Duration::from_secs(1), Duration::from_secs);
        let body: Value =
            serde_json::from_str(&response.body_mut().read_to_string().unwrap_or_default())
                .unwrap_or_default();
        match status {
            200 => return Ok(body),
            // Nothing played recently.
            204 => return Ok(Value::Null),
            401 => return Err(Error::Connection(
                "Spotify says Rostery's connection has expired or was removed. Disconnect, then \
                     connect again."
                    .into(),
            )),
            429 if attempt == 0 => std::thread::sleep(wait.min(LONGEST_WAIT)),
            429 => break,
            _ => {
                return Err(Error::Connection(format!(
                    "Spotify answered {status}: {}. Try Sync again later.",
                    body["error"]["message"]
                        .as_str()
                        .unwrap_or("no reason given")
                )))
            }
        }
    }
    Err(Error::Connection(
        "Spotify is limiting how often Rostery can ask. Try Sync again in a few minutes.".into(),
    ))
}

/// New Plays since the last sync, and today's top artists in place of the last ones. Plays at or before
/// the newest Export Play are left out, because the Export already holds them.
pub fn sync(api: &str, access_token: &str, store: &mut Store, now: i64) -> Result<SyncSummary> {
    let cursor: Option<String> = store.get_setting(CURSOR)?;
    let path = match &cursor {
        Some(after) => format!("/v1/me/player/recently-played?limit=50&after={after}"),
        None => "/v1/me/player/recently-played?limit=50".into(),
    };
    let recent = get(api, &path, access_token)?;
    let exported_up_to = store.latest_played_at(Source::Spotify, Origin::Export)?;
    let plays: Vec<Play> = recent["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let track = &item["track"];
            let artist = &track["artists"][0];
            let played_at = parse_utc(item["played_at"].as_str()?)?;
            if exported_up_to.is_some_and(|newest| played_at <= newest) {
                return None;
            }
            Some(Play {
                source: Source::Spotify,
                origin: Origin::Api,
                played_at,
                duration_ms: track["duration_ms"].as_i64(),
                creator: artist["name"].as_str()?.to_owned(),
                creator_ref: artist["uri"].as_str().map(str::to_owned),
                item: track["name"].as_str()?.to_owned(),
                item_ref: track["uri"].as_str().map(str::to_owned),
                skipped: false,
            })
        })
        .collect();
    let plays_added = store.insert_plays(&plays)?.added;
    if let Some(after) = recent["cursors"]["after"].as_str() {
        store.set_setting(CURSOR, &after)?;
    }

    let top = get(
        api,
        "/v1/me/top/artists?limit=50&time_range=medium_term",
        access_token,
    )?;
    let artists: Vec<&Value> = top["items"].as_array().into_iter().flatten().collect();
    let affinities: Vec<Affinity> = artists
        .iter()
        .enumerate()
        .filter_map(|(i, artist)| {
            Some(Affinity {
                source: Source::Spotify,
                creator: artist["name"].as_str()?.to_owned(),
                creator_ref: artist["uri"].as_str().map(str::to_owned),
                kind: TOP_ARTIST.into(),
                // The first of 50 scores 500.
                score: (50 - i as i64) as f64 * 10.0,
                fetched_at: now,
            })
        })
        .collect();
    store.set_affinities(Source::Spotify, TOP_ARTIST, &affinities)?;
    let genres: Vec<(String, Vec<String>)> = artists
        .iter()
        .filter_map(|artist| {
            let tags: Vec<String> = artist["genres"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| Some(g.as_str()?.trim().to_lowercase()))
                .filter(|g| !g.is_empty())
                .collect();
            Some((artist["name"].as_str()?.to_owned(), tags))
        })
        .collect();
    let interests = store.replace_interests(Source::Spotify, INTERESTS_ORIGIN, &genres, now)?;
    Ok(SyncSummary {
        plays_added,
        affinities: affinities.len() as u64,
        interests,
        demo_cleared: false,
    })
}

/// Everything the Connection brought, and where the next sync would have started. Export Plays stay.
pub fn forget(store: &mut Store) -> Result<u64> {
    let removed = store.delete_api_data(Source::Spotify, &[TOP_ARTIST], INTERESTS_ORIGIN)?;
    store.delete_setting(CURSOR)?;
    Ok(removed)
}
