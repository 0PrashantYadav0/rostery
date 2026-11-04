//! `youtube_api::sync` against a fake YouTube Data API.

use mockito::{Matcher, Mock, Server, ServerGuard};
use rostery_core::model::{Origin, Play, Source};
use rostery_core::store::Store;
use rostery_core::youtube_api::{self, LIKED, SUBSCRIPTION};
use serde_json::json;

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn sub(id: &str, title: &str) -> serde_json::Value {
    json!({"snippet": {"title": title, "resourceId": {"channelId": id}}})
}

fn liked(id: &str, title: &str) -> serde_json::Value {
    json!({"snippet": {"channelId": id, "channelTitle": title}})
}

fn subscriptions(server: &mut ServerGuard, items: serde_json::Value) -> Mock {
    server
        .mock("GET", "/subscriptions")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("part".into(), "snippet".into()),
            Matcher::UrlEncoded("mine".into(), "true".into()),
            Matcher::UrlEncoded("maxResults".into(), "50".into()),
        ]))
        .with_body(json!({"items": items}).to_string())
        .create()
}

fn likes(server: &mut ServerGuard, items: serde_json::Value) -> Mock {
    server
        .mock("GET", "/videos")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("part".into(), "snippet".into()),
            Matcher::UrlEncoded("myRating".into(), "like".into()),
        ]))
        .with_body(json!({"items": items}).to_string())
        .create()
}

fn channels(server: &mut ServerGuard, items: serde_json::Value) -> Mock {
    server
        .mock("GET", "/channels")
        .match_query(Matcher::UrlEncoded("part".into(), "topicDetails".into()))
        .with_body(json!({"items": items}).to_string())
        .create()
}

fn sync(server: &ServerGuard, store: &mut Store) -> rostery_core::spotify_api::SyncSummary {
    youtube_api::sync(&server.url(), "at", store, NOW).unwrap()
}

#[test]
fn subscriptions_follow_pages() {
    let mut server = Server::new();
    let _page2 = server
        .mock("GET", "/subscriptions")
        .match_query(Matcher::UrlEncoded("pageToken".into(), "next".into()))
        .with_body(json!({"items": [sub("UC2", "Second Channel")]}).to_string())
        .create();
    let _page1 = server
        .mock("GET", "/subscriptions")
        .match_query(Matcher::Regex(
            "^part=snippet&mine=true&maxResults=50$".into(),
        ))
        .with_body(
            json!({"items": [sub("UC1", "First Channel")], "nextPageToken": "next"}).to_string(),
        )
        .create();
    let _likes = likes(&mut server, json!([]));
    let _channels = channels(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!(summary.affinities, 2);
    let affinities = store.affinities().unwrap();
    let names: Vec<&str> = affinities.iter().map(|a| a.creator.as_str()).collect();
    assert_eq!(names, ["First Channel", "Second Channel"]);
    assert!(affinities
        .iter()
        .all(|a| a.kind == SUBSCRIPTION && a.score == 60.0));
    assert_eq!(
        affinities[0].creator_ref.as_deref(),
        Some("https://www.youtube.com/channel/UC1")
    );
}

#[test]
fn liked_videos_become_affinities() {
    let mut server = Server::new();
    let _subs = subscriptions(&mut server, json!([]));
    let _likes = likes(
        &mut server,
        json!([
            liked("UC1", "Chess Cafe"),
            liked("UC1", "Chess Cafe"),
            liked("UC2", "Soup Lab")
        ]),
    );
    let _channels = channels(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    sync(&server, &mut store);
    let affinities = store.affinities().unwrap();
    let chess = affinities
        .iter()
        .find(|a| a.creator == "Chess Cafe")
        .unwrap();
    assert_eq!((chess.kind.as_str(), chess.score), (LIKED, 30.0));
    let soup = affinities.iter().find(|a| a.creator == "Soup Lab").unwrap();
    assert_eq!(soup.score, 15.0);
}

#[test]
fn likes_stop_at_200() {
    let mut server = Server::new();
    let _subs = subscriptions(&mut server, json!([]));
    let page: Vec<serde_json::Value> = (0..50).map(|_| liked("UC1", "Chess Cafe")).collect();
    let more = server
        .mock("GET", "/videos")
        .match_query(Matcher::Any)
        .with_body(json!({"items": page, "nextPageToken": "more"}).to_string())
        .expect(4)
        .create();
    let _channels = channels(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    sync(&server, &mut store);
    more.assert();
    assert_eq!(store.affinities().unwrap()[0].score, 200.0 * 15.0);
}

#[test]
fn topics_become_interests() {
    let mut server = Server::new();
    let _subs = subscriptions(&mut server, json!([sub("UC1", "Beat Shop")]));
    let _likes = likes(&mut server, json!([]));
    let _channels = channels(
        &mut server,
        json!([{"id": "UC1", "topicDetails": {"topicCategories": [
            "https://en.wikipedia.org/wiki/Hip_hop_music",
            "https://en.wikipedia.org/wiki/Caf%C3%A9_culture",
        ]}}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!(summary.interests, 2);
    let mut tags = store.interests().unwrap()[&(Source::Youtube, "Beat Shop".to_owned())].clone();
    tags.sort();
    assert_eq!(tags, ["café culture", "hip hop music"]);
    assert_eq!(
        youtube_api::topic("https://en.wikipedia.org/wiki/Jazz").as_deref(),
        Some("jazz")
    );
}

#[test]
fn channels_requested_in_batches_of_50() {
    let mut server = Server::new();
    let subs: Vec<serde_json::Value> = (0..120)
        .map(|i| sub(&format!("UC{i:03}"), &format!("Channel {i}")))
        .collect();
    let _subs = subscriptions(&mut server, json!(subs));
    let _likes = likes(&mut server, json!([]));
    let batches = server
        .mock("GET", "/channels")
        .match_query(Matcher::Regex(
            "id=(UC[0-9]{3}(%2C|,)){0,49}UC[0-9]{3}(&|$)".into(),
        ))
        .with_body(r#"{"items":[]}"#)
        .expect(3)
        .create();
    let mut store = Store::open_in_memory().unwrap();
    sync(&server, &mut store);
    batches.assert();
}

#[test]
fn quota_exceeded_is_readable() {
    let mut server = Server::new();
    let _subs = server
        .mock("GET", "/subscriptions")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_body(
            r#"{"error":{"code":403,"message":"quota","errors":[{"reason":"quotaExceeded"}]}}"#,
        )
        .create();
    let mut store = Store::open_in_memory().unwrap();
    let message = youtube_api::sync(&server.url(), "at", &mut store, NOW)
        .unwrap_err()
        .to_string();
    assert!(message.contains("quota for today"), "{message}");
}

#[test]
fn an_expired_token_says_connect_again() {
    let mut server = Server::new();
    let _subs = server
        .mock("GET", "/subscriptions")
        .match_query(Matcher::Any)
        .with_status(401)
        .create();
    let mut store = Store::open_in_memory().unwrap();
    let message = youtube_api::sync(&server.url(), "at", &mut store, NOW)
        .unwrap_err()
        .to_string();
    assert!(message.contains("connect again"), "{message}");
}

fn takeout_play() -> Play {
    Play {
        source: Source::Youtube,
        origin: Origin::Export,
        played_at: NOW - 400 * DAY,
        duration_ms: None,
        creator: "Watched Channel".into(),
        creator_ref: None,
        item: "A video".into(),
        item_ref: None,
        skipped: false,
    }
}

#[test]
fn purge_removes_old_api_data_only() {
    let mut store = Store::open_in_memory().unwrap();
    store.insert_plays(&[takeout_play()]).unwrap();
    let affinity = |creator: &str, days_old: i64| rostery_core::model::Affinity {
        source: Source::Youtube,
        creator: creator.into(),
        creator_ref: None,
        kind: SUBSCRIPTION.into(),
        score: 60.0,
        fetched_at: NOW - days_old * DAY,
    };
    store
        .set_affinities(
            Source::Youtube,
            SUBSCRIPTION,
            &[affinity("Old", 31), affinity("Recent", 29)],
        )
        .unwrap();
    store
        .set_interests(
            Source::Youtube,
            "Old",
            &["chess".into()],
            "youtube_api",
            NOW - 31 * DAY,
        )
        .unwrap();
    store
        .set_interests(
            Source::Youtube,
            "Old",
            &["night".into()],
            "ai",
            NOW - 31 * DAY,
        )
        .unwrap();
    store
        .set_interests(
            Source::Youtube,
            "Watched Channel",
            &["night".into()],
            "ai",
            NOW - 31 * DAY,
        )
        .unwrap();

    let removed = store.purge_youtube_api_data(NOW - 30 * DAY).unwrap();

    assert_eq!(removed, 2);
    let names: Vec<String> = store
        .affinities()
        .unwrap()
        .into_iter()
        .map(|a| a.creator)
        .collect();
    assert_eq!(names, ["Recent"]);
    let interests = store.interests().unwrap();
    assert!(!interests.contains_key(&(Source::Youtube, "Old".to_owned())));
    assert_eq!(
        interests[&(Source::Youtube, "Watched Channel".to_owned())],
        ["night"]
    );
    assert_eq!(store.plays(None).unwrap().len(), 1);
}

#[test]
fn disconnect_deletes_api_data_only() {
    let mut server = Server::new();
    let _subs = subscriptions(&mut server, json!([sub("UC1", "Beat Shop")]));
    let _likes = likes(&mut server, json!([liked("UC2", "Soup Lab")]));
    let _channels = channels(
        &mut server,
        json!([{"id": "UC1", "topicDetails": {"topicCategories": ["https://en.wikipedia.org/wiki/Jazz"]}}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    store.insert_plays(&[takeout_play()]).unwrap();
    sync(&server, &mut store);
    assert_eq!(store.affinities().unwrap().len(), 2);

    youtube_api::forget(&mut store).unwrap();

    assert!(store.affinities().unwrap().is_empty());
    assert!(store.interests().unwrap().is_empty());
    assert_eq!(store.plays(None).unwrap().len(), 1);
}
