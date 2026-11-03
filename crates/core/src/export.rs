//! The Share Site: what goes in it, and the files that hold it. Nothing reaches a file unless the person
//! chose it, so every choice is applied here, before the payload exists.

use crate::error::{Error, Result};
use crate::model::Source;
use crate::roast::Roast;
use crate::store::Store;
use crate::universe::{self, Universe, UniverseFilter, UniverseInput};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path};

const DATA: &str = "__ROSTERY_DATA__";
const TITLE: &str = "__ROSTERY_TITLE__";
const DESC: &str = "__ROSTERY_DESC__";
const MAX_DISPLAY_NAME: usize = 60;
const MAX_DESC: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportOptions {
    pub display_name: String,
    /// On, because sharing the Roast is the point of a Share Site. It quotes names and titles from all
    /// of history whatever else is chosen, and the Share screen says so beside the choice.
    pub include_roast: bool,
    pub include_spotify: bool,
    pub include_youtube: bool,
    /// Off, because a video title can say far more about someone than a channel name.
    pub include_item_titles: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            display_name: "Anonymous".into(),
            include_roast: true,
            include_spotify: true,
            include_youtube: true,
            include_item_titles: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharePayload {
    pub display_name: String,
    pub generated_at: i64,
    pub roast: Option<Roast>,
    pub universes: BTreeMap<UniverseFilter, Universe>,
    pub attribution: Vec<String>,
}

/// Where a Share Site went, and how big it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub dir: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SiteFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub fn build_payload(store: &Store, opts: &ExportOptions, now: i64) -> Result<SharePayload> {
    let roast = if opts.include_roast {
        // Its Galaxy names travel on the Universes, and their anchors hold Creator names.
        store.latest_roast()?.map(|stored| Roast {
            galaxy_names: Vec::new(),
            ..stored.roast
        })
    } else {
        None
    };
    // A Roast's Galaxy names are Roast text, so they only name Galaxies when the Roast goes too.
    let names = if roast.is_some() {
        store.latest_galaxy_names()?
    } else {
        Vec::new()
    };
    let (plays, affinities, interests) =
        (store.plays(None)?, store.affinities()?, store.interests()?);
    let has = |source: Source| {
        plays.iter().any(|p| p.source == source) || affinities.iter().any(|a| a.source == source)
    };
    let spotify = opts.include_spotify && has(Source::Spotify);
    let youtube = opts.include_youtube && has(Source::Youtube);
    let input = UniverseInput {
        plays: &plays,
        affinities: &affinities,
        interests: &interests,
        galaxy_names: &HashMap::new(),
    };
    let filters = [
        (UniverseFilter::Spotify, spotify),
        (UniverseFilter::Youtube, youtube),
        (UniverseFilter::Both, spotify && youtube),
    ];
    let universes = filters
        .into_iter()
        .filter(|(_, wanted)| *wanted)
        .map(|(filter, _)| {
            let mut u = universe::named(&input, &names, filter);
            if !opts.include_item_titles {
                for planet in &mut u.planets {
                    planet.top_items.clear();
                }
            }
            (filter, u)
        })
        .collect();
    if roast.is_none() && !spotify && !youtube {
        return Err(Error::Export(
            "There is nothing to share with these choices. Include your Roast or a Universe, or add \
             your history on the Sources screen first."
                .into(),
        ));
    }
    let attribution = [
        (spotify, "Listening history from Spotify."),
        (youtube, "Watch history from YouTube."),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, line)| line.to_owned())
    .collect();
    Ok(SharePayload {
        display_name: display_name(&opts.display_name),
        generated_at: now,
        roast,
        universes,
        attribution,
    })
}

fn display_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DISPLAY_NAME)
        .collect();
    if name.trim().is_empty() {
        "Anonymous".into()
    } else {
        name.trim().to_owned()
    }
}

/// The template with the payload in it, and a `.nojekyll` so GitHub Pages serves the folder as it is.
pub fn render_site(template: &[SiteFile], payload: &SharePayload) -> Result<Vec<SiteFile>> {
    let index = template
        .iter()
        .find(|f| f.path == "index.html")
        .ok_or_else(|| broken_template("it has no index.html"))?;
    let html = std::str::from_utf8(&index.bytes)
        .map_err(|_| broken_template("its index.html is not text"))?;
    if !html.contains(DATA) {
        return Err(broken_template("its index.html has no place for the data"));
    }
    let (title, desc) = title_and_description(payload);
    let html = fill(
        html,
        &[
            (DATA, script_json(&serde_json::to_string(payload)?)),
            (TITLE, escape_html(&title)),
            (DESC, escape_html(&desc)),
        ],
    );
    let mut files: Vec<SiteFile> = template
        .iter()
        .map(|f| match f.path.as_str() {
            "index.html" => SiteFile {
                path: f.path.clone(),
                bytes: html.clone().into_bytes(),
            },
            _ => f.clone(),
        })
        .collect();
    files.push(SiteFile {
        path: ".nojekyll".into(),
        bytes: Vec::new(),
    });
    Ok(files)
}

fn broken_template(why: &str) -> Error {
    Error::Export(format!(
        "Rostery could not build the Share Site, because {why}. Reinstall Rostery and try again."
    ))
}

fn title_and_description(payload: &SharePayload) -> (String, String) {
    let name = &payload.display_name;
    match &payload.roast {
        Some(roast) => (
            format!("{name}'s Roast: {}", roast.headline),
            roast.opening.chars().take(MAX_DESC).collect(),
        ),
        None => (
            format!("{name}'s Universe"),
            format!("A universe of what {name} listens to and watches, made with Rostery."),
        ),
    }
}

/// One pass over the template, so a value that happens to hold a marker is never filled in itself.
fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((at, marker, value)) = values
        .iter()
        .filter_map(|(marker, value)| rest.find(marker).map(|at| (at, *marker, value)))
        .min_by_key(|(at, _, _)| *at)
    {
        out.push_str(&rest[..at]);
        out.push_str(value);
        rest = &rest[at + marker.len()..];
    }
    out.push_str(rest);
    out
}

/// JSON that cannot end its `<script>` early. These characters only ever sit inside JSON strings, where
/// the escapes read the same.
fn script_json(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// `dir` belongs to Rostery, and what was there goes, so a file left from an earlier export cannot
/// share something the person has since left out.
pub fn write_site(files: &[SiteFile], dir: &Path) -> Result<()> {
    for file in files {
        let path = Path::new(&file.path);
        if file.path.is_empty() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err(broken_template("one of its files has an unsafe path"));
        }
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    for file in files {
        let path = dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &file.bytes)?;
    }
    Ok(())
}

/// What `render_site` needs from a built `dist/share`, read from disk.
pub fn read_template(dir: &Path) -> Result<Vec<SiteFile>> {
    ["index.html", "assets/share.js", "assets/share.css"]
        .into_iter()
        .map(|path| {
            Ok(SiteFile {
                path: path.into(),
                bytes: std::fs::read(dir.join(path))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Settings;
    use crate::roast::parse_roast;
    use crate::universe::testing::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000;
    const CHANNEL: &str = "Sentinel Channel Qx7";
    const TITLE_SENTINEL: &str = "Sentinel Video Title Zk3";

    fn template() -> Vec<SiteFile> {
        let file = |path: &str, text: &str| SiteFile {
            path: path.into(),
            bytes: text.as_bytes().to_vec(),
        };
        vec![
            file(
                "index.html",
                "<!doctype html><title>__ROSTERY_TITLE__</title>\
                 <meta property=\"og:title\" content=\"__ROSTERY_TITLE__\">\
                 <meta name=\"description\" content=\"__ROSTERY_DESC__\">\
                 <script defer src=\"./assets/share.js\"></script>\
                 <link rel=\"stylesheet\" href=\"./assets/share.css\">\
                 <script id=\"rostery-data\" type=\"application/json\">__ROSTERY_DATA__</script>",
            ),
            file("assets/share.js", "console.log(1)"),
            file("assets/share.css", "body{}"),
        ]
    }

    fn roast_quoting(words: &str) -> Roast {
        parse_roast(
            &json!({
                "headline": format!("You and {words}"),
                "opening": format!("We need to talk about {words}."),
                "burns": [{"title": "Burn", "body": format!("{words}, again.")}],
                "verdict": {"archetype": "Night Clerk", "score": 40, "sentence": "Sit down."},
                "galaxy_names": [{"id": 0, "name": "The Quiet Room"}],
            })
            .to_string(),
        )
        .unwrap()
    }

    /// Both Sources, with a channel and a video title that make good sentinels, and a stored Roast that
    /// quotes them both.
    fn store() -> Store {
        let mut store = Store::open_in_memory().unwrap();
        let mut watch = youtube(CHANNEL, 0);
        watch.item = TITLE_SENTINEL.into();
        store
            .insert_plays(&[spotify("Moth Season", DAY, 30), watch])
            .unwrap();
        store
            .save_roast(
                "Claude Code",
                &roast_quoting(&format!("{CHANNEL} and {TITLE_SENTINEL}")),
                NOW,
                false,
            )
            .unwrap();
        store
    }

    fn site(store: &Store, opts: &ExportOptions) -> Vec<SiteFile> {
        render_site(&template(), &build_payload(store, opts, NOW).unwrap()).unwrap()
    }

    fn anywhere(files: &[SiteFile], needle: &str) -> bool {
        files
            .iter()
            .any(|f| String::from_utf8_lossy(&f.bytes).contains(needle))
    }

    fn index(files: &[SiteFile]) -> String {
        String::from_utf8(
            files
                .iter()
                .find(|f| f.path == "index.html")
                .unwrap()
                .bytes
                .clone(),
        )
        .unwrap()
    }

    fn data_json(files: &[SiteFile]) -> serde_json::Value {
        let html = index(files);
        let (_, rest) = html.split_once("application/json\">").unwrap();
        let (json, _) = rest.split_once("</script>").unwrap();
        serde_json::from_str(json).unwrap()
    }

    fn data(files: &[SiteFile]) -> SharePayload {
        serde_json::from_value(data_json(files)).unwrap()
    }

    #[test]
    fn escapes_script_breakout() {
        let attack = "</script><img src=x onerror=alert(1)>";
        let mut store = Store::open_in_memory().unwrap();
        store.insert_plays(&[spotify(attack, 0, 30)]).unwrap();
        let payload = build_payload(&store, &ExportOptions::default(), NOW).unwrap();
        let files = render_site(&template(), &payload).unwrap();
        let html = index(&files);
        assert!(!html.contains("</script><img"), "{html}");
        // Through the same parse, since a float's last digit can change on the way.
        let expected: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&payload).unwrap()).unwrap();
        assert_eq!(data_json(&files), expected);
    }

    #[test]
    fn title_is_html_escaped() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[spotify("Moth Season", 0, 30)])
            .unwrap();
        store
            .save_roast(
                "Claude Code",
                &roast_quoting("\"><script>alert(1)</script>"),
                NOW,
                false,
            )
            .unwrap();
        let html = index(&site(&store, &ExportOptions::default()));
        let head = html.split("<script defer").next().unwrap();
        assert!(!head.contains("\"><script>"), "{head}");
        assert!(head.contains("&quot;&gt;&lt;script&gt;"), "{head}");
    }

    #[test]
    fn a_value_holding_a_marker_is_not_filled_in() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[spotify("Moth Season", 0, 30)])
            .unwrap();
        let opts = ExportOptions {
            display_name: "__ROSTERY_DATA__ __ROSTERY_TITLE__".into(),
            ..ExportOptions::default()
        };
        let html = index(&site(&store, &opts));
        assert_eq!(html.matches("\"displayName\"").count(), 1);
        assert!(
            html.contains("<title>__ROSTERY_DATA__ __ROSTERY_TITLE__&#39;s Universe</title>"),
            "{html}"
        );
    }

    #[test]
    fn line_separators_escaped() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[spotify("Line\u{2028}Para\u{2029}", 0, 30)])
            .unwrap();
        let files = site(&store, &ExportOptions::default());
        let html = index(&files);
        assert!(!html.contains('\u{2028}') && !html.contains('\u{2029}'));
        assert!(html.contains("\\u2028") && html.contains("\\u2029"));
        data(&files);
    }

    #[test]
    fn excluded_source_is_absent() {
        let store = store();
        let opts = ExportOptions {
            include_youtube: false,
            include_roast: false,
            ..ExportOptions::default()
        };
        let files = site(&store, &opts);
        assert!(!anywhere(&files, CHANNEL));
        let payload = data(&files);
        assert_eq!(
            payload.universes.keys().copied().collect::<Vec<_>>(),
            [UniverseFilter::Spotify]
        );
    }

    #[test]
    fn excluded_roast_is_absent() {
        let opts = ExportOptions {
            include_roast: false,
            ..ExportOptions::default()
        };
        let files = site(&store(), &opts);
        assert!(!anywhere(&files, "We need to talk about"));
        assert!(
            !anywhere(&files, "The Quiet Room"),
            "Roast names go with the Roast"
        );
        assert_eq!(data(&files).roast, None);
    }

    #[test]
    fn roast_is_on_by_default() {
        let payload = build_payload(&store(), &ExportOptions::default(), NOW).unwrap();
        let roast = payload.roast.unwrap();
        assert!(roast.headline.starts_with("You and"));
        assert!(
            roast.galaxy_names.is_empty(),
            "names travel on the Universes"
        );
    }

    #[test]
    fn item_titles_off_by_default() {
        let store = store();
        let without_roast = ExportOptions {
            include_roast: false,
            ..ExportOptions::default()
        };
        assert!(!anywhere(&site(&store, &without_roast), TITLE_SENTINEL));
        let with_titles = ExportOptions {
            include_item_titles: true,
            ..without_roast
        };
        assert!(anywhere(&site(&store, &with_titles), TITLE_SENTINEL));
    }

    #[test]
    fn included_roast_keeps_its_words() {
        let opts = ExportOptions {
            include_roast: true,
            include_youtube: false,
            include_item_titles: false,
            ..ExportOptions::default()
        };
        let files = site(&store(), &opts);
        assert!(anywhere(&files, CHANNEL));
        assert!(anywhere(&files, TITLE_SENTINEL));
    }

    #[test]
    fn no_secrets_in_site() {
        let store = store();
        store
            .set_setting("sentinel", &"Secret Sentinel Wq9")
            .unwrap();
        let settings = Settings {
            system_prompt: Some("Prompt Sentinel Vb2".into()),
            ..Settings::default()
        };
        settings.save(&store).unwrap();
        let files = site(&store, &ExportOptions::default());
        assert!(!anywhere(&files, "Secret Sentinel Wq9"));
        assert!(!anywhere(&files, "Prompt Sentinel Vb2"));
    }

    #[test]
    fn paths_are_relative() {
        let html = index(&site(&store(), &ExportOptions::default()));
        assert!(!html.contains("=\"/"), "{html}");
        assert!(
            !html.contains("src=\"http") && !html.contains("href=\"http"),
            "{html}"
        );
    }

    #[test]
    fn site_files_are_exact() {
        let mut paths: Vec<String> = site(&store(), &ExportOptions::default())
            .into_iter()
            .map(|f| f.path)
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                ".nojekyll",
                "assets/share.css",
                "assets/share.js",
                "index.html"
            ]
        );
    }

    #[test]
    fn rewrite_removes_stale_files() {
        let dir = tempfile::tempdir().unwrap();
        let site_dir = dir.path().join("site");
        std::fs::create_dir_all(site_dir.join("old")).unwrap();
        std::fs::write(site_dir.join("old/stale.html"), CHANNEL).unwrap();
        write_site(&site(&store(), &ExportOptions::default()), &site_dir).unwrap();
        assert!(!site_dir.join("old").exists());
        assert!(site_dir.join("index.html").exists());
        assert!(site_dir.join("assets/share.js").exists());
        assert!(site_dir.join(".nojekyll").exists());
    }

    #[test]
    fn an_unsafe_path_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let site_dir = dir.path().join("site");
        let files = [SiteFile {
            path: "../escaped.html".into(),
            bytes: Vec::new(),
        }];
        assert!(write_site(&files, &site_dir).is_err());
        assert!(!dir.path().join("escaped.html").exists());
        assert!(!site_dir.exists());
    }

    #[test]
    fn attribution_lists_included_sources() {
        let store = store();
        let both = build_payload(&store, &ExportOptions::default(), NOW).unwrap();
        assert_eq!(
            both.attribution,
            [
                "Listening history from Spotify.",
                "Watch history from YouTube."
            ]
        );
        let spotify_only = ExportOptions {
            include_youtube: false,
            ..ExportOptions::default()
        };
        let one = build_payload(&store, &spotify_only, NOW).unwrap();
        assert_eq!(one.attribution, ["Listening history from Spotify."]);
    }

    #[test]
    fn a_source_with_no_data_has_no_universe() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[spotify("Moth Season", 0, 30)])
            .unwrap();
        let payload = build_payload(&store, &ExportOptions::default(), NOW).unwrap();
        assert_eq!(
            payload.universes.keys().copied().collect::<Vec<_>>(),
            [UniverseFilter::Spotify]
        );
        assert_eq!(payload.attribution, ["Listening history from Spotify."]);
    }

    #[test]
    fn nothing_to_share_says_so() {
        let opts = ExportOptions {
            include_roast: false,
            include_spotify: false,
            include_youtube: false,
            ..ExportOptions::default()
        };
        let message = build_payload(&store(), &opts, NOW).unwrap_err().to_string();
        assert!(message.contains("nothing to share"), "{message}");
        let empty = Store::open_in_memory().unwrap();
        assert!(build_payload(&empty, &ExportOptions::default(), NOW).is_err());
    }

    #[test]
    fn a_blank_display_name_is_anonymous() {
        assert_eq!(display_name("  \u{0}\n "), "Anonymous");
        assert_eq!(display_name(&"x".repeat(500)).len(), MAX_DISPLAY_NAME);
    }

    #[test]
    fn options_cross_ipc_in_camel_case_and_fill_gaps_with_defaults() {
        let opts: ExportOptions = serde_json::from_value(json!({"displayName": "Sam"})).unwrap();
        assert_eq!(opts.display_name, "Sam");
        assert!(opts.include_roast && !opts.include_item_titles);
    }
}
