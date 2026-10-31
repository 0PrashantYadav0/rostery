//! The Universe: a pure function from history to a picture. It is computed here and not in the
//! webview, so the app and the Share Site draw the same one and a test can check it.

mod graph;
mod layout;

pub(crate) use graph::play_ms;

use crate::model::{Affinity, InterestMap, Play, Source};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum UniverseFilter {
    Spotify,
    Youtube,
    Both,
}

/// One Creator. `id` is its place in `Universe::planets`, heaviest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Planet {
    pub id: u32,
    pub name: String,
    pub source: Source,
    pub minutes: f64,
    pub plays: u64,
    pub radius: f64,
    pub position: [f64; 3],
    pub galaxy: u32,
    pub interests: Vec<String>,
    pub url: Option<String>,
    pub top_items: Vec<TopItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub title: String,
    pub url: Option<String>,
    pub plays: u64,
}

/// `a` is below `b`. `shared` lists the Interests the two have in common.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub a: u32,
    pub b: u32,
    pub strength: f64,
    pub shared: Vec<String>,
}

/// `hue` is a share of the colour wheel, from 0 up to 1.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Galaxy {
    pub id: u32,
    pub name: String,
    pub hue: f64,
    pub size: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Universe {
    pub filter: UniverseFilter,
    pub planets: Vec<Planet>,
    pub links: Vec<Link>,
    pub galaxies: Vec<Galaxy>,
}

pub struct UniverseInput<'a> {
    pub plays: &'a [Play],
    pub affinities: &'a [Affinity],
    pub interests: &'a InterestMap,
    pub galaxy_names: &'a HashMap<u32, String>,
}

pub fn build(input: &UniverseInput, filter: UniverseFilter) -> Universe {
    let mut planets = graph::planets(input, filter);
    let links = graph::links(input.plays, &planets, filter);
    let galaxies = graph::galaxies(&mut planets, &links, input.galaxy_names);
    layout::place(&mut planets, &links, galaxies.len());
    Universe {
        filter,
        planets,
        links,
        galaxies,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::model::Origin;

    pub const DAY: i64 = 86_400;

    pub fn spotify(creator: &str, played_at: i64, minutes: i64) -> Play {
        Play {
            source: Source::Spotify,
            origin: Origin::Export,
            played_at,
            duration_ms: Some(minutes * 60_000),
            creator: creator.into(),
            creator_ref: None,
            item: "Low Tide".into(),
            item_ref: None,
            skipped: false,
        }
    }

    pub fn youtube(creator: &str, played_at: i64) -> Play {
        Play {
            source: Source::Youtube,
            duration_ms: None,
            ..spotify(creator, played_at, 0)
        }
    }

    /// `(Source, Creator, Interests)` rows, the way a Store hands them over.
    pub fn interests(rows: &[(Source, &str, &[&str])]) -> InterestMap {
        rows.iter()
            .map(|(source, creator, tags)| {
                let tags = tags.iter().map(|t| t.to_string()).collect();
                ((*source, creator.to_string()), tags)
            })
            .collect()
    }

    pub fn from_plays(plays: &[Play], filter: UniverseFilter) -> Universe {
        with_interests(plays, &InterestMap::new(), filter)
    }

    pub fn with_interests(plays: &[Play], map: &InterestMap, filter: UniverseFilter) -> Universe {
        build(
            &UniverseInput {
                plays,
                affinities: &[],
                interests: map,
                galaxy_names: &HashMap::new(),
            },
            filter,
        )
    }

    pub fn planet<'a>(u: &'a Universe, name: &str) -> &'a Planet {
        u.planets
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no Planet called {name}"))
    }

    pub fn linked(u: &Universe, a: &str, b: &str) -> Option<Link> {
        let (a, b) = (planet(u, a).id, planet(u, b).id);
        u.links
            .iter()
            .find(|l| (l.a, l.b) == (a.min(b), a.max(b)))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn filter_works_as_a_lowercase_map_key() {
        let map = BTreeMap::from([
            (UniverseFilter::Both, 3),
            (UniverseFilter::Spotify, 1),
            (UniverseFilter::Youtube, 2),
        ]);
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"spotify":1,"youtube":2,"both":3}"#
        );
        assert_eq!(
            serde_json::from_str::<UniverseFilter>(r#""youtube""#).unwrap(),
            UniverseFilter::Youtube
        );
    }

    #[test]
    fn universe_crosses_ipc_in_camel_case() {
        let u = from_plays(&[spotify("Moth Season", 0, 3)], UniverseFilter::Both);
        let json = serde_json::to_value(&u).unwrap();
        assert_eq!(json["filter"], "both");
        let planet = &json["planets"][0];
        assert_eq!(planet["source"], "spotify");
        assert_eq!(planet["topItems"][0]["title"], "Low Tide");
        assert!(planet.get("top_items").is_none());
    }

    /// A decade of heavy listening, in Sessions of 20 Plays over 2,000 Creators.
    #[test]
    #[ignore = "timing test, run in release"]
    fn universe_300k() {
        let plays: Vec<Play> = (0..300_000_i64)
            .map(|i| {
                let session = i / 20;
                let creator = format!("Creator {}", (i * i + session * 7) % 2_000);
                // 20 Plays three minutes apart, then a gap of a few hours.
                let at = session * 14_400 + (i % 20) * 180;
                if i % 3 == 0 {
                    youtube(&creator, at)
                } else {
                    spotify(&creator, at, 3)
                }
            })
            .collect();
        let start = std::time::Instant::now();
        let u = from_plays(&plays, UniverseFilter::Both);
        let elapsed = start.elapsed();
        println!(
            "universe_300k: {elapsed:?}, {} Planets, {} Links, {} Galaxies",
            u.planets.len(),
            u.links.len(),
            u.galaxies.len()
        );
        assert_eq!(u.planets.len(), 150);
        assert!(!u.links.is_empty());
        assert!(elapsed.as_secs_f64() < 3.0, "took {elapsed:?}");
    }
}
