use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Every Interest of each Creator, sorted, with no repeats.
pub type InterestMap = HashMap<(Source, String), Vec<String>>;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Spotify,
    Youtube,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Export,
    Api,
    Demo,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Spotify => "spotify",
            Source::Youtube => "youtube",
        }
    }

    pub fn from_db(s: &str) -> Option<Source> {
        match s {
            "spotify" => Some(Source::Spotify),
            "youtube" => Some(Source::Youtube),
            _ => None,
        }
    }
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Export => "export",
            Origin::Api => "api",
            Origin::Demo => "demo",
        }
    }

    pub fn from_db(s: &str) -> Option<Origin> {
        match s {
            "export" => Some(Origin::Export),
            "api" => Some(Origin::Api),
            "demo" => Some(Origin::Demo),
            _ => None,
        }
    }
}

/// One listen to a track or one watch of a video. `played_at` is Unix seconds in UTC.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Play {
    pub source: Source,
    pub origin: Origin,
    pub played_at: i64,
    pub duration_ms: Option<i64>,
    pub creator: String,
    pub creator_ref: Option<String>,
    pub item: String,
    pub item_ref: Option<String>,
    pub skipped: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceTotals {
    pub plays: u64,
    pub minutes: u64,
    pub creators: u64,
    pub first_play: Option<i64>,
    pub last_play: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub spotify: SourceTotals,
    pub youtube: SourceTotals,
}

/// A signal that the user likes a Creator without a Play behind it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Affinity {
    pub source: Source,
    pub creator: String,
    pub creator_ref: Option<String>,
    pub kind: String,
    pub score: f64,
    pub fetched_at: i64,
}
