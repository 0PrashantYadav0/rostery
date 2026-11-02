//! The Universe: a pure function from history to a picture. It is computed here and not in the
//! webview, so the app and the Share Site draw the same one and a test can check it.

mod graph;
mod layout;

pub(crate) use graph::play_ms;

use crate::error::Result;
use crate::model::{Affinity, InterestMap, Play, Source};
use crate::store::Store;
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

/// `input.galaxy_names` go by the ids of Galaxies in this `filter`'s own view.
pub fn build(input: &UniverseInput, filter: UniverseFilter) -> Universe {
    let (planets, links, count) = grouped(input, filter);
    finished(filter, planets, links, count, input.galaxy_names)
}

fn grouped(input: &UniverseInput, filter: UniverseFilter) -> (Vec<Planet>, Vec<Link>, u32) {
    let mut planets = graph::planets(input, filter);
    let links = graph::links(input.plays, &planets, filter);
    let count = graph::group(&mut planets, &links);
    (planets, links, count)
}

fn finished(
    filter: UniverseFilter,
    mut planets: Vec<Planet>,
    links: Vec<Link>,
    count: u32,
    names: &HashMap<u32, String>,
) -> Universe {
    let galaxies = graph::galaxies(&planets, count, names);
    layout::place(&mut planets, &links, galaxies.len());
    Universe {
        filter,
        planets,
        links,
        galaxies,
    }
}

/// The Universe a screen or a Share Site shows, where `input.galaxy_names` are a Roast's, by the ids of
/// Galaxies in the Both view. Those are what a Digest holds, and each view works out Galaxies of its
/// own, so in a Source's view those ids mean something else. There each Galaxy goes by its members.
pub fn named(input: &UniverseInput, filter: UniverseFilter) -> Universe {
    if filter == UniverseFilter::Both {
        return build(input, filter);
    }
    let (both, _, _) = grouped(input, UniverseFilter::Both);
    let (planets, links, count) = grouped(input, filter);
    let names = names_from_both(&planets, count, &both, input.galaxy_names);
    finished(filter, planets, links, count, &names)
}

/// Each Galaxy of a Source's view takes the name of the Both Galaxy holding its heaviest member that is
/// in Both at all, since the Planet cap can leave some out. Two Galaxies can end up claiming one name,
/// and `graph::galaxies` gives it to the heavier.
fn names_from_both(
    view: &[Planet],
    count: u32,
    both: &[Planet],
    names: &HashMap<u32, String>,
) -> HashMap<u32, String> {
    let galaxy_in_both: HashMap<(Source, &str), u32> = both
        .iter()
        .map(|p| ((p.source, p.name.as_str()), p.galaxy))
        .collect();
    (0..count)
        .filter_map(|id| {
            let in_both = view
                .iter()
                .filter(|p| p.galaxy == id)
                .find_map(|p| galaxy_in_both.get(&(p.source, p.name.as_str())))?;
            Some((id, names.get(in_both)?.clone()))
        })
        .collect()
}

/// `named`, from what is stored and the last Roast's Galaxy names. Everything that shows a Universe to
/// a person comes through here, so they all agree.
pub fn from_store(store: &Store, filter: UniverseFilter) -> Result<Universe> {
    let names = store.latest_galaxy_names()?;
    Ok(named(
        &UniverseInput {
            plays: &store.plays(None)?,
            affinities: &store.affinities()?,
            interests: &store.interests()?,
            galaxy_names: &names,
        },
        filter,
    ))
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
    use serde_json::json;
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

    fn names(rows: &[(u32, &str)]) -> HashMap<u32, String> {
        rows.iter().map(|(id, n)| (*id, n.to_string())).collect()
    }

    fn named_view(plays: &[Play], rows: &[(u32, &str)], filter: UniverseFilter) -> Universe {
        named(
            &UniverseInput {
                plays,
                affinities: &[],
                interests: &InterestMap::new(),
                galaxy_names: &names(rows),
            },
            filter,
        )
    }

    fn galaxy_of<'a>(u: &'a Universe, creator: &str) -> &'a str {
        &u.galaxies[planet(u, creator).galaxy as usize].name
    }

    /// Two pairs, each sharing two Sessions. A watch counts as 8 minutes, so in Both the channels are
    /// heavier and hold Galaxy 0, and the artists have Galaxy 1 there and Galaxy 0 in their own view.
    fn artists_and_channels() -> Vec<Play> {
        vec![
            spotify("Artist A", 0, 2),
            spotify("Artist B", 60, 2),
            spotify("Artist A", DAY, 2),
            spotify("Artist B", DAY + 60, 2),
            youtube("Channel X", 2 * DAY),
            youtube("Channel Y", 2 * DAY + 60),
            youtube("Channel X", 3 * DAY),
            youtube("Channel Y", 3 * DAY + 60),
        ]
    }

    #[test]
    fn filtered_view_names_follow_both() {
        let plays = artists_and_channels();
        let rows = [(0, "The Screen Hours"), (1, "The Headphone Hours")];

        let both = named_view(&plays, &rows, UniverseFilter::Both);
        assert_eq!(galaxy_of(&both, "Channel X"), "The Screen Hours");
        assert_eq!(galaxy_of(&both, "Artist A"), "The Headphone Hours");

        // The artists are Galaxy 0 here, so a name by id would be the channels' one.
        let artists = named_view(&plays, &rows, UniverseFilter::Spotify);
        assert_eq!(planet(&artists, "Artist A").galaxy, 0);
        assert_eq!(galaxy_of(&artists, "Artist A"), "The Headphone Hours");

        let channels = named_view(&plays, &rows, UniverseFilter::Youtube);
        assert_eq!(galaxy_of(&channels, "Channel X"), "The Screen Hours");
    }

    /// One Galaxy in Both, because a channel holds them together, and two in the view without it.
    #[test]
    fn names_do_not_repeat_in_a_view() {
        let mut plays = vec![
            spotify("Heavier", 0, 5),
            youtube("Channel", 60),
            spotify("Heavier", DAY, 5),
            youtube("Channel", DAY + 60),
            spotify("Lighter", 2 * DAY, 3),
            youtube("Channel", 2 * DAY + 60),
            spotify("Lighter", 3 * DAY, 3),
            youtube("Channel", 3 * DAY + 60),
        ];
        // Sessions the others are not in, so that sharing one with the channel means something.
        plays.extend((4..8).map(|day| youtube("Alone", day * DAY)));

        let both = from_plays(&plays, UniverseFilter::Both);
        let holder = planet(&both, "Channel").galaxy;
        assert_eq!(planet(&both, "Heavier").galaxy, holder);
        assert_eq!(planet(&both, "Lighter").galaxy, holder);
        let view = from_plays(&plays, UniverseFilter::Spotify);
        assert_ne!(
            planet(&view, "Heavier").galaxy,
            planet(&view, "Lighter").galaxy
        );

        let view = named_view(&plays, &[(holder, "The Only One")], UniverseFilter::Spotify);
        assert_eq!(galaxy_of(&view, "Heavier"), "The Only One");
        assert_eq!(galaxy_of(&view, "Lighter"), "Lighter");

        // And a Roast that gives two Galaxies one name, seen in the view those ids are for.
        let plays = artists_and_channels();
        let both = named_view(&plays, &[(0, "Twice"), (1, "Twice")], UniverseFilter::Both);
        assert_eq!(galaxy_of(&both, "Channel X"), "Twice");
        assert_eq!(galaxy_of(&both, "Artist A"), "Artist A");
    }

    fn at(name: &str, galaxy: u32) -> Planet {
        Planet {
            galaxy,
            ..from_plays(&[spotify(name, 0, 1)], UniverseFilter::Spotify).planets[0].clone()
        }
    }

    #[test]
    fn a_view_takes_its_name_from_the_first_member_still_in_both() {
        // The heaviest was cut from Both by the Planet cap, and a lighter one was not.
        let view = [at("Cut From Both", 0), at("Still There", 0)];
        let both = [at("Still There", 3)];
        assert_eq!(
            names_from_both(&view, 1, &both, &names(&[(3, "Found")])),
            names(&[(0, "Found")])
        );

        // The first one still there decides, even when its Galaxy has no name from the Roast.
        let both = [at("Cut From Both", 1), at("Still There", 2)];
        assert!(names_from_both(&view, 1, &both, &names(&[(2, "Other")])).is_empty());

        // Nobody left in Both means nothing to go by.
        assert!(names_from_both(&view, 1, &[], &names(&[(0, "Other")])).is_empty());
    }

    fn roast_naming(rows: &[(u32, &str)]) -> crate::roast::Roast {
        let names: Vec<_> = rows
            .iter()
            .map(|(id, n)| json!({"id": id, "name": n}))
            .collect();
        let reply = json!({
            "headline": "H",
            "opening": "O",
            "burns": [{"title": "T", "body": "B"}],
            "verdict": {"archetype": "A", "score": 1, "sentence": "S"},
            "galaxy_names": names,
        });
        crate::roast::parse_roast(&reply.to_string()).unwrap()
    }

    fn store_of(plays: &[Play]) -> Store {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_plays(plays).unwrap();
        store
    }

    #[test]
    fn galaxy_names_apply_by_id() {
        let store = store_of(&artists_and_channels());
        let both = || from_store(&store, UniverseFilter::Both).unwrap();
        assert_eq!(galaxy_of(&both(), "Channel X"), "Channel X");

        store
            .save_roast(
                "Claude Code",
                &roast_naming(&[(0, "The Screen Hours"), (1, "The Headphone Hours")]),
                1000,
                false,
            )
            .unwrap();

        assert_eq!(galaxy_of(&both(), "Channel X"), "The Screen Hours");
        assert_eq!(galaxy_of(&both(), "Artist A"), "The Headphone Hours");
        let artists = from_store(&store, UniverseFilter::Spotify).unwrap();
        assert_eq!(galaxy_of(&artists, "Artist A"), "The Headphone Hours");
    }

    #[test]
    fn stale_galaxy_id_is_ignored() {
        let store = store_of(&artists_and_channels());
        store
            .save_roast(
                "Claude Code",
                &roast_naming(&[(99, "Long Gone")]),
                1000,
                false,
            )
            .unwrap();
        for filter in [
            UniverseFilter::Both,
            UniverseFilter::Spotify,
            UniverseFilter::Youtube,
        ] {
            let u = from_store(&store, filter).unwrap();
            assert!(
                u.galaxies.iter().all(|g| g.name != "Long Gone"),
                "{filter:?}"
            );
        }
    }

    #[test]
    fn a_blank_galaxy_name_keeps_the_default() {
        let store = store_of(&artists_and_channels());
        store
            .save_roast(
                "Claude Code",
                &roast_naming(&[(0, " \u{200b}\u{0} "), (1, "Kept")]),
                1000,
                false,
            )
            .unwrap();
        let both = from_store(&store, UniverseFilter::Both).unwrap();
        assert_eq!(galaxy_of(&both, "Channel X"), "Channel X");
        assert_eq!(galaxy_of(&both, "Artist A"), "Kept");
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
