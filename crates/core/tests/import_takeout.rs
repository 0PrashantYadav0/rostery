//! Importing a Google Takeout Export. Every name and address here is invented.

use rostery_core::ingest::{import_path, import_paths, ImportSummary};
use rostery_core::model::{Origin, Play, Source};
use rostery_core::store::Store;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// 2024-03-01 13:05:00 UTC.
const MINUTE: i64 = 1_709_298_300;
const TIME: &str = "2024-03-01T13:05:42.123Z";
const CHANNEL_URL: &str = "https://www.youtube.com/channel/UCnightowlradio";

/// A row as Takeout writes it, with a Creator. Tests take away what they do not want.
fn row(title: &str, url: &str, time: &str) -> Value {
    json!({
        "header": "YouTube",
        "title": title,
        "titleUrl": url,
        "subtitles": [{"name": "Night Owl Radio", "url": CHANNEL_URL}],
        "time": time,
        "products": ["YouTube"],
        "activityControls": ["YouTube watch history"],
    })
}

fn watch(video: &str, time: &str) -> Value {
    row(
        &format!("Watched {video}"),
        &format!("https://www.youtube.com/watch?v={}", video.replace(' ', "")),
        time,
    )
}

fn without(mut row: Value, field: &str) -> Value {
    row.as_object_mut().unwrap().remove(field);
    row
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

/// Imports the rows as one Takeout file into a fresh in-memory Store.
fn import_rows(rows: Value) -> (Store, ImportSummary) {
    let dir = TempDir::new().unwrap();
    let file = write_json(dir.path(), "watch-history.json", &rows);
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &file).unwrap();
    (store, summary)
}

fn items(store: &Store) -> Vec<String> {
    store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| p.item)
        .collect()
}

fn api_play(source: Source, at: i64, creator: &str) -> Play {
    Play {
        source,
        origin: Origin::Api,
        played_at: at,
        duration_ms: None,
        creator: creator.into(),
        creator_ref: None,
        item: "Some Item".into(),
        item_ref: None,
        skipped: false,
    }
}

fn creators(store: &Store) -> Vec<String> {
    store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| p.creator)
        .collect()
}

/// A page the way Takeout writes it: a long stylesheet, a header naming the product, then links.
/// Written from what parsers of these pages select, not copied from a real file.
fn activity_html(product: &str) -> Vec<u8> {
    format!(
        "<html><head><style>{}</style></head><body>\
         <div class=\"outer-cell mdl-cell mdl-cell--12-col\"><div class=\"mdl-grid\">\
         <div class=\"header-cell mdl-cell mdl-cell--12-col\">\
         <p class=\"mdl-typography--title\">{product}<br></p></div>\
         <div class=\"content-cell mdl-cell mdl-cell--6-col mdl-typography--body-1\">\
         Watched <a href=\"https://www.youtube.com/watch?v=abc123\">Night Drive Mix</a></div>\
         </div></div></body></html>",
        ".mdl-cell { margin: 8px; }\n".repeat(400)
    )
    .into_bytes()
}

/// HTML is the default format for YouTube history.
fn takeout_html() -> Vec<u8> {
    activity_html("YouTube")
}

/// Every Takeout carries this index page, whatever format was chosen. It lists no Plays.
const INDEX_PAGE: &[u8] =
    b"<html><body><h1>Archive for you</h1><p>YouTube and YouTube Music</p></body></html>";

#[test]
fn watch_entry_maps_fields() {
    let (store, summary) = import_rows(json!([row(
        "Watched Night Drive Mix",
        "https://www.youtube.com/watch?v=abc123",
        TIME
    )]));
    assert_eq!((summary.files_read, summary.added), (1, 1));
    assert_eq!(
        store.plays(None).unwrap(),
        [Play {
            source: Source::Youtube,
            origin: Origin::Export,
            played_at: MINUTE + 42,
            duration_ms: None,
            creator: "Night Owl Radio".into(),
            creator_ref: Some(CHANNEL_URL.into()),
            item: "Night Drive Mix".into(),
            item_ref: Some("abc123".into()),
            skipped: false,
        }]
    );
}

#[test]
fn only_a_youtube_channel_link_becomes_a_creator_ref() {
    let too_long = format!("https://www.youtube.com/channel/{}", "a".repeat(65));
    let huge = "x".repeat(1 << 20);
    for url in [
        "javascript:alert(document.domain)",
        "https://phish.example/login",
        "https://phish.example/www.youtube.com/channel/UCnightowlradio",
        "https://www.youtube.com.phish.example/channel/UCnightowlradio",
        "https://www.youtube.com/channel/",
        "https://www.youtube.com/channel/UCnightowlradio/community",
        "https://www.youtube.com/channel/\"><script>",
        "https://www.youtube.com/@nightowlradio",
        &too_long,
        &huge,
    ] {
        let mut forged = watch("Low Tide", TIME);
        forged["subtitles"][0]["url"] = json!(url);
        let (store, summary) = import_rows(json!([forged]));
        let shown = &url[..url.len().min(70)];
        assert_eq!((summary.added, summary.rejected), (1, 0), "{shown}");
        let play = &store.plays(None).unwrap()[0];
        assert_eq!(play.creator_ref, None, "{shown}");
        // A bad link costs the link, not the Creator.
        assert_eq!(play.creator, "Night Owl Radio", "{shown}");
    }
}

#[test]
fn a_channel_link_is_stored_in_one_form() {
    for url in [
        CHANNEL_URL,
        "http://youtube.com/channel/UCnightowlradio",
        "https://m.youtube.com/channel/UCnightowlradio",
        "https://music.youtube.com/channel/UCnightowlradio",
    ] {
        let mut good = watch("Low Tide", TIME);
        good["subtitles"][0]["url"] = json!(url);
        let (store, _) = import_rows(json!([good]));
        assert_eq!(
            store.plays(None).unwrap()[0].creator_ref.as_deref(),
            Some(CHANNEL_URL),
            "{url}"
        );
    }
}

#[test]
fn ads_are_excluded() {
    let mut ad = row(
        "Watched Buy More Socks",
        "https://www.youtube.com/watch?v=adad01",
        "2024-03-01T13:09:00.000Z",
    );
    ad["details"] = json!([{"name": "From Google Ads"}]);
    let (store, summary) = import_rows(json!([watch("Night Drive Mix", TIME), ad]));
    assert_eq!(
        (summary.excluded_ads, summary.added, summary.rejected),
        (1, 1, 0)
    );
    assert_eq!(items(&store), ["Night Drive Mix"]);
}

#[test]
fn an_ad_with_a_translated_label_or_no_link_is_still_an_ad() {
    let mut translated = watch("Buy More Socks", TIME);
    translated["details"] = json!([{"name": "De Google Ads"}]);
    let mut no_link = without(watch("Buy More Hats", TIME), "titleUrl");
    no_link["details"] = json!([{"name": "From Google Ads"}]);
    let (store, summary) = import_rows(json!([translated, no_link]));
    assert_eq!((summary.excluded_ads, summary.added), (2, 0));
    assert!(store.plays(None).unwrap().is_empty());
}

#[test]
fn deleted_videos_have_empty_creator() {
    let mut empty_list = watch("Lost Tape", "2024-03-01T13:06:00.000Z");
    empty_list["subtitles"] = json!([]);
    let (store, summary) = import_rows(json!([
        without(
            row(
                "Watched https://www.youtube.com/watch?v=gone01",
                "https://www.youtube.com/watch?v=gone01",
                TIME
            ),
            "subtitles"
        ),
        empty_list,
    ]));
    assert_eq!((summary.added, summary.rejected), (2, 0));
    let plays = store.plays(None).unwrap();
    assert!(plays
        .iter()
        .all(|p| p.creator.is_empty() && p.creator_ref.is_none()));
    assert_eq!(plays[0].item_ref.as_deref(), Some("gone01"));
    // They are Plays, but nobody's Creator.
    let totals = store.totals().unwrap();
    assert_eq!((totals.youtube.plays, totals.youtube.creators), (2, 0));
}

#[test]
fn a_removed_video_has_no_link_and_is_still_a_play() {
    let removed = without(
        without(
            row("Watched a video that has been removed", "", TIME),
            "titleUrl",
        ),
        "subtitles",
    );
    let translated = without(
        without(
            row(
                "Vous avez regardé une vidéo supprimée",
                "",
                "2024-03-01T13:06:00.000Z",
            ),
            "titleUrl",
        ),
        "subtitles",
    );
    let (store, summary) = import_rows(json!([removed, translated]));
    assert_eq!((summary.added, summary.rejected), (2, 0));
    let plays = store.plays(None).unwrap();
    assert_eq!(plays[0].item, "a video that has been removed");
    assert_eq!(plays[1].item, "Vous avez regardé une vidéo supprimée");
    assert!(plays
        .iter()
        .all(|p| p.creator.is_empty() && p.item_ref.is_none()));
}

#[test]
fn a_file_of_only_removed_videos_is_still_found() {
    let (store, summary) = import_rows(json!([without(
        without(
            row("Watched a video that has been removed", "", TIME),
            "titleUrl"
        ),
        "subtitles"
    )]));
    assert_eq!(summary.added, 1);
    assert_eq!(store.plays(None).unwrap().len(), 1);
}

#[test]
fn topic_suffix_is_removed() {
    let mut music = watch("Low Tide", TIME);
    music["subtitles"][0]["name"] = json!("Some Band - Topic");
    let (store, _) = import_rows(json!([music]));
    assert_eq!(store.plays(None).unwrap()[0].creator, "Some Band");
}

#[test]
fn non_watch_activity_is_skipped() {
    let (store, summary) = import_rows(json!([
        // No link, like a removed video, but it says it is not a watch.
        without(
            row("Viewed a post that is no longer available", "", TIME),
            "titleUrl"
        ),
        row(
            "Searched for slow math",
            "https://www.youtube.com/results?search_query=slow+math",
            TIME
        ),
        row(
            "Viewed a post by Night Owl Radio",
            "https://www.youtube.com/post/UgkxFakePost",
            TIME
        ),
        row(
            "Viewed a community post",
            "https://www.youtube.com/channel/UCnightowlradio/community?lb=UgkxFakePost",
            TIME
        ),
        row(
            "Visited the feed",
            "https://www.youtube.com/feed/history",
            TIME
        ),
        row(
            "Watched nothing",
            "https://www.youtube.com/watch?list=PL1",
            TIME
        ),
        row("Watched nothing", "https://www.youtube.com/shorts/", TIME),
        row(
            "Watched forged",
            "https://www.youtube.com/watch?v=\"><script>",
            TIME
        ),
        row(
            "Watched forged",
            &format!("https://www.youtube.com/watch?v={}", "a".repeat(65)),
            TIME
        ),
        row(
            "Watched forged",
            &format!("https://www.youtube.com/shorts/{}", "a".repeat(65)),
            TIME
        ),
        row(
            "Watched elsewhere",
            "https://www.example.com/watch?v=abc123",
            TIME
        ),
    ]));
    assert_eq!(
        (summary.files_read, summary.added, summary.rejected),
        (1, 0, 0)
    );
    assert!(store.plays(None).unwrap().is_empty());
}

#[test]
fn subscribing_or_liking_is_not_a_watch() {
    let (store, summary) = import_rows(json!([
        row("Subscribed to Night Owl Radio", CHANNEL_URL, TIME),
        // A like links to the video, so the link alone cannot tell it from a watch.
        row(
            "Liked Night Drive Mix",
            "https://www.youtube.com/watch?v=abc123",
            TIME
        ),
        row(
            "Commented on Night Drive Mix",
            "https://www.youtube.com/watch?v=abc123&lc=Ugzfake",
            TIME
        ),
    ]));
    assert_eq!((summary.added, summary.rejected), (0, 0));
    assert!(store.plays(None).unwrap().is_empty());
}

#[test]
fn a_video_named_like_an_activity_is_still_a_watch() {
    let (store, _) = import_rows(json!([row(
        "Watched Liked by Nobody",
        "https://www.youtube.com/watch?v=abc123",
        TIME
    )]));
    assert_eq!(items(&store), ["Liked by Nobody"]);
}

#[test]
fn other_google_products_are_skipped() {
    // Chrome's own activity links straight to YouTube, and only `products` says it is not a watch.
    let mut chrome = row(
        "Visited Night Drive Mix - YouTube",
        "https://www.youtube.com/watch?v=abc123",
        TIME,
    );
    chrome["header"] = json!("Chrome");
    chrome["products"] = json!(["Chrome"]);
    let mut other = row("Saw an ad", "https://www.youtube.com/watch?v=adad01", TIME);
    other["products"] = json!(["Ads"]);
    other["details"] = json!([{"name": "From Google Ads"}]);
    let (store, summary) = import_rows(json!([chrome, other]));
    assert_eq!(
        (summary.added, summary.excluded_ads, summary.rejected),
        (0, 0, 0)
    );
    assert!(store.plays(None).unwrap().is_empty());
}

#[test]
fn shorts_urls_count_as_watches() {
    let (store, summary) = import_rows(json!([
        row(
            "Watched Tiny Compilers",
            "https://www.youtube.com/shorts/sh0rt1",
            "2024-03-01T13:05:00.000Z"
        ),
        row(
            "Watched Tiny Linkers",
            "https://www.youtube.com/shorts/sh0rt2?feature=share",
            "2024-03-01T13:06:00.000Z"
        ),
    ]));
    assert_eq!(summary.added, 2);
    let refs: Vec<Option<String>> = store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| p.item_ref)
        .collect();
    assert_eq!(
        refs,
        [Some("sh0rt1".to_string()), Some("sh0rt2".to_string())]
    );
}

#[test]
fn the_video_id_is_found_wherever_the_link_puts_it() {
    for (url, id) in [
        (
            "https://www.youtube.com/watch?list=PL1&v=abc123&t=9s",
            "abc123",
        ),
        ("https://music.youtube.com/watch?v=abc123", "abc123"),
        ("https://m.youtube.com/watch?v=abc123", "abc123"),
        ("http://youtube.com/watch?v=abc123", "abc123"),
        ("https://www.youtube.com/watch?v=abc123#t=9", "abc123"),
        ("https://www.youtube.com/watch?v=a-b_c1", "a-b_c1"),
    ] {
        let (store, _) = import_rows(json!([row("Watched Low Tide", url, TIME)]));
        assert_eq!(
            store.plays(None).unwrap()[0].item_ref.as_deref(),
            Some(id),
            "{url}"
        );
    }
}

/// What a real watch history and My Activity file hold, down to how Takeout escapes `=` in a link.
const REALISTIC_MIX: &str = r#"[
  {"header":"YouTube Music","title":"Watched Low Tide","titleUrl":"https://music.youtube.com/watch?v=music01",
   "subtitles":[{"name":"The Paper Lanterns - Topic","url":"https://www.youtube.com/channel/UCpaperlanterns"}],
   "time":"2024-03-01T13:09:00.100Z","products":["YouTube"],"activityControls":["YouTube watch history"]},
  {"header":"YouTube","title":"Watched Tiny Compilers","titleUrl":"https://www.youtube.com/shorts/short01",
   "subtitles":[{"name":"Cache Miss","url":"https://www.youtube.com/channel/UCcachemiss"}],
   "time":"2024-03-01T13:08:00.200Z","products":["YouTube"],"activityControls":["YouTube watch history"]},
  {"header":"YouTube","title":"Subscribed to Cache Miss","titleUrl":"https://www.youtube.com/channel/UCcachemiss",
   "time":"2024-03-01T13:07:30.000Z","products":["YouTube"]},
  {"header":"YouTube","title":"Liked Why Zero Matters","titleUrl":"https://www.youtube.com/watch?v=math01",
   "time":"2024-03-01T13:07:00.000Z","products":["YouTube"]},
  {"header":"YouTube","title":"Viewed a post by Slow Math Club","titleUrl":"https://www.youtube.com/post/UgkxFakePost",
   "time":"2024-03-01T13:06:45.000Z","products":["YouTube"]},
  {"header":"YouTube","title":"Viewed a community post","titleUrl":"https://www.youtube.com/channel/UCslowmath/community?lb\u003dUgkxFakePost",
   "time":"2024-03-01T13:06:40.000Z","products":["YouTube"]},
  {"header":"YouTube","title":"Watched Why Zero Matters","titleUrl":"https://www.youtube.com/watch?v=math01",
   "subtitles":[{"name":"Slow Math Club","url":"https://www.youtube.com/channel/UCslowmath"}],
   "time":"2024-03-01T13:06:30.300Z","products":["YouTube"],"activityControls":["YouTube watch history"]},
  {"header":"YouTube","title":"Watched Buy More Socks","titleUrl":"https://www.youtube.com/watch?v=adad01",
   "details":[{"name":"From Google Ads"}],
   "time":"2024-03-01T13:06:00.000Z","products":["YouTube"],"activityControls":["YouTube watch history"]},
  {"header":"YouTube","title":"Searched for slow math","titleUrl":"https://www.youtube.com/results?search_query=slow+math",
   "time":"2024-03-01T13:05:30.000Z","products":["YouTube"],"activityControls":["YouTube search history"]},
  {"header":"YouTube","title":"Watched a video that has been removed",
   "time":"2024-03-01T13:05:00.000Z","products":["YouTube"],"activityControls":["YouTube watch history"]}
]"#;

#[test]
fn a_realistic_mix_stores_only_the_watches() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("MyActivity.json");
    std::fs::write(&file, REALISTIC_MIX).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &file).unwrap();

    assert_eq!(
        summary,
        ImportSummary {
            files_read: 1,
            added: 4,
            excluded_ads: 1,
            ..ImportSummary::default()
        }
    );
    let got: Vec<(String, String, Option<String>)> = store
        .plays(None)
        .unwrap()
        .into_iter()
        .map(|p| (p.item, p.creator, p.item_ref))
        .collect();
    let want = |item: &str, creator: &str, id: Option<&str>| {
        (item.to_string(), creator.to_string(), id.map(String::from))
    };
    assert_eq!(
        got,
        [
            want("a video that has been removed", "", None),
            want("Why Zero Matters", "Slow Math Club", Some("math01")),
            want("Tiny Compilers", "Cache Miss", Some("short01")),
            want("Low Tide", "The Paper Lanterns", Some("music01")),
        ]
    );
}

#[test]
fn localised_file_name_still_imports() {
    let dir = TempDir::new().unwrap();
    let zip = write_zip(
        dir.path(),
        "takeout-20240301T130542Z-001.zip",
        &[(
            "Takeout/YouTube/historial/historial-de-reproducciones.json",
            serde_json::to_vec(&json!([watch("Night Drive Mix", TIME)])).unwrap(),
        )],
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, &zip).unwrap();
    assert_eq!((summary.files_read, summary.added), (1, 1));
    assert_eq!(items(&store), ["Night Drive Mix"]);
}

#[test]
fn localised_title_is_kept_whole() {
    let (store, _) = import_rows(json!([row(
        "Visionné Foo",
        "https://www.youtube.com/watch?v=abc123",
        TIME
    )]));
    let plays = store.plays(None).unwrap();
    assert_eq!(plays[0].item, "Visionné Foo");
    assert_eq!(plays[0].item_ref.as_deref(), Some("abc123"));
}

#[test]
fn a_bad_row_costs_only_that_row() {
    let mut number_title = watch("Night Drive Mix", TIME);
    number_title["title"] = json!(12345);
    let (store, summary) = import_rows(json!([
        watch("Low Tide", "2024-03-01T13:05:00.000Z"),
        42,
        number_title,
        without(watch("No Time", TIME), "time"),
        watch("Last Tuesday", "last Tuesday"),
        row("", "https://www.youtube.com/watch?v=abc123", TIME),
        without(watch("No Title", TIME), "title"),
        watch("Soft Static", "2024-03-01T13:07:00.000Z"),
    ]));
    assert_eq!((summary.rejected, summary.added), (6, 2));
    assert_eq!(items(&store), ["Low Tide", "Soft Static"]);
}

#[test]
fn only_a_believable_time_is_kept() {
    // YouTube is older than Spotify, so the earliest year differs from the Spotify parser's.
    let (store, summary) = import_rows(json!([
        watch("First Year", "2005-06-01T00:00:00.000Z"),
        watch("Too Early", "2005-01-01T00:00:00.000Z"),
        watch("Too Late", "9999-12-31T23:59:59.000Z"),
    ]));
    assert_eq!((summary.added, summary.rejected), (1, 2));
    assert_eq!(items(&store), ["First Year"]);
}

#[test]
fn importing_twice_adds_nothing() {
    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "watch-history.json",
        &json!([watch("Night Drive Mix", TIME)]),
    );
    let mut store = Store::open_in_memory().unwrap();
    import_path(&mut store, &file).unwrap();
    let again = import_path(&mut store, &file).unwrap();
    assert_eq!((again.added, again.duplicates), (0, 1));
}

#[test]
fn a_later_takeout_that_rewords_a_watch_adds_nothing() {
    let time = |minute: u32| format!("2024-03-01T13:{minute:02}:42.123Z");
    let link = |id: &str| format!("https://www.youtube.com/watch?v={id}");
    let dir = TempDir::new().unwrap();
    let first = write_json(
        dir.path(),
        "first.json",
        &json!([
            row("Watched Original Title", &link("title1"), &time(1)),
            row("Watched Soon Gone", &link("gone01"), &time(2)),
            row("Watched Soon Removed", &link("gone02"), &time(3)),
            row("Watched By Old Name", &link("chan01"), &time(4)),
            row("Watched Foo", &link("lang01"), &time(5)),
        ]),
    );
    // Takeout writes all of this when it exports, so the same watches can come back like this.
    let mut renamed = row("Watched By Old Name", &link("chan01"), &time(4));
    renamed["subtitles"][0]["name"] = json!("Morning Lark Radio");
    let second = write_json(
        dir.path(),
        "second.json",
        &json!([
            row("Watched New Title", &link("title1"), &time(1)),
            without(
                row(
                    &format!("Watched {}", link("gone01")),
                    &link("gone01"),
                    &time(2)
                ),
                "subtitles"
            ),
            without(
                without(
                    row("Watched a video that has been removed", "", &time(3)),
                    "titleUrl"
                ),
                "subtitles"
            ),
            renamed,
            row("Visionn\u{e9} Foo", &link("lang01"), &time(5)),
        ]),
    );
    for (one, other) in [(&first, &second), (&second, &first)] {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(import_path(&mut store, one).unwrap().added, 5);
        let again = import_path(&mut store, other).unwrap();
        assert_eq!((again.added, again.duplicates), (0, 5));
        assert_eq!(store.totals().unwrap().youtube.plays, 5);
    }
}

#[test]
fn watching_a_video_again_within_the_minute_is_another_play() {
    let (store, summary) = import_rows(json!([
        watch("Tiny Compilers", "2024-03-01T13:05:10.000Z"),
        watch("Tiny Compilers", "2024-03-01T13:05:40.000Z"),
    ]));
    assert_eq!((summary.added, summary.duplicates), (2, 0));
    assert_eq!(store.plays(None).unwrap().len(), 2);
}

#[test]
fn html_export_explains_json() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("watch-history.html");
    std::fs::write(&file, takeout_html()).unwrap();
    let message = import_path(&mut Store::open_in_memory().unwrap(), &file)
        .unwrap_err()
        .to_string();
    for words in [
        "watch-history.html",
        "HTML",
        "Deselect all",
        "Multiple formats",
        "JSON",
    ] {
        assert!(message.contains(words), "{words}: {message}");
    }
}

#[test]
fn html_in_any_language_is_found_by_its_content() {
    let dir = TempDir::new().unwrap();
    let html = takeout_html();
    // The links start well past the 4 KB that are enough to know a JSON file by.
    assert!(html.iter().position(|b| *b == b'W').unwrap() > 4096);
    let file = dir.path().join("historial-de-reproducciones.html");
    std::fs::write(&file, &html).unwrap();
    let zip = write_zip(
        dir.path(),
        "takeout.zip",
        &[
            ("Takeout/archive_browser.html", INDEX_PAGE.to_vec()),
            ("Takeout/YouTube/historial/historial.html", html),
        ],
    );
    for input in [file, zip] {
        let message = import_path(&mut Store::open_in_memory().unwrap(), &input)
            .unwrap_err()
            .to_string();
        // Said about this file, not only as part of the message for a file Rostery does not know.
        assert!(message.contains("Takeout in HTML"), "{message}");
        assert!(message.contains("Multiple formats"), "{message}");
    }
}

#[test]
fn html_beside_a_good_file_still_explains_json() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([watch("Night Drive Mix", TIME)]),
    );
    std::fs::write(dir.path().join("older-history.html"), takeout_html()).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, dir.path()).unwrap();
    assert_eq!(
        (summary.files_read, summary.files_skipped, summary.added),
        (1, 1, 1)
    );
    let reason = summary.skipped_reason.unwrap();
    assert!(reason.contains("Multiple formats"), "{reason}");
}

#[test]
fn nothing_skipped_means_no_reason() {
    let (_, summary) = import_rows(json!([watch("Night Drive Mix", TIME)]));
    assert_eq!(summary.skipped_reason, None);
}

#[test]
fn the_takeout_index_page_is_not_a_problem() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([
            watch("Low Tide", "2024-03-01T13:05:00.000Z"),
            watch("Soft Static", "2024-03-01T13:09:00.000Z"),
        ]),
    );
    std::fs::write(dir.path().join("archive_browser.html"), INDEX_PAGE).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[api_play(Source::Youtube, MINUTE + 60, "Api Channel")])
        .unwrap();
    let summary = import_path(&mut store, dir.path()).unwrap();
    assert_eq!((summary.files_read, summary.files_skipped), (1, 0));
    // The folder counts as fully read, so the overlap rule still runs.
    assert!(!creators(&store).contains(&"Api Channel".to_string()));

    // On its own it is simply not an Export.
    let alone = dir.path().join("archive_browser.html");
    let message = import_path(&mut Store::open_in_memory().unwrap(), &alone)
        .unwrap_err()
        .to_string();
    assert!(message.contains("did not find"), "{message}");
}

#[test]
fn a_page_that_only_links_to_youtube_is_not_a_problem() {
    let bookmarks = b"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p><DT>\
        <A HREF=\"https://www.youtube.com/watch?v=abc123\">Night Drive Mix</A></DL>"
        .to_vec();
    let history = json!([
        watch("Low Tide", "2024-03-01T13:05:00.000Z"),
        watch("Soft Static", "2024-03-01T13:09:00.000Z"),
    ]);
    let dir = TempDir::new().unwrap();
    // A Takeout of a whole account, with history set to JSON as the steps say.
    let zip = write_zip(
        dir.path(),
        "takeout.zip",
        &[
            ("Takeout/Chrome/Bookmarks.html", bookmarks.clone()),
            (
                "Takeout/My Activity/Chrome/MyActivity.html",
                activity_html("Chrome"),
            ),
            (
                "Takeout/My Activity/Search/MyActivity.html",
                activity_html("Search"),
            ),
            (
                "Takeout/YouTube and YouTube Music/history/watch-history.json",
                serde_json::to_vec(&history).unwrap(),
            ),
        ],
    );
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[api_play(Source::Youtube, MINUTE + 60, "Api Channel")])
        .unwrap();
    let summary = import_path(&mut store, &zip).unwrap();
    assert_eq!((summary.files_read, summary.files_skipped), (1, 0));
    assert_eq!(summary.skipped_reason, None);
    // Nothing was left unread, so the overlap rule still runs.
    assert!(!creators(&store).contains(&"Api Channel".to_string()));

    // On its own it is simply not an Export.
    let alone = dir.path().join("Bookmarks.html");
    std::fs::write(&alone, bookmarks).unwrap();
    let message = import_path(&mut Store::open_in_memory().unwrap(), &alone)
        .unwrap_err()
        .to_string();
    assert!(message.contains("did not find"), "{message}");
}

#[test]
fn a_youtube_page_with_no_watches_is_not_a_problem() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([watch("Low Tide", TIME)]),
    );
    let searches = String::from_utf8(takeout_html())
        .unwrap()
        .replace("watch?v=abc123", "results?search_query=slow+math");
    std::fs::write(dir.path().join("search-history.html"), searches).unwrap();
    let summary = import_path(&mut Store::open_in_memory().unwrap(), dir.path()).unwrap();
    assert_eq!((summary.files_read, summary.files_skipped), (1, 0));
}

#[test]
fn every_youtube_history_page_is_found() {
    let dir = TempDir::new().unwrap();
    for product in ["YouTube", "YouTube Music"] {
        let file = dir.path().join("MyActivity.html");
        std::fs::write(&file, activity_html(product)).unwrap();
        let message = import_path(&mut Store::open_in_memory().unwrap(), &file)
            .unwrap_err()
            .to_string();
        assert!(message.contains("Takeout in HTML"), "{product}: {message}");
    }
}

#[test]
fn an_html_file_that_is_skipped_keeps_api_plays() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([
            watch("Low Tide", "2024-03-01T13:05:00.000Z"),
            watch("Soft Static", "2024-03-01T13:09:00.000Z"),
        ]),
    );
    std::fs::write(dir.path().join("older-history.html"), takeout_html()).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[api_play(Source::Youtube, MINUTE + 60, "Api Channel")])
        .unwrap();
    import_path(&mut store, dir.path()).unwrap();
    assert!(creators(&store).contains(&"Api Channel".to_string()));
}

fn spotify_row(ts: &str) -> Value {
    json!({
        "ts": ts,
        "ms_played": 200_000,
        "master_metadata_album_artist_name": "The Paper Lanterns",
        "master_metadata_track_name": "Low Tide",
        "spotify_track_uri": "spotify:track:4uLU6hMCjMI75M1A2tKUQC",
    })
}

#[test]
fn mixed_folder_imports_both_sources() {
    let dir = TempDir::new().unwrap();
    write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([spotify_row("2024-03-01T13:05:00Z")]),
    );
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([watch("Night Drive Mix", TIME)]),
    );
    let mut store = Store::open_in_memory().unwrap();
    let summary = import_path(&mut store, dir.path()).unwrap();
    assert_eq!((summary.files_read, summary.added), (2, 2));
    let totals = store.totals().unwrap();
    assert_eq!((totals.spotify.plays, totals.youtube.plays), (1, 1));
}

#[test]
fn each_source_replaces_api_plays_only_inside_its_own_span() {
    let at = |s: i64| MINUTE + s;
    let dir = TempDir::new().unwrap();
    // The Spotify Export spans 10 minutes, the Takeout a different hour. The API Plays below show
    // that neither span is applied to the other Source, and that the two edges are inclusive.
    write_json(
        dir.path(),
        "Streaming_History_Audio_2024_0.json",
        &json!([
            spotify_row("2024-03-01T13:05:00Z"),
            spotify_row("2024-03-01T13:15:00Z"),
        ]),
    );
    write_json(
        dir.path(),
        "watch-history.json",
        &json!([
            watch("Low Tide", "2024-03-01T15:05:00.000Z"),
            watch("Soft Static", "2024-03-01T15:15:00.000Z"),
        ]),
    );
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[
            api_play(Source::Spotify, at(-1), "Spotify before"),
            api_play(Source::Spotify, at(0), "Spotify oldest edge"),
            api_play(Source::Spotify, at(600), "Spotify newest edge"),
            api_play(Source::Spotify, at(601), "Spotify after"),
            api_play(Source::Spotify, at(7300), "Spotify inside the Takeout span"),
            api_play(Source::Youtube, at(300), "YouTube inside the Spotify span"),
            api_play(Source::Youtube, at(7200), "YouTube oldest edge"),
            api_play(Source::Youtube, at(7800), "YouTube newest edge"),
            api_play(Source::Youtube, at(7801), "YouTube after"),
        ])
        .unwrap();

    import_path(&mut store, dir.path()).unwrap();

    let left = creators(&store);
    for kept in [
        "Spotify before",
        "Spotify after",
        "Spotify inside the Takeout span",
        "YouTube inside the Spotify span",
        "YouTube after",
    ] {
        assert!(
            left.contains(&kept.to_string()),
            "{kept} should stay: {left:?}"
        );
    }
    for gone in [
        "Spotify oldest edge",
        "Spotify newest edge",
        "YouTube oldest edge",
        "YouTube newest edge",
    ] {
        assert!(
            !left.contains(&gone.to_string()),
            "{gone} should go: {left:?}"
        );
    }
}

#[test]
fn a_source_with_no_plays_in_the_import_keeps_its_api_plays() {
    let dir = TempDir::new().unwrap();
    let file = write_json(
        dir.path(),
        "watch-history.json",
        &json!([
            watch("Low Tide", "2024-03-01T13:05:00.000Z"),
            watch("Soft Static", "2024-03-01T13:15:00.000Z"),
        ]),
    );
    let mut store = Store::open_in_memory().unwrap();
    store
        .insert_plays(&[api_play(Source::Spotify, MINUTE + 300, "Spotify Band")])
        .unwrap();
    import_paths(&mut store, &[&file]).unwrap();
    assert!(creators(&store).contains(&"Spotify Band".to_string()));
}
