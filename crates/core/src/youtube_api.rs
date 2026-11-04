//! YouTube Data API v3: subscriptions and liked videos as Affinities, channel topics as Interests. It has
//! no watch history, so Takeout stays the real history (ADR 0002). YouTube's policies want stored API
//! data refreshed or deleted within 30 days, which `Store::purge_youtube_api_data` does at startup.

use crate::error::{Error, Result};
use crate::model::{Affinity, Source};
use crate::oauth::{OAuthConfig, REDIRECT_PORT};
use crate::spotify_api::SyncSummary;
use crate::store::Store;
use serde_json::Value;
use std::collections::BTreeMap;

pub const API: &str = "https://www.googleapis.com/youtube/v3";
pub const SUBSCRIPTION: &str = "subscription";
pub const LIKED: &str = "liked";
/// The `origin` of an Interest that came from a channel's topics.
pub const INTERESTS_ORIGIN: &str = "youtube_api";
pub const KINDS: [&str; 2] = [SUBSCRIPTION, LIKED];
const SUBSCRIPTION_SCORE: f64 = 60.0;
const LIKE_SCORE: f64 = 15.0;
const MAX_LIKES: usize = 200;
const MAX_SUBSCRIPTION_PAGES: usize = 40;

pub fn oauth_config(client_id: &str, client_secret: &str) -> OAuthConfig {
    OAuthConfig {
        host: "Google".into(),
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_url: "https://oauth2.googleapis.com/token".into(),
        client_id: client_id.trim().into(),
        // A desktop client's secret is not secret in Google's own terms, but it is the person's, so it
        // stays in the keychain.
        client_secret: Some(client_secret.trim().into()),
        scopes: vec!["https://www.googleapis.com/auth/youtube.readonly".into()],
        extra_auth_params: vec![
            ("access_type".into(), "offline".into()),
            ("prompt".into(), "consent".into()),
        ],
        port: REDIRECT_PORT,
    }
}

fn get(api: &str, path: &str, token: &str) -> Result<Value> {
    let mut response = crate::deploy::agent()
        .get(format!("{}{path}", api.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {token}"))
        .call()
        .map_err(|e| {
            Error::Connection(format!(
                "Rostery could not reach YouTube: {e}. Check your connection, then try again."
            ))
        })?;
    let status = response.status().as_u16();
    let body: Value =
        serde_json::from_str(&response.body_mut().read_to_string().unwrap_or_default())
            .unwrap_or_default();
    let reason = body["error"]["errors"][0]["reason"].as_str().unwrap_or("");
    match (status, reason) {
        (200, _) => Ok(body),
        (401, _) => Err(Error::Connection(
            "Google says Rostery's connection has expired or was removed. While your Google app is in \
             Testing, that happens every 7 days. Disconnect, then connect again."
                .into(),
        )),
        (403, "quotaExceeded" | "dailyLimitExceeded") => Err(Error::Connection(
            "Your Google app has used up its YouTube quota for today. Try Sync again tomorrow.".into(),
        )),
        (403, "accessNotConfigured" | "SERVICE_DISABLED") => Err(Error::Connection(
            "The YouTube Data API v3 is not turned on for your Google app. Enable it in the Google \
             Cloud console, then try again."
                .into(),
        )),
        _ => Err(Error::Connection(format!(
            "YouTube answered {status}: {}. Try Sync again later.",
            body["error"]["message"].as_str().unwrap_or("no reason given")
        ))),
    }
}

/// Every page of a list, up to `pages`.
fn all_pages(api: &str, path: &str, token: &str, pages: usize) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    let mut next: Option<String> = None;
    for _ in 0..pages {
        let page_path = match &next {
            Some(token) => format!("{path}&pageToken={token}"),
            None => path.to_owned(),
        };
        let page = get(api, &page_path, token)?;
        items.extend(page["items"].as_array().cloned().unwrap_or_default());
        next = page["nextPageToken"].as_str().map(str::to_owned);
        if next.is_none() {
            break;
        }
    }
    Ok(items)
}

fn channel_url(id: &str) -> String {
    format!("https://www.youtube.com/channel/{id}")
}

/// `https://en.wikipedia.org/wiki/Hip_hop_music` is `hip hop music`.
pub fn topic(url: &str) -> Option<String> {
    let name = url.rsplit('/').next()?;
    let decoded = url::form_urlencoded::parse(format!("t={name}").as_bytes())
        .next()?
        .1
        .replace('_', " ")
        .trim()
        .to_lowercase();
    (!decoded.is_empty()).then_some(decoded)
}

/// Subscriptions and liked videos in place of the last ones, and each of those channels' topics.
pub fn sync(api: &str, access_token: &str, store: &mut Store, now: i64) -> Result<SyncSummary> {
    let subscriptions = all_pages(
        api,
        "/subscriptions?part=snippet&mine=true&maxResults=50",
        access_token,
        MAX_SUBSCRIPTION_PAGES,
    )?;
    // Channel id to its title.
    let mut channels: BTreeMap<String, String> = BTreeMap::new();
    let subscribed: Vec<Affinity> = subscriptions
        .iter()
        .filter_map(|item| {
            let snippet = &item["snippet"];
            let id = snippet["resourceId"]["channelId"].as_str()?;
            let title = snippet["title"].as_str()?;
            channels.insert(id.to_owned(), title.to_owned());
            Some(Affinity {
                source: Source::Youtube,
                creator: title.to_owned(),
                creator_ref: Some(channel_url(id)),
                kind: SUBSCRIPTION.into(),
                score: SUBSCRIPTION_SCORE,
                fetched_at: now,
            })
        })
        .collect();

    let likes = all_pages(
        api,
        "/videos?part=snippet&myRating=like&maxResults=50",
        access_token,
        MAX_LIKES / 50,
    )?;
    let mut liked: BTreeMap<String, Affinity> = BTreeMap::new();
    for item in likes.iter().take(MAX_LIKES) {
        let snippet = &item["snippet"];
        let (Some(id), Some(title)) = (
            snippet["channelId"].as_str(),
            snippet["channelTitle"].as_str(),
        ) else {
            continue;
        };
        channels
            .entry(id.to_owned())
            .or_insert_with(|| title.to_owned());
        liked
            .entry(id.to_owned())
            .or_insert_with(|| Affinity {
                source: Source::Youtube,
                creator: title.to_owned(),
                creator_ref: Some(channel_url(id)),
                kind: LIKED.into(),
                score: 0.0,
                fetched_at: now,
            })
            .score += LIKE_SCORE;
    }
    let liked: Vec<Affinity> = liked.into_values().collect();

    // 50 ids is the most one request may ask about.
    let ids: Vec<&String> = channels.keys().collect();
    let mut topics: Vec<(String, Vec<String>)> = Vec::new();
    for batch in ids.chunks(50) {
        let joined: Vec<&str> = batch.iter().map(|id| id.as_str()).collect();
        let found = get(
            api,
            &format!(
                "/channels?part=topicDetails&id={}&maxResults=50",
                joined.join(",")
            ),
            access_token,
        )?;
        for channel in found["items"].as_array().into_iter().flatten() {
            let Some(title) = channel["id"].as_str().and_then(|id| channels.get(id)) else {
                continue;
            };
            let tags: Vec<String> = channel["topicDetails"]["topicCategories"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|url| topic(url.as_str()?))
                .collect();
            topics.push((title.clone(), tags));
        }
    }

    store.set_affinities(Source::Youtube, SUBSCRIPTION, &subscribed)?;
    store.set_affinities(Source::Youtube, LIKED, &liked)?;
    let interests = store.replace_interests(Source::Youtube, INTERESTS_ORIGIN, &topics, now)?;
    Ok(SyncSummary {
        plays_added: 0,
        affinities: (subscribed.len() + liked.len()) as u64,
        interests,
        demo_cleared: false,
    })
}

/// Everything the Connection brought. Takeout Plays stay.
pub fn forget(store: &mut Store) -> Result<u64> {
    store.delete_api_data(Source::Youtube, &KINDS, INTERESTS_ORIGIN)
}
