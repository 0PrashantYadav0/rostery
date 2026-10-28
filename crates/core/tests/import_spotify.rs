//! Importing a Spotify Export. Every name and address here is invented.

use rostery_core::ingest::{import_path, ImportSummary};
use rostery_core::model::{Origin, Play, Source};
use rostery_core::store::Store;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

/// 2024-03-01 13:05:00 UTC.
const MINUTE: i64 = 1_709_298_300;

fn extended(ts: &str, artist: &str, track: &str, ms: i64) -> Value {
    json!({
        "ts": ts,
        "ms_played": ms,
        "master_metadata_album_artist_name": artist,
        "master_metadata_track_name": track,
        "spotify_track_uri": "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
        "skipped": false,
        "incognito_mode": false,
    })
}

fn write_json(dir: &Path, name: &str, rows: &Value) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_vec(rows).unwrap()).unwrap();
    path
}

fn write_zip(dir: &Path, name: &str, entries: &[(&str, Vec<u8>)]) -> PathBuf {
    let path = dir.join(name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (entry, bytes) in entries {
        zip.start_file(*entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Imports the rows as one extended-format file into a fresh in-memory Store.
fn import_rows(rows: Value) -> (Store, ImportSummary) {
    let dir = TempDir::new().unwrap();
    let file = write_json(dir.path(), "Streaming_History_Audio_2024_0.json", &rows);
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &file).unwrap();
    (store, summary)
}

#[test]
fn extended_row_maps_fields() {
    let (store, summary) = import_rows(json!([extended(
        "2024-03-01T13:05:42Z",
        "The Paper Lanterns",
        "Low Tide",
        200_000
    )]));
    assert_eq!((summary.files_read, summary.added), (1, 1));
    assert_eq!(
        store.plays(None).unwrap(),
        [Play {
            source: Source::Spotify,
            origin: Origin::Export,
            played_at: MINUTE + 42,
            duration_ms: Some(200_000),
            creator: "The Paper Lanterns".into(),
            creator_ref: None,
            item: "Low Tide".into(),
            item_ref: Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC".into()),
            skipped: false,
        }]
    );
}

#[test]
fn private_sessions_are_excluded() {
    let mut private = extended(
        "2024-03-01T13:09:00Z",
        "Velvet Anchor",
        "Quiet Hours",
        200_000,
    );
    private["incognito_mode"] = json!(true);
    let (store, summary) = import_rows(json!([
        extended(
            "2024-03-01T13:05:00Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        ),
        private,
    ]));
    assert_eq!(summary.excluded_private, 1);
    assert_eq!(summary.added, 1);
    let plays = store.plays(None).unwrap();
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].creator, "The Paper Lanterns");
}

#[test]
fn pii_never_reaches_disk() {
    let dir = TempDir::new().unwrap();
    let mut row = extended(
        "2024-03-01T13:05:00Z",
        "The Paper Lanterns",
        "Low Tide",
        200_000,
    );
    row["ip_addr"] = json!("203.0.113.7");
    row["username"] = json!("sentinel_user");
    let file = write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([row]),
    );

    let mut store = Store::open(&dir.path().join("data").join("rostery.db")).unwrap();
    import_path(&mut store, &file).unwrap();
    drop(store);

    // The database, its WAL and its shared-memory file.
    let mut scanned = 0;
    for entry in std::fs::read_dir(dir.path().join("data")).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        for secret in ["203.0.113.7", "sentinel_user"] {
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{secret} reached the database"
            );
        }
        scanned += bytes
            .windows(18)
            .filter(|w| *w == b"The Paper Lanterns")
            .count();
    }
    // The scan can see real data, so the empty result above means something.
    assert!(scanned > 0, "the control string was never found on disk");
}

#[test]
fn podcasts_use_show_as_creator() {
    let (store, _) = import_rows(json!([{
        "ts": "2024-03-01T13:05:00Z",
        "ms_played": 1_800_000,
        "master_metadata_album_artist_name": null,
        "master_metadata_track_name": null,
        "spotify_track_uri": null,
        "episode_show_name": "Slow Math Weekly",
        "episode_name": "Why Zero Matters",
        "spotify_episode_uri": "spotify:episode:7makk4oTQel546B0PZlDM5",
    }]));
    let play = &store.plays(None).unwrap()[0];
    assert_eq!(play.creator, "Slow Math Weekly");
    assert_eq!(play.item, "Why Zero Matters");
    assert_eq!(
        play.item_ref.as_deref(),
        Some("spotify:episode:7makk4oTQel546B0PZlDM5")
    );
}

#[test]
fn short_plays_flagged_skipped() {
    let mut flagged = extended(
        "2024-03-01T13:07:00Z",
        "Moth Season",
        "Soft Static",
        200_000,
    );
    flagged["skipped"] = json!(true);
    let (store, _) = import_rows(json!([
        extended(
            "2024-03-01T13:05:00Z",
            "Velvet Anchor",
            "Quiet Hours",
            12_000
        ),
        flagged,
        extended(
            "2024-03-01T13:09:00Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        ),
    ]));
    let skipped: Vec<bool> = store
        .plays(None)
        .unwrap()
        .iter()
        .map(|p| p.skipped)
        .collect();
    assert_eq!(skipped, [true, true, false]);
}

#[test]
fn account_format_parses_minutes() {
    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "StreamingHistory_music_0.json",
        &json!([{"endTime": "2024-03-01 13:05", "artistName": "A", "trackName": "T", "msPlayed": 200_000}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &file).unwrap();
    assert_eq!(summary.added, 1);
    let play = &store.plays(None).unwrap()[0];
    assert_eq!(play.played_at, MINUTE);
    assert_eq!(play.duration_ms, Some(200_000));
    assert_eq!((play.creator.as_str(), play.item.as_str()), ("A", "T"));
    assert_eq!(
        (play.source, play.origin),
        (Source::Spotify, Origin::Export)
    );
}

#[test]
fn account_and_extended_do_not_double_count() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([extended("2024-03-01T13:05:42Z", "A", "T", 200_000)]),
    );
    write_json(
        dir.path(),
        "StreamingHistory_music_0.json",
        &json!([{"endTime": "2024-03-01 13:05", "artistName": "A", "trackName": "T", "msPlayed": 200_000}]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, dir.path()).unwrap();
    assert_eq!(
        (summary.files_read, summary.added, summary.duplicates),
        (2, 1, 1)
    );
    assert_eq!(store.plays(None).unwrap().len(), 1);
}

#[test]
fn importing_twice_adds_nothing() {
    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([extended("2024-03-01T13:05:42Z", "A", "T", 200_000)]),
    );
    let mut store = Store::open_in_memory().unwrap();
    import_path(&mut store, &file).unwrap();
    let again = import_path(&mut store, &file).unwrap();
    assert_eq!((again.added, again.duplicates), (0, 1));
    assert_eq!(store.plays(None).unwrap().len(), 1);
}

#[test]
fn zip_reads_only_history() {
    let dir = TempDir::new().unwrap();
    let history = serde_json::to_vec(&json!([extended(
        "2024-03-01T13:05:00Z",
        "The Paper Lanterns",
        "Low Tide",
        200_000
    )]))
    .unwrap();
    let userdata = br#"{"username":"sentinel_user","email":"nobody@example.com"}"#.to_vec();
    let zip = write_zip(
        dir.path(),
        "my_spotify_data.zip",
        &[
            ("Spotify Extended Streaming History/Userdata.json", userdata),
            (
                "Spotify Extended Streaming History/Streaming_History_Audio_2024_0.json",
                history,
            ),
            (
                "Spotify Extended Streaming History/ReadMeFirst.pdf",
                b"%PDF".to_vec(),
            ),
        ],
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &zip).unwrap();
    assert_eq!((summary.files_read, summary.added), (1, 1));
}

#[test]
fn rows_missing_time_are_rejected() {
    let mut no_time = extended("x", "Moth Season", "Soft Static", 200_000);
    no_time.as_object_mut().unwrap().remove("ts");
    let (store, summary) = import_rows(json!([
        extended(
            "2024-03-01T13:05:00Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        ),
        no_time,
        extended(
            "2024-03-01T13:09:00Z",
            "Velvet Anchor",
            "Quiet Hours",
            200_000
        ),
    ]));
    assert_eq!((summary.rejected, summary.added), (1, 2));
    assert_eq!(store.plays(None).unwrap().len(), 2);
}

#[test]
fn rows_with_a_bad_time_or_no_item_are_rejected() {
    let video = json!({"ts": "2024-03-01T13:11:00Z", "ms_played": 5000,
        "master_metadata_track_name": null, "episode_name": null});
    let mut number_time = extended("x", "Moth Season", "Soft Static", 200_000);
    number_time["ts"] = json!(12345);
    let mut text_duration = extended(
        "2024-03-01T13:13:00Z",
        "Moth Season",
        "Soft Static",
        200_000,
    );
    text_duration["ms_played"] = json!("abc");
    let (store, summary) = import_rows(json!([
        extended("last Tuesday", "Moth Season", "Soft Static", 200_000),
        extended(
            "2024-13-45T25:61:00Z",
            "Moth Season",
            "Soft Static",
            200_000
        ),
        video,
        number_time,
        text_duration,
        42,
        extended(
            "2024-03-01T13:05:00Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        ),
    ]));
    assert_eq!((summary.rejected, summary.added), (6, 1));
    assert_eq!(store.plays(None).unwrap()[0].item, "Low Tide");
}

/// Imports the rows as one account-format file into a fresh in-memory Store.
fn import_account_rows(rows: Value) -> (Store, ImportSummary) {
    let dir = TempDir::new().unwrap();
    let file = write_json(dir.path(), "StreamingHistory_music_0.json", &rows);
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &file).unwrap();
    (store, summary)
}

#[test]
fn account_rows_of_the_wrong_type_are_rejected() {
    let (_, summary) = import_account_rows(json!([
        {"endTime": "2024-03-01 13:05", "artistName": "A", "trackName": "T", "msPlayed": 200_000},
        {"endTime": 12345, "artistName": "A", "trackName": "T", "msPlayed": 200_000},
        "not a row",
    ]));
    assert_eq!((summary.rejected, summary.added), (2, 1));
}

#[test]
fn account_podcasts_use_show_as_creator() {
    let (store, summary) = import_account_rows(json!([{
        "endTime": "2024-03-01 13:05",
        "podcastName": "Slow Math Weekly",
        "episodeName": "Why Zero Matters",
        "msPlayed": 1_800_000,
    }]));
    assert_eq!((summary.rejected, summary.added), (0, 1));
    let play = &store.plays(None).unwrap()[0];
    assert_eq!(play.creator, "Slow Math Weekly");
    assert_eq!(play.item, "Why Zero Matters");
}

#[test]
fn durations_out_of_range_are_rejected() {
    const THIRTY_DAYS_MS: i64 = 2_592_000_000;
    let (store, summary) = import_rows(json!([
        extended("2024-03-01T13:05:00Z", "A", "T", i64::MAX),
        extended("2024-03-01T13:06:00Z", "A", "U", i64::MAX),
        extended("2024-03-01T13:07:00Z", "A", "V", -6_000_000_000_i64),
        extended("2024-03-01T13:08:00Z", "A", "W", -1),
        extended("2024-03-01T13:09:00Z", "A", "X", THIRTY_DAYS_MS + 1),
        extended("2024-03-01T13:10:00Z", "A", "Y", THIRTY_DAYS_MS),
        extended("2024-03-01T13:11:00Z", "A", "Z", 0),
    ]));
    assert_eq!((summary.rejected, summary.added), (5, 2));
    // Before the range check this failed with an integer overflow, on every launch.
    assert_eq!(store.totals().unwrap().spotify.minutes, 43_200);
}

#[test]
fn impossible_or_unbelievable_dates_are_rejected() {
    let two_days_ahead = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 2 * 86_400;
    for ts in [
        "2024-02-31T00:00:00Z".to_string(),
        "2023-02-29T00:00:00Z".to_string(),
        "2024-04-31T00:00:00Z".to_string(),
        "0000-01-01T00:00:00Z".to_string(),
        "1999-12-31T23:59:59Z".to_string(),
        "9999-12-31T23:59:59Z".to_string(),
        format_ts(two_days_ahead),
    ] {
        let (store, summary) = import_rows(json!([extended(&ts, "A", "T", 200_000)]));
        assert_eq!((summary.rejected, summary.added), (1, 0), "{ts}");
        assert!(store.plays(None).unwrap().is_empty(), "{ts}");
    }
    let (_, leap_day) = import_rows(json!([extended("2024-02-29T12:00:00Z", "A", "T", 200_000)]));
    assert_eq!((leap_day.rejected, leap_day.added), (0, 1));
}

#[test]
fn a_far_future_row_does_not_delete_api_plays() {
    let api = |at: i64, creator: &str| Play {
        source: Source::Spotify,
        origin: Origin::Api,
        played_at: at,
        duration_ms: None,
        creator: creator.into(),
        creator_ref: None,
        item: "Song".into(),
        item_ref: None,
        skipped: false,
    };
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[
            api(1_700_000_000, "Older Api Band"),
            api(1_790_000_000, "Newer Api Band"),
        ])
        .unwrap();
    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([extended("9999-12-31T23:59:59Z", "Forged", "Row", 200_000)]),
    );
    let summary = import_path(&mut store, &file).unwrap();
    assert_eq!((summary.rejected, summary.added), (1, 0));
    let left: Vec<String> = store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| p.creator)
        .collect();
    assert_eq!(left, ["Older Api Band", "Newer Api Band"]);
}

#[test]
fn unrecognised_input_explains_what_to_do() {
    let dir = TempDir::new().unwrap();
    let file = write_json(dir.path(), "random.json", &json!({"hello": "world"}));
    let mut store = Store::open_in_memory().unwrap();
    for input in [file.as_path(), dir.path()] {
        let message = import_path(&mut store, input).unwrap_err().to_string();
        assert!(message.contains("Spotify"), "{message}");
        assert!(message.contains("Google Takeout"), "{message}");
    }
}

#[test]
fn export_replaces_overlapping_api_plays() {
    let api = |at: i64, creator: &str| Play {
        source: Source::Spotify,
        origin: Origin::Api,
        played_at: at,
        duration_ms: None,
        creator: creator.into(),
        creator_ref: None,
        item: "Song".into(),
        item_ref: None,
        skipped: false,
    };
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[
            api(MINUTE - 3600, "Older Api Band"),
            api(MINUTE + 3600, "Newer Api Band"),
        ])
        .unwrap();

    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([extended(
            "2024-03-01T13:05:00Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        )]),
    );
    import_path(&mut store, &file).unwrap();

    let left: Vec<(String, Origin)> = store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| (p.creator, p.origin))
        .collect();
    assert_eq!(
        left,
        [
            ("The Paper Lanterns".to_string(), Origin::Export),
            ("Newer Api Band".to_string(), Origin::Api),
        ]
    );
}

#[test]
fn export_keeps_the_api_play_it_repeats() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[Play {
            source: Source::Spotify,
            origin: Origin::Api,
            played_at: MINUTE + 30,
            duration_ms: None,
            creator: "The Paper Lanterns".into(),
            creator_ref: None,
            item: "Low Tide".into(),
            item_ref: None,
            skipped: false,
        }])
        .unwrap();

    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([extended(
            "2024-03-01T13:05:42Z",
            "The Paper Lanterns",
            "Low Tide",
            200_000
        )]),
    );
    import_path(&mut store, &file).unwrap();

    // The Export's copy takes over, so the Play is neither doubled nor lost.
    let plays = store.plays(None).unwrap();
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].origin, Origin::Export);
    assert_eq!(plays[0].duration_ms, Some(200_000));
}

/// A heavy listener: 300,000 rows over 3 files in one zip.
/// Run with `cargo test --release -p rostery-core --test import_spotify -- --ignored import_300k`.
#[test]
#[ignore]
fn import_300k() {
    let dir = TempDir::new().unwrap();
    let mut entries = Vec::new();
    for file in 0..3_i64 {
        let rows: Vec<Value> = (0..100_000_i64)
            .map(|i| {
                let n = file * 100_000 + i;
                // One minute apart, so none collide.
                let ts = MINUTE + n * 60;
                let mut row = extended(
                    &format_ts(ts),
                    &format!("Artist {}", n % 700),
                    &format!("Track {n}"),
                    180_000,
                );
                row["username"] = json!("sentinel_user");
                row["ip_addr"] = json!("203.0.113.7");
                row["platform"] = json!("OS X 14.3.0 [arm 2]");
                row["conn_country"] = json!("GB");
                row["master_metadata_album_album_name"] = json!(format!("Album {}", n % 3000));
                row["reason_start"] = json!("trackdone");
                row["reason_end"] = json!("trackdone");
                row
            })
            .collect();
        entries.push((
            format!("Spotify Extended Streaming History/Streaming_History_Audio_{file}.json"),
            serde_json::to_vec(&rows).unwrap(),
        ));
    }
    let borrowed: Vec<(&str, Vec<u8>)> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.clone()))
        .collect();
    let zip = write_zip(dir.path(), "my_spotify_data.zip", &borrowed);

    let mut store = Store::open(&dir.path().join("rostery.db")).unwrap();
    let start = Instant::now();
    let summary = import_path(&mut store, &zip).unwrap();
    let took = start.elapsed();

    assert_eq!((summary.files_read, summary.added), (3, 300_000));
    eprintln!("imported 300,000 Plays in {took:?}");
    assert!(took.as_secs() < 30, "import took {took:?}");
}

/// Seconds since the epoch back to `YYYY-MM-DDTHH:MM:SSZ`, via the civil-from-days algorithm.
fn format_ts(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
