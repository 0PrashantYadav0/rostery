//! Fictional history for trying Rostery without an account. Every name is invented.

use crate::error::{Error, Result};
use crate::model::{Origin, Play, Source};
use crate::store::{InsertCount, Store};

#[derive(Clone, Debug, PartialEq)]
pub struct DemoData {
    pub plays: Vec<Play>,
    pub interests: Vec<(Source, String, Vec<String>)>,
}

struct Side {
    tags: [&'static str; 2],
    creators: [&'static str; 6],
}

/// A taste group: its Creators on each Source, plus two tags both Sources share. Two, because
/// sharing one of five Interests is too weak to make a Link, and the combined Universe should show
/// a channel linked to an artist it never shared a Session with.
struct Group {
    shared: [&'static str; 2],
    spotify: Side,
    youtube: Side,
}

const GROUPS: [Group; 5] = [
    Group {
        shared: ["dreamy", "headphones"],
        spotify: Side {
            tags: ["dream pop", "shoegaze"],
            creators: [
                "The Paper Lanterns",
                "Velvet Anchor",
                "Moth Season",
                "Pale Harbour Club",
                "Lull and Lantern",
                "Soft Cartography",
            ],
        },
        youtube: Side {
            tags: ["music production", "guitar pedals"],
            creators: [
                "Pedalboard Diaries",
                "Reverb Garden",
                "Tape Hiss TV",
                "Bedroom Mixdown",
                "Gentle Fuzz Lab",
                "Lighthouse Lofi",
            ],
        },
    },
    Group {
        shared: ["tinkering", "weekend projects"],
        spotify: Side {
            tags: ["chiptune", "synthwave"],
            creators: [
                "Slow Math Club",
                "Kernel Panic Orchestra",
                "Floppy Disco",
                "Null Pointer Choir",
                "Binary Tide",
                "Solder Saints",
            ],
        },
        youtube: Side {
            tags: ["retro computing", "electronics"],
            creators: [
                "Breadboard Bob",
                "Cache Miss",
                "Retro Repair Shed",
                "Tiny Compilers",
                "Soldering Sundays",
                "Bit Rot Weekly",
            ],
        },
    },
    Group {
        shared: ["family", "bedtime"],
        spotify: Side {
            tags: ["kids music", "singalong"],
            creators: [
                "Vanta Kids",
                "Puddle Parade",
                "The Marmalade Mice",
                "Sleepy Sprocket",
                "Captain Biscuit",
                "Tiny Orchestra of Nothing",
            ],
        },
        youtube: Side {
            tags: ["kids shows", "crafts"],
            creators: [
                "Sprout Sing-Along",
                "Crayon Cove",
                "Mr Kettle's Workshop",
                "Bubble Bus Stories",
                "Blanket Fort Club",
                "Otter Academy",
            ],
        },
    },
    Group {
        shared: ["heavy", "loud"],
        spotify: Side {
            tags: ["metal", "hard rock"],
            creators: [
                "Iron Hymnal",
                "Concrete Saint",
                "Thunderclap Mutiny",
                "Grim Parsley",
                "Barbell Choir",
                "Anvil Weather",
            ],
        },
        youtube: Side {
            tags: ["strength training", "fitness"],
            creators: [
                "Deadlift Diary",
                "Plate Math",
                "Chalk Dust Weekly",
                "Rack Pull Radio",
                "Gym Bro Anthropology",
                "Squat Cage Cinema",
            ],
        },
    },
    Group {
        shared: ["unwinding", "slow evenings"],
        spotify: Side {
            tags: ["jazz", "lounge"],
            creators: [
                "The Midnight Colanders",
                "Quarter Note Cafe",
                "Saffron Quartet",
                "Dusty Vermouth",
                "Tuesday Brass",
                "Blue Spoon Ensemble",
            ],
        },
        youtube: Side {
            tags: ["cooking", "food"],
            creators: [
                "Slow Simmer",
                "Knife Skills Academy",
                "One Pot Wonders",
                "Pantry Philosophy",
                "Sourdough Soliloquy",
                "Late Night Noodles",
            ],
        },
    },
];

const TRACK_A: [&str; 8] = [
    "Low", "Paper", "Quiet", "Velvet", "Midnight", "Slow", "Soft", "Hollow",
];
const TRACK_B: [&str; 8] = [
    "Tide", "Lantern", "Harbour", "Orbit", "Garden", "Weather", "Signal", "Season",
];
const VIDEO_A: [&str; 6] = [
    "Why",
    "How",
    "Inside",
    "Rebuilding",
    "Fixing",
    "The Truth About",
];
const VIDEO_B: [&str; 8] = [
    "Tide Tables",
    "Old Modems",
    "Cast Iron",
    "Bedtime",
    "Zero",
    "Cable Knots",
    "Quiet Pedals",
    "Sunday Soup",
];

const SESSIONS_PER_SOURCE: usize = 130;
const ITEMS_PER_CREATOR: usize = 4;
const DEMO_SEED: u64 = 2024;

/// A small linear congruential generator, so the same seed always gives the same demo.
pub(crate) struct Lcg(pub(crate) u64);

impl Lcg {
    /// 31 random bits.
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Inclusive range.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

fn item_names(rng: &mut Lcg, source: Source) -> Vec<String> {
    (0..ITEMS_PER_CREATOR)
        .map(|_| match source {
            Source::Spotify => format!("{} {}", TRACK_A[rng.below(8)], TRACK_B[rng.below(8)]),
            Source::Youtube => format!("{} {}", VIDEO_A[rng.below(6)], VIDEO_B[rng.below(8)]),
        })
        .collect()
}

fn side(group: &Group, source: Source) -> &Side {
    match source {
        Source::Spotify => &group.spotify,
        Source::Youtube => &group.youtube,
    }
}

/// Walks back from `now`. Each Session is 3 to 12 Plays, 2 to 5 minutes apart, from one
/// taste group, and Sessions are hours apart.
fn push_plays(rng: &mut Lcg, source: Source, now: i64, out: &mut Vec<Play>) {
    let items: Vec<Vec<Vec<String>>> = GROUPS
        .iter()
        .map(|g| {
            side(g, source)
                .creators
                .iter()
                .map(|_| item_names(rng, source))
                .collect()
        })
        .collect();

    let mut end = now - 30 * 60;
    for _ in 0..SESSIONS_PER_SOURCE {
        let group = rng.below(GROUPS.len());
        let count = rng.range(3, 12) as usize;
        let gaps: Vec<i64> = (1..count).map(|_| rng.range(150, 290)).collect();
        let start = end - gaps.iter().sum::<i64>();

        let mut played_at = start;
        for i in 0..count {
            if i > 0 {
                played_at += gaps[i - 1];
            }
            // Taking the lower of two rolls makes the first Creators the favourites.
            let creator = rng.below(6).min(rng.below(6));
            let item = rng.below(ITEMS_PER_CREATOR);
            let spotify = source == Source::Spotify;
            out.push(Play {
                source,
                origin: Origin::Demo,
                played_at,
                duration_ms: spotify.then(|| rng.range(120_000, 300_000)),
                creator: side(&GROUPS[group], source).creators[creator].to_string(),
                creator_ref: None,
                item: items[group][creator][item].clone(),
                item_ref: None,
                skipped: spotify && rng.below(12) == 0,
            });
        }
        end = start - rng.range(2 * 3600, 40 * 3600);
    }
}

pub fn generate(seed: u64, now: i64) -> DemoData {
    let mut rng = Lcg(seed ^ 0x9E37_79B9_7F4A_7C15);
    let mut plays = Vec::new();
    let mut interests = Vec::new();
    for source in [Source::Spotify, Source::Youtube] {
        push_plays(&mut rng, source, now, &mut plays);
        for group in &GROUPS {
            let s = side(group, source);
            for creator in s.creators {
                let tags = [s.tags[0], s.tags[1], group.shared[0], group.shared[1]];
                interests.push((source, creator.to_string(), tags.map(String::from).to_vec()));
            }
        }
    }
    plays.sort_by_key(|p| p.played_at);
    DemoData { plays, interests }
}

/// Stores the demo. The first load fixes the demo's clock, so loading again adds nothing. Real history
/// keeps it out, as real history arriving later clears it, so the two never mix in a Digest.
// ponytail: the clock is never reset, so an old demo ages; add a delete_setting to reset it on clear.
pub fn load(store: &mut Store, now: i64) -> Result<InsertCount> {
    if store.has_real_plays()? {
        return Err(Error::RealHistoryStored);
    }
    let anchor = match store.get_setting::<i64>("demo_now")? {
        Some(t) => t,
        None => {
            store.set_setting("demo_now", &now)?;
            now
        }
    };
    let demo = generate(DEMO_SEED, anchor);
    for (source, creator, tags) in &demo.interests {
        store.set_interests(*source, creator, tags, Origin::Demo.as_str(), anchor)?;
    }
    store.insert_plays(&demo.plays)
}

/// Removes demo Plays, Interests, Affinities and Roasts. What came from an Export or Connection stays.
pub fn clear(store: &mut Store) -> Result<u64> {
    store.delete_demo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Affinity;
    use crate::store::DEMO_KIND;
    use std::collections::HashSet;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn generate_is_deterministic_and_rich() {
        let a = generate(7, NOW);
        assert_eq!(a, generate(7, NOW));
        assert_ne!(a, generate(8, NOW));

        assert!(a.plays.len() >= 1000, "only {} plays", a.plays.len());
        assert!(a.plays.iter().all(|p| p.played_at < NOW));
        assert!(a.plays.iter().all(|p| p.origin == Origin::Demo));
        for source in [Source::Spotify, Source::Youtube] {
            let creators: HashSet<_> = a
                .plays
                .iter()
                .filter(|p| p.source == source)
                .map(|p| p.creator.as_str())
                .collect();
            assert!(
                creators.len() >= 20,
                "{source:?}: {} creators",
                creators.len()
            );
        }
        assert!(a.interests.iter().all(|(_, _, tags)| !tags.is_empty()));
    }

    #[test]
    fn sessions_stay_in_one_taste_group() {
        // Plays 2 to 5 minutes apart on one Source are one Session, and share a group of Creators.
        let plays = generate(7, NOW).plays;
        let tags: std::collections::HashMap<_, _> = generate(7, NOW)
            .interests
            .into_iter()
            .map(|(s, c, t)| ((s, c), t[0].clone()))
            .collect();
        let spotify: Vec<_> = plays
            .iter()
            .filter(|p| p.source == Source::Spotify)
            .collect();
        let mut mixed = 0;
        for pair in spotify.windows(2) {
            if pair[1].played_at - pair[0].played_at < 600
                && tags[&(Source::Spotify, pair[0].creator.clone())]
                    != tags[&(Source::Spotify, pair[1].creator.clone())]
            {
                mixed += 1;
            }
        }
        assert_eq!(mixed, 0);
    }

    #[test]
    fn loading_twice_does_not_double() {
        let mut store = Store::open_in_memory().unwrap();
        let first = load(&mut store, NOW).unwrap();
        let totals = store.totals().unwrap();
        assert!(totals.spotify.plays > 0 && totals.youtube.plays > 0);
        assert_eq!(first.duplicates, 0);

        // Even when the clock has moved on, the demo is the same demo.
        let second = load(&mut store, NOW + 86_400).unwrap();
        assert_eq!(second.added, 0);
        assert_eq!(store.totals().unwrap(), totals);
    }

    #[test]
    fn clear_removes_only_demo_plays() {
        let mut store = Store::open_in_memory().unwrap();
        load(&mut store, NOW).unwrap();
        let removed = clear(&mut store).unwrap();
        assert!(removed >= 1000);
        assert_eq!(store.totals().unwrap(), Default::default());
    }

    #[test]
    fn load_stores_demo_interests() {
        let mut store = Store::open_in_memory().unwrap();
        load(&mut store, NOW).unwrap();
        let interests = store.interests().unwrap();
        assert_eq!(interests.len(), 60);
        assert_eq!(
            interests[&(Source::Spotify, "Moth Season".to_string())],
            ["dream pop", "dreamy", "headphones", "shoegaze"]
        );
    }

    #[test]
    fn demo_data_stays_out_once_real_plays_are_stored() {
        let mut store = Store::open_in_memory().unwrap();
        let real = Play {
            source: Source::Spotify,
            origin: Origin::Export,
            played_at: NOW,
            duration_ms: Some(200_000),
            creator: "Real Band".into(),
            creator_ref: None,
            item: "Real Song".into(),
            item_ref: None,
            skipped: false,
        };
        store.insert_plays(std::slice::from_ref(&real)).unwrap();

        let message = load(&mut store, NOW).unwrap_err().to_string();

        assert!(message.contains("already here"), "{message}");
        assert!(message.contains("Universe"), "{message}");
        assert_eq!(store.plays(None).unwrap(), [real]);
        assert!(store.interests().unwrap().is_empty());
        assert_eq!(store.get_setting::<i64>("demo_now").unwrap(), None);
    }

    #[test]
    fn clear_removes_demo_interests_and_affinities_and_keeps_real_ones() {
        let mut store = Store::open_in_memory().unwrap();
        load(&mut store, NOW).unwrap();
        let liked = |kind: &str| Affinity {
            source: Source::Spotify,
            creator: "Moth Season".into(),
            creator_ref: None,
            kind: kind.into(),
            score: 10.0,
            fetched_at: NOW,
        };
        store
            .set_affinities(Source::Spotify, DEMO_KIND, &[liked(DEMO_KIND)])
            .unwrap();
        store
            .set_affinities(Source::Spotify, "top_artist", &[liked("top_artist")])
            .unwrap();
        let real = ["indie".to_string()];
        store
            .set_interests(Source::Spotify, "Moth Season", &real, "spotify_api", NOW)
            .unwrap();

        clear(&mut store).unwrap();

        // Only what came from a real Source is left, even on a Creator the demo also named.
        let interests = store.interests().unwrap();
        assert_eq!(interests.len(), 1);
        assert_eq!(
            interests[&(Source::Spotify, "Moth Season".to_string())],
            real
        );
        assert_eq!(store.affinities().unwrap(), [liked("top_artist")]);
        assert!(!store.has_demo_data().unwrap());
    }
}
