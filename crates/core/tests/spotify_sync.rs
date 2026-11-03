//! `spotify_api::sync` against a fake Spotify Web API.

use mockito::{Matcher, Mock, Server, ServerGuard};
use rostery_core::model::{Origin, Play, Source};
use rostery_core::spotify_api::{self, INTERESTS_ORIGIN, TOP_ARTIST};
use rostery_core::store::Store;
use serde_json::json;

const NOW: i64 = 1_800_000_000;
const TOKEN: &str = "at";

fn recent(
    server: &mut ServerGuard,
    query: Matcher,
    items: serde_json::Value,
    after: Option<&str>,
) -> Mock {
    server
        .mock("GET", "/v1/me/player/recently-played")
        .match_query(query)
        .match_header("authorization", "Bearer at")
        .with_body(
            json!({"items": items, "cursors": after.map(|a| json!({"after": a}))}).to_string(),
        )
        .create()
}

fn played(track: &str, artist: &str, at: &str) -> serde_json::Value {
    json!({
        "played_at": at,
        "track": {
            "name": track,
            "uri": format!("spotify:track:{}", track.replace(' ', "")),
            "duration_ms": 201_000,
            "artists": [{"name": artist, "uri": "spotify:artist:4Z8W4fKeB5YxbusRsdQVPb"}, {"name": "Guest"}],
        },
    })
}

fn top(server: &mut ServerGuard, artists: serde_json::Value) -> Mock {
    server
        .mock("GET", "/v1/me/top/artists")
        .match_query(Matcher::Any)
        .with_body(json!({"items": artists}).to_string())
        .create()
}

fn sync(server: &ServerGuard, store: &mut Store) -> spotify_api::SyncSummary {
    spotify_api::sync(&server.url(), TOKEN, store, NOW).unwrap()
}

#[test]
fn recent_plays_stored_with_api_origin() {
    let mut server = Server::new();
    let _recent = recent(
        &mut server,
        Matcher::Any,
        json!([played(
            "Low Tide",
            "Moth Season",
            "2027-01-10T22:15:03.123Z"
        )]),
        Some("1799999999000"),
    );
    let _top = top(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!(summary.plays_added, 1);
    let plays = store.plays(Some(Source::Spotify)).unwrap();
    assert_eq!(plays.len(), 1);
    let p = &plays[0];
    assert_eq!(p.origin, Origin::Api);
    assert_eq!(p.creator, "Moth Season");
    assert_eq!(p.item, "Low Tide");
    assert_eq!(p.duration_ms, Some(201_000));
    assert_eq!(p.item_ref.as_deref(), Some("spotify:track:LowTide"));
    assert_eq!(
        p.creator_ref.as_deref(),
        Some("spotify:artist:4Z8W4fKeB5YxbusRsdQVPb")
    );
}

#[test]
fn top_artists_become_affinities_with_genres() {
    let mut server = Server::new();
    let _recent = recent(&mut server, Matcher::Any, json!([]), None);
    let _top = top(
        &mut server,
        json!([
            {"name": "Moth Season", "uri": "spotify:artist:a", "genres": ["Dream Pop", " shoegaze "]},
            {"name": "Second", "uri": "spotify:artist:b", "genres": ["dream pop"]},
        ]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!(summary.affinities, 2);
    assert_eq!(summary.interests, 3);
    let affinities = store.affinities().unwrap();
    let first = affinities
        .iter()
        .find(|a| a.creator == "Moth Season")
        .unwrap();
    assert_eq!((first.kind.as_str(), first.score), (TOP_ARTIST, 500.0));
    let second = affinities.iter().find(|a| a.creator == "Second").unwrap();
    assert_eq!(second.score, 490.0);
    let interests = store.interests().unwrap();
    assert_eq!(
        interests[&(Source::Spotify, "Moth Season".to_owned())],
        ["dream pop", "shoegaze"]
    );
}

#[test]
fn second_sync_uses_after_cursor() {
    let mut server = Server::new();
    let first = recent(
        &mut server,
        Matcher::Regex("^limit=50$".into()),
        json!([]),
        Some("1799999999000"),
    );
    let _top = top(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    sync(&server, &mut store);
    first.assert();
    let second = recent(
        &mut server,
        Matcher::UrlEncoded("after".into(), "1799999999000".into()),
        json!([]),
        None,
    );
    sync(&server, &mut store);
    second.assert();
}

#[test]
fn plays_before_latest_export_are_skipped() {
    let mut server = Server::new();
    let _recent = recent(
        &mut server,
        Matcher::Any,
        json!([
            played("Old", "Moth Season", "2027-01-01T10:00:00Z"),
            played("New", "Moth Season", "2027-01-10T10:00:00Z"),
        ]),
        None,
    );
    let _top = top(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    let exported = Play {
        source: Source::Spotify,
        origin: Origin::Export,
        played_at: 1_799_000_000, // 2027-01-03
        duration_ms: Some(1000),
        creator: "Moth Season".into(),
        creator_ref: None,
        item: "Exported".into(),
        item_ref: None,
        skipped: false,
    };
    store.insert_plays(&[exported]).unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!(summary.plays_added, 1);
    let api: Vec<String> = store
        .plays(Some(Source::Spotify))
        .unwrap()
        .into_iter()
        .filter(|p| p.origin == Origin::Api)
        .map(|p| p.item)
        .collect();
    assert_eq!(api, ["New"]);
}

#[test]
fn missing_genres_are_fine() {
    let mut server = Server::new();
    let _recent = recent(&mut server, Matcher::Any, json!([]), None);
    let _top = top(
        &mut server,
        json!([{"name": "No Genres", "uri": "spotify:artist:c"}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = sync(&server, &mut store);
    assert_eq!((summary.affinities, summary.interests), (1, 0));
}

#[test]
fn rate_limit_respects_retry_after_once() {
    let mut server = Server::new();
    let limited = server
        .mock("GET", "/v1/me/player/recently-played")
        .match_query(Matcher::Any)
        .with_status(429)
        .with_header("retry-after", "1")
        .expect(1)
        .create();
    let _recent = recent(&mut server, Matcher::Any, json!([]), None);
    let _top = top(&mut server, json!([]));
    let mut store = Store::open_in_memory().unwrap();
    let started = std::time::Instant::now();
    sync(&server, &mut store);
    limited.assert();
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));

    // Twice in a row is a message, not an endless wait.
    let mut server = Server::new();
    let _always = server
        .mock("GET", "/v1/me/player/recently-played")
        .match_query(Matcher::Any)
        .with_status(429)
        .with_header("retry-after", "0")
        .create();
    let message = spotify_api::sync(&server.url(), TOKEN, &mut store, NOW)
        .unwrap_err()
        .to_string();
    assert!(message.contains("limiting"), "{message}");
}

#[test]
fn an_expired_token_says_connect_again() {
    let mut server = Server::new();
    let _gone = server
        .mock("GET", "/v1/me/player/recently-played")
        .match_query(Matcher::Any)
        .with_status(401)
        .create();
    let mut store = Store::open_in_memory().unwrap();
    let message = spotify_api::sync(&server.url(), TOKEN, &mut store, NOW)
        .unwrap_err()
        .to_string();
    assert!(message.contains("connect again"), "{message}");
}

#[test]
fn disconnect_deletes_api_data_only() {
    let mut server = Server::new();
    let _recent = recent(
        &mut server,
        Matcher::Any,
        json!([played(
            "From API",
            "Api Only Artist",
            "2027-01-10T10:00:00Z"
        )]),
        Some("1"),
    );
    let _top = top(
        &mut server,
        json!([{"name": "Api Only Artist", "uri": "spotify:artist:a", "genres": ["jazz"]}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let exported = Play {
        source: Source::Spotify,
        origin: Origin::Export,
        played_at: 1_700_000_000,
        duration_ms: Some(1000),
        creator: "Exported Artist".into(),
        creator_ref: None,
        item: "Exported".into(),
        item_ref: None,
        skipped: false,
    };
    store.insert_plays(&[exported]).unwrap();
    sync(&server, &mut store);
    // An AI tag for each, as a Roast would leave.
    for creator in ["Api Only Artist", "Exported Artist"] {
        store
            .set_interests(Source::Spotify, creator, &["night".into()], "ai", NOW)
            .unwrap();
    }

    let removed = spotify_api::forget(&mut store).unwrap();

    assert_eq!(removed, 1);
    let plays = store.plays(None).unwrap();
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].origin, Origin::Export);
    assert!(store.affinities().unwrap().is_empty());
    let interests = store.interests().unwrap();
    assert!(!interests.contains_key(&(Source::Spotify, "Api Only Artist".to_owned())));
    assert_eq!(
        interests[&(Source::Spotify, "Exported Artist".to_owned())],
        ["night"]
    );
    assert_eq!(store.get_setting::<String>("spotify_after").unwrap(), None);
}

#[test]
fn genres_stay_out_of_other_origins() {
    let mut server = Server::new();
    let _recent = recent(&mut server, Matcher::Any, json!([]), None);
    let _top = top(
        &mut server,
        json!([{"name": "A", "uri": "spotify:artist:a", "genres": ["jazz"]}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    store
        .set_interests(Source::Spotify, "A", &["night".into()], "ai", NOW)
        .unwrap();
    sync(&server, &mut store);
    let mut tags = store.interests().unwrap()[&(Source::Spotify, "A".to_owned())].clone();
    tags.sort();
    assert_eq!(tags, ["jazz", "night"]);
    let _ = INTERESTS_ORIGIN;
}

#[test]
fn connect_requires_consent() {
    let id = "0123456789abcdef0123456789abcdef";
    let message = spotify_api::check_connect(id, false)
        .unwrap_err()
        .to_string();
    assert!(message.contains("tick the box"), "{message}");
    assert!(spotify_api::check_connect("not an id", true).is_err());
    assert!(spotify_api::check_connect(&format!(" {id} "), true).is_ok());
}
