//! Who is in a Universe, how big they are, who is linked to whom, and which Galaxy each is in.

use super::{Galaxy, Link, Planet, TopItem, UniverseFilter, UniverseInput};
use crate::ingest::looks_like_id;
use crate::model::{Play, Source};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// Takeout has no durations, so a watch counts as this many milliseconds.
const WATCH_MS: i64 = 8 * 60_000;
const MIN_RADIUS: f64 = 0.6;
const RADIUS_RANGE: f64 = 3.4;
const MAX_PLANETS: usize = 150;
const TOP_ITEMS: usize = 5;
const SESSION_GAP_SECS: i64 = 30 * 60;
/// So a week of autoplay cannot form a giant clique.
const MAX_SESSION_CREATORS: usize = 50;
const MIN_SHARED_SESSIONS: u32 = 2;
const SESSION_SHARE: f64 = 0.6;
const INTEREST_SHARE: f64 = 0.4;
const MIN_STRENGTH: f64 = 0.12;
const TOP_LINKS: usize = 6;
const MAX_ROUNDS: usize = 20;
/// Brass, the app's own colour, for the heaviest Galaxy.
const FIRST_HUE: f64 = 0.11;
/// Stepping round the wheel by this ratio never lands two Galaxies close together.
const GOLDEN_RATIO: f64 = 0.618_033_988_749_895;

#[derive(Default)]
struct Tally<'a> {
    ms: i64,
    affinity: f64,
    plays: u64,
    url: Option<String>,
    items: HashMap<&'a str, ItemTally>,
}

#[derive(Default)]
struct ItemTally {
    plays: u64,
    url: Option<String>,
}

fn in_view(filter: UniverseFilter, source: Source) -> bool {
    match filter {
        UniverseFilter::Both => true,
        UniverseFilter::Spotify => source == Source::Spotify,
        UniverseFilter::Youtube => source == Source::Youtube,
    }
}

/// The heaviest Creators as Planets, heaviest first, with no position or Galaxy yet.
pub fn planets(input: &UniverseInput, filter: UniverseFilter) -> Vec<Planet> {
    let mut tallies: HashMap<(Source, &str), Tally> = HashMap::new();
    let named = |creator: &str| !creator.trim().is_empty();
    for p in input.plays {
        if !in_view(filter, p.source) || !named(&p.creator) {
            continue;
        }
        let t = tallies.entry((p.source, &p.creator)).or_default();
        t.ms += match p.duration_ms {
            Some(ms) => ms.max(0),
            None if p.source == Source::Youtube => WATCH_MS,
            None => 0,
        };
        if t.url.is_none() {
            t.url = p
                .creator_ref
                .as_deref()
                .and_then(|r| creator_url(p.source, r));
        }
        // A skip adds its time, but it does not show interest.
        if !p.skipped {
            t.plays += 1;
            let item = t.items.entry(&p.item).or_default();
            item.plays += 1;
            if item.url.is_none() {
                item.url = p.item_ref.as_deref().and_then(|r| item_url(p.source, r));
            }
        }
    }
    for a in input.affinities {
        if !in_view(filter, a.source) || !named(&a.creator) {
            continue;
        }
        let t = tallies.entry((a.source, &a.creator)).or_default();
        if a.score.is_finite() {
            t.affinity += a.score.max(0.0);
        }
        if t.url.is_none() {
            t.url = a
                .creator_ref
                .as_deref()
                .and_then(|r| creator_url(a.source, r));
        }
    }

    let minutes = |t: &Tally| t.ms as f64 / 60_000.0;
    let mut ranked: Vec<(f64, (Source, &str), Tally)> = tallies
        .into_iter()
        .map(|(key, t)| (minutes(&t) + t.affinity, key, t))
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked.truncate(MAX_PLANETS);

    let mut heaviest: HashMap<Source, f64> = HashMap::new();
    for (weight, (source, _), _) in &ranked {
        heaviest.entry(*source).or_insert(*weight);
    }

    ranked
        .into_iter()
        .enumerate()
        .map(|(id, (weight, (source, name), t))| {
            // With no weight at all there is nothing to compare, so every Planet is the smallest size.
            let share = match heaviest[&source] {
                0.0 => 0.0,
                max => weight / max,
            };
            let mut interests = input
                .interests
                .get(&(source, name.to_string()))
                .cloned()
                .unwrap_or_default();
            interests.sort();
            interests.dedup();
            Planet {
                id: id as u32,
                name: name.to_string(),
                source,
                minutes: minutes(&t),
                plays: t.plays,
                radius: MIN_RADIUS + RADIUS_RANGE * share.sqrt(),
                position: [0.0; 3],
                galaxy: 0,
                interests,
                url: t.url.clone().or_else(|| search_url(source, name)),
                top_items: top_items(t.items),
            }
        })
        .collect()
}

fn top_items(items: HashMap<&str, ItemTally>) -> Vec<TopItem> {
    let mut items: Vec<_> = items.into_iter().collect();
    items.sort_by(|a, b| b.1.plays.cmp(&a.1.plays).then_with(|| a.0.cmp(b.0)));
    items
        .into_iter()
        .take(TOP_ITEMS)
        .map(|(title, t)| TopItem {
            title: title.to_string(),
            url: t.url,
            plays: t.plays,
        })
        .collect()
}

// These strings came from untrusted files, and a page will open what these return. So each one is
// rebuilt from a checked ID, and can only ever be an https URL on the Source's own host.

fn creator_url(source: Source, reference: &str) -> Option<String> {
    let checked = |prefix| {
        reference
            .strip_prefix(prefix)
            .filter(|id| looks_like_id(id))
    };
    match source {
        Source::Spotify => {
            checked("spotify:artist:").map(|id| format!("https://open.spotify.com/artist/{id}"))
        }
        Source::Youtube => {
            checked("https://www.youtube.com/channel/").map(|_| reference.to_string())
        }
    }
}

fn item_url(source: Source, reference: &str) -> Option<String> {
    match source {
        Source::Spotify => {
            let (kind, id) = reference.strip_prefix("spotify:")?.split_once(':')?;
            (matches!(kind, "track" | "episode") && looks_like_id(id))
                .then(|| format!("https://open.spotify.com/{kind}/{id}"))
        }
        Source::Youtube => {
            looks_like_id(reference).then(|| format!("https://www.youtube.com/watch?v={reference}"))
        }
    }
}

/// An Export names an artist with no ID, so the way back to Spotify is a search for the name. A
/// YouTube search is not a page about the Creator, so a channel with no URL stays without one.
fn search_url(source: Source, name: &str) -> Option<String> {
    if source != Source::Spotify {
        return None;
    }
    let mut url = String::from("https://open.spotify.com/search/");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            url.push(b as char);
        } else {
            write!(url, "%{b:02X}").expect("writing to a String cannot fail");
        }
    }
    Some(url)
}

/// How many Sessions each Planet and each pair of Planets appear in.
struct Sessions {
    total: u32,
    per_planet: Vec<u32>,
    /// `together[a * n + b]`, with `a` below `b`.
    together: Vec<u32>,
}

/// A Session is a run of Plays, so a skip keeps it going, but only a Play that shows interest
/// counts as its Creator being there.
fn sessions(plays: &[Play], planets: &[Planet], filter: UniverseFilter) -> Sessions {
    let n = planets.len();
    let ids: HashMap<(Source, &str), usize> = planets
        .iter()
        .enumerate()
        .map(|(id, p)| ((p.source, p.name.as_str()), id))
        .collect();
    let mut in_order: Vec<&Play> = plays.iter().filter(|p| in_view(filter, p.source)).collect();
    in_order.sort_by_key(|p| p.played_at);

    let mut counts = Sessions {
        total: 0,
        per_planet: vec![0; n],
        together: vec![0; n * n],
    };
    for run in in_order.chunk_by(|a, b| b.played_at - a.played_at <= SESSION_GAP_SECS) {
        counts.total += 1;
        let mut members = Vec::new();
        for p in run.iter().filter(|p| !p.skipped) {
            if let Some(&id) = ids.get(&(p.source, p.creator.as_str())) {
                if members.len() < MAX_SESSION_CREATORS && !members.contains(&id) {
                    members.push(id);
                }
            }
        }
        for (i, &a) in members.iter().enumerate() {
            counts.per_planet[a] += 1;
            for &b in &members[i + 1..] {
                counts.together[a.min(b) * n + a.max(b)] += 1;
            }
        }
    }
    counts
}

/// Normalised pointwise mutual information, floored at 0.
fn npmi(c_ab: u32, c_a: u32, c_b: u32, sessions: u32) -> f64 {
    if c_ab < MIN_SHARED_SESSIONS {
        return 0.0;
    }
    // In every Session the formula is 0 / 0. They could not be more together, which is its limit.
    if c_ab == sessions {
        return 1.0;
    }
    let (c_ab, c_a, c_b, s) = (c_ab as f64, c_a as f64, c_b as f64, sessions as f64);
    ((c_ab * s / (c_a * c_b)).ln() / -(c_ab / s).ln()).max(0.0)
}

pub fn links(plays: &[Play], planets: &[Planet], filter: UniverseFilter) -> Vec<Link> {
    let n = planets.len();
    let counts = sessions(plays, planets, filter);
    let mut candidates = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let npmi = npmi(
                counts.together[a * n + b],
                counts.per_planet[a],
                counts.per_planet[b],
                counts.total,
            );
            let (tags_a, tags_b) = (&planets[a].interests, &planets[b].interests);
            let shared: Vec<String> = tags_a
                .iter()
                .filter(|tag| tags_b.binary_search(tag).is_ok())
                .cloned()
                .collect();
            let union = tags_a.len() + tags_b.len() - shared.len();
            let jaccard = if union == 0 {
                0.0
            } else {
                shared.len() as f64 / union as f64
            };
            let strength = SESSION_SHARE * npmi + INTEREST_SHARE * jaccard;
            if strength >= MIN_STRENGTH {
                candidates.push(Link {
                    a: a as u32,
                    b: b as u32,
                    strength,
                    shared,
                });
            }
        }
    }

    let mut touching: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, link) in candidates.iter().enumerate() {
        touching[link.a as usize].push(i);
        touching[link.b as usize].push(i);
    }
    let mut keep = vec![false; candidates.len()];
    for mut list in touching {
        // Stable, so a tie goes to the heavier Planet.
        list.sort_by(|&x, &y| candidates[y].strength.total_cmp(&candidates[x].strength));
        for &i in list.iter().take(TOP_LINKS) {
            keep[i] = true;
        }
    }
    candidates
        .into_iter()
        .zip(keep)
        .filter_map(|(link, keep)| keep.then_some(link))
        .collect()
}

/// Weighted label propagation. Planets are visited in `id` order, which is heaviest first, and each
/// takes the label its Links pull on hardest, with a tie going to the lowest label.
fn propagate_labels(n: usize, links: &[Link]) -> Vec<u32> {
    let mut neighbours: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for l in links {
        neighbours[l.a as usize].push((l.b as usize, l.strength));
        neighbours[l.b as usize].push((l.a as usize, l.strength));
    }
    let mut labels: Vec<u32> = (0..n as u32).collect();
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for (i, around) in neighbours.iter().enumerate() {
            let mut pulls: BTreeMap<u32, f64> = BTreeMap::new();
            for &(other, strength) in around {
                *pulls.entry(labels[other]).or_default() += strength;
            }
            let mut best = (labels[i], f64::NEG_INFINITY);
            for (label, pull) in pulls {
                if pull > best.1 {
                    best = (label, pull);
                }
            }
            changed |= best.0 != labels[i];
            labels[i] = best.0;
        }
        if !changed {
            break;
        }
    }
    labels
}

/// Sets each Planet's Galaxy. IDs follow the heaviest member, so Galaxy 0 holds the heaviest Planet.
pub fn galaxies(
    planets: &mut [Planet],
    links: &[Link],
    names: &HashMap<u32, String>,
) -> Vec<Galaxy> {
    let labels = propagate_labels(planets.len(), links);
    let mut ids: HashMap<u32, u32> = HashMap::new();
    for (planet, label) in planets.iter_mut().zip(labels) {
        let next = ids.len() as u32;
        planet.galaxy = *ids.entry(label).or_insert(next);
    }
    (0..ids.len() as u32)
        .map(|id| {
            let members: Vec<&Planet> = planets.iter().filter(|p| p.galaxy == id).collect();
            Galaxy {
                id,
                name: names
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| default_name(&members)),
                hue: (FIRST_HUE + id as f64 * GOLDEN_RATIO).fract(),
                size: members.len() as u32,
            }
        })
        .collect()
}

/// The most common Interest, the first in the alphabet on a tie, or else the heaviest member.
fn default_name(members: &[&Planet]) -> String {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for tag in members.iter().flat_map(|p| &p.interests) {
        *counts.entry(tag).or_default() += 1;
    }
    let mut best: Option<(&str, u32)> = None;
    for (tag, count) in counts {
        if best.is_none_or(|(_, most)| count > most) {
            best = Some((tag, count));
        }
    }
    best.map_or(members[0].name.as_str(), |(tag, _)| tag)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::*;
    use UniverseFilter::{Both, Spotify, Youtube};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn planet_size_follows_time() {
        let u = from_plays(
            &[spotify("Big", 0, 400), spotify("Small", DAY, 100)],
            Spotify,
        );
        assert!(close(planet(&u, "Big").radius, 4.0));
        assert!(close(planet(&u, "Small").radius, 2.3));
        assert!(close(planet(&u, "Big").minutes, 400.0));
        assert_eq!(u.planets[0].name, "Big");
    }

    #[test]
    fn youtube_watches_size_planets() {
        let plays: Vec<_> = (0..10).map(|i| youtube("Cache Miss", i * DAY)).collect();
        let u = from_plays(&plays, Youtube);
        assert!(close(u.planets[0].minutes, 80.0));
        assert_eq!(u.planets[0].plays, 10);
    }

    #[test]
    fn sources_scale_independently() {
        let mut plays = vec![
            spotify("Long Listen", 0, 100_000),
            spotify("Short Listen", DAY, 25_000),
        ];
        plays.push(youtube("Cache Miss", 2 * DAY));
        let u = from_plays(&plays, Both);
        assert!(close(planet(&u, "Long Listen").radius, 4.0));
        assert!(close(planet(&u, "Cache Miss").radius, 4.0));
        assert!(close(planet(&u, "Short Listen").radius, 2.3));
    }

    #[test]
    fn filter_excludes_other_source() {
        let plays = [spotify("Moth Season", 0, 3), youtube("Cache Miss", DAY)];
        let names = |filter| -> Vec<String> {
            from_plays(&plays, filter)
                .planets
                .into_iter()
                .map(|p| p.name)
                .collect()
        };
        assert_eq!(names(Spotify), ["Moth Season"]);
        assert_eq!(names(Youtube), ["Cache Miss"]);
        assert_eq!(names(Both).len(), 2);
        assert_eq!(from_plays(&plays, Youtube).filter, Youtube);
    }

    #[test]
    fn the_same_name_on_two_sources_is_two_planets() {
        let u = from_plays(&[spotify("Twin", 0, 3), youtube("Twin", DAY)], Both);
        assert_eq!(u.planets.len(), 2);
        assert!(u.planets.iter().any(|p| p.source == Source::Spotify));
        assert!(u.planets.iter().any(|p| p.source == Source::Youtube));
    }

    /// Plays in the same Session sit a minute apart. Sessions sit a day apart.
    fn sessions(groups: &[&[&str]]) -> Vec<Play> {
        groups
            .iter()
            .enumerate()
            .flat_map(|(s, group)| {
                group
                    .iter()
                    .enumerate()
                    .map(move |(i, c)| spotify(c, s as i64 * DAY + i as i64 * 60, 3))
            })
            .collect()
    }

    #[test]
    fn shared_sessions_make_a_link() {
        let plays = sessions(&[&["A", "B"], &["A", "B"], &["C"]]);
        let u = from_plays(&plays, Spotify);
        let link = linked(&u, "A", "B").expect("A and B share 2 Sessions");
        // They are together every time either appears, so npmi is 1 and there are no Interests.
        assert!(close(link.strength, 0.6));
        assert!(link.shared.is_empty());
        assert_eq!(u.links.len(), 1);
    }

    #[test]
    fn link_strength_follows_the_formula() {
        // S = 5, c_a = 3, c_b = 4, c_ab = 2, and they share 1 of 3 Interests.
        let plays = sessions(&[&["A", "B"], &["A", "B"], &["A"], &["B"], &["B"]]);
        let map = interests(&[
            (Source::Spotify, "A", &["jazz", "lounge"]),
            (Source::Spotify, "B", &["jazz", "metal"]),
        ]);
        let u = with_interests(&plays, &map, Spotify);
        let link = linked(&u, "A", "B").unwrap();
        let npmi = (2.0_f64 * 5.0 / (3.0 * 4.0)).ln() / -(2.0_f64 / 5.0).ln();
        assert!(npmi < 0.0, "this pair meets less often than chance");
        assert!(close(link.strength, 0.4 / 3.0), "npmi is floored at 0");
        assert_eq!(link.shared, ["jazz"]);

        // S = 6 and c_a = c_b = 3, so npmi is above 0 and counts.
        let plays = sessions(&[&["A", "B"], &["A", "B"], &["A"], &["B"], &["C"], &["C"]]);
        let u = from_plays(&plays, Spotify);
        let link = linked(&u, "A", "B").unwrap();
        let npmi = (2.0_f64 * 6.0 / (3.0 * 3.0)).ln() / -(2.0_f64 / 6.0).ln();
        assert!(close(link.strength, 0.6 * npmi));
    }

    #[test]
    fn one_shared_session_is_not_enough() {
        // They meet every time they appear, so only the count of 2 stops this from being a Link.
        let plays = sessions(&[&["A", "B"], &["C"], &["D"]]);
        let u = from_plays(&plays, Spotify);
        assert!(u.links.is_empty());
    }

    #[test]
    fn creators_in_every_session_get_a_finite_link() {
        // S = c_ab = c_a = c_b, where the formula on its own is 0 / 0.
        let u = from_plays(&sessions(&[&["A", "B"], &["A", "B"]]), Spotify);
        let link = linked(&u, "A", "B").unwrap();
        assert!(close(link.strength, 0.6));
    }

    #[test]
    fn session_gap_boundary() {
        let pairs = |gap: i64| {
            let plays = [
                spotify("A", 0, 3),
                spotify("B", gap, 3),
                spotify("A", DAY, 3),
                spotify("B", DAY + gap, 3),
                spotify("C", 2 * DAY, 3),
            ];
            from_plays(&plays, Spotify)
        };
        assert!(linked(&pairs(1800), "A", "B").is_some());
        assert!(linked(&pairs(1801), "A", "B").is_none());
    }

    #[test]
    fn a_session_is_capped_at_50_creators() {
        let names: Vec<String> = (0..60).map(|i| format!("Creator {i:02}")).collect();
        let group: Vec<&str> = names.iter().map(String::as_str).collect();
        let u = from_plays(&sessions(&[&group, &group, &["Elsewhere"]]), Spotify);
        let linked_ids: std::collections::HashSet<u32> =
            u.links.iter().flat_map(|l| [l.a, l.b]).collect();
        assert!(linked_ids.contains(&planet(&u, "Creator 00").id));
        assert!(linked_ids.contains(&planet(&u, "Creator 49").id));
        for late in 50..60 {
            let id = planet(&u, &format!("Creator {late}")).id;
            assert!(!linked_ids.contains(&id), "Creator {late} is past the cap");
        }
    }

    #[test]
    fn shared_interests_make_a_link() {
        let plays = [spotify("A", 0, 3), spotify("B", DAY, 3)];
        let map = interests(&[
            (Source::Spotify, "A", &["shoegaze", "dream pop"]),
            (Source::Spotify, "B", &["dream pop", "shoegaze"]),
        ]);
        let u = with_interests(&plays, &map, Spotify);
        let link = linked(&u, "A", "B").expect("same 2 Interests");
        assert!(close(link.strength, 0.4));
        assert_eq!(link.shared, ["dream pop", "shoegaze"]);
        assert_eq!(planet(&u, "A").interests, ["dream pop", "shoegaze"]);
    }

    #[test]
    fn a_link_needs_strength_of_at_least_0_12() {
        let plays = [spotify("A", 0, 3), spotify("B", DAY, 3)];
        let strength = |a: &[&str], b: &[&str]| {
            let map = interests(&[(Source::Spotify, "A", a), (Source::Spotify, "B", b)]);
            linked(&with_interests(&plays, &map, Spotify), "A", "B")
        };
        // 1 of 3 gives 0.133, and 1 of 5 gives 0.08.
        assert!(strength(&["x", "a"], &["x", "b"]).is_some());
        assert!(strength(&["x", "a", "c"], &["x", "b", "d"]).is_none());
    }

    #[test]
    fn a_link_stays_only_in_the_top_6_of_one_of_its_planets() {
        // 8 Creators with the same Interests are 28 equal pairs, and ties go to the heaviest.
        let plays: Vec<_> = (0..8)
            .map(|i| spotify(&format!("C{i}"), i * DAY, 100 - i))
            .collect();
        let tag: &[&str] = &["jazz"];
        let rows: Vec<(Source, String)> =
            (0..8).map(|i| (Source::Spotify, format!("C{i}"))).collect();
        let rows: Vec<(Source, &str, &[&str])> =
            rows.iter().map(|(s, c)| (*s, c.as_str(), tag)).collect();
        let u = with_interests(&plays, &interests(&rows), Spotify);
        // C6 and C7 both fill their 6 places with C0 to C5.
        assert!(linked(&u, "C6", "C7").is_none());
        assert!(linked(&u, "C5", "C6").is_some());
        assert_eq!(u.links.len(), 27);
    }

    #[test]
    fn cross_source_link_only_in_both_view() {
        let tags: &[&str] = &["lo-fi", "study"];
        let map = interests(&[
            (Source::Spotify, "Pale Harbour Club", tags),
            (Source::Youtube, "Lighthouse Lofi", tags),
        ]);
        let plays = [
            spotify("Pale Harbour Club", 0, 3),
            youtube("Lighthouse Lofi", DAY),
        ];
        let both = with_interests(&plays, &map, Both);
        assert!(linked(&both, "Pale Harbour Club", "Lighthouse Lofi").is_some());
        assert!(with_interests(&plays, &map, Spotify).links.is_empty());
        assert!(with_interests(&plays, &map, Youtube).links.is_empty());

        // Sessions count across Sources too, but only where both Sources are in view.
        let plays = [
            spotify("A", 0, 3),
            youtube("B", 60),
            spotify("A", DAY, 3),
            youtube("B", DAY + 60),
            spotify("C", 2 * DAY, 3),
        ];
        assert!(linked(&from_plays(&plays, Both), "A", "B").is_some());
        assert!(from_plays(&plays, Spotify).links.is_empty());
    }

    #[test]
    fn a_play_out_of_view_does_not_join_a_session() {
        // Only the YouTube watch between them makes A and B one Session.
        let plays = [
            spotify("A", 0, 3),
            youtube("Y", 1500),
            spotify("B", 3000, 3),
            spotify("A", DAY, 3),
            youtube("Y", DAY + 1500),
            spotify("B", DAY + 3000, 3),
            spotify("C", 2 * DAY, 3),
        ];
        assert!(linked(&from_plays(&plays, Both), "A", "B").is_some());
        assert!(linked(&from_plays(&plays, Spotify), "A", "B").is_none());
    }

    #[test]
    fn skipped_plays_add_minutes_but_show_no_interest() {
        let mut skipped = spotify("Moth Season", DAY, 1);
        skipped.skipped = true;
        skipped.item = "Never Finished".into();
        let u = from_plays(&[spotify("Moth Season", 0, 3), skipped], Spotify);
        let moth = planet(&u, "Moth Season");
        assert!(close(moth.minutes, 4.0));
        assert_eq!(moth.plays, 1);
        assert_eq!(moth.top_items.len(), 1);
        assert_eq!(moth.top_items[0].title, "Low Tide");
    }

    #[test]
    fn a_skip_does_not_count_as_a_shared_session() {
        let skip = |creator, at| Play {
            skipped: true,
            ..spotify(creator, at, 0)
        };
        let plays = [
            spotify("A", 0, 3),
            skip("B", 60),
            spotify("A", DAY, 3),
            skip("B", DAY + 60),
            spotify("C", 2 * DAY, 3),
        ];
        let u = from_plays(&plays, Spotify);
        assert!(linked(&u, "A", "B").is_none());
        assert_eq!(planet(&u, "B").plays, 0);
    }

    #[test]
    fn a_skip_still_keeps_a_session_going() {
        // A Session is a run of Plays, and a skipped Play is still a Play.
        let skip = |at| Play {
            skipped: true,
            ..spotify("Between", at, 0)
        };
        let plays = [
            spotify("A", 0, 3),
            skip(1800),
            spotify("B", 3600, 3),
            spotify("A", DAY, 3),
            skip(DAY + 1800),
            spotify("B", DAY + 3600, 3),
            spotify("C", 2 * DAY, 3),
        ];
        assert!(linked(&from_plays(&plays, Spotify), "A", "B").is_some());
    }

    #[test]
    fn plays_may_arrive_in_any_order() {
        let plays = sessions(&[&["A", "B"], &["A", "B"], &["C"]]);
        let mut shuffled = plays.clone();
        shuffled.reverse();
        assert_eq!(from_plays(&plays, Spotify), from_plays(&shuffled, Spotify));
    }

    #[test]
    fn galaxies_group_linked_planets() {
        let plays = sessions(&[&["A", "B"], &["A", "B"], &["C", "D"], &["C", "D"]]);
        let u = from_plays(&plays, Spotify);
        assert_eq!(u.galaxies.len(), 2);
        assert_eq!(planet(&u, "A").galaxy, planet(&u, "B").galaxy);
        assert_eq!(planet(&u, "C").galaxy, planet(&u, "D").galaxy);
        assert_ne!(planet(&u, "A").galaxy, planet(&u, "C").galaxy);
        assert!(u.galaxies.iter().all(|g| g.size == 2));
        assert_eq!(
            u.galaxies.iter().map(|g| g.id).collect::<Vec<_>>(),
            [0, 1],
            "Galaxy IDs have no gaps"
        );
    }

    #[test]
    fn a_planet_pulled_equally_two_ways_joins_the_heavier_galaxy() {
        // X has the same Links to each pair, so its pulls tie and the lower label wins.
        let plays = [
            spotify("A", 0, 50),
            spotify("C", DAY, 40),
            spotify("B", 2 * DAY, 30),
            spotify("D", 3 * DAY, 20),
            spotify("X", 4 * DAY, 10),
        ];
        let map = interests(&[
            (Source::Spotify, "A", &["jazz"]),
            (Source::Spotify, "B", &["jazz"]),
            (Source::Spotify, "C", &["metal"]),
            (Source::Spotify, "D", &["metal"]),
            (Source::Spotify, "X", &["jazz", "metal"]),
        ]);
        let u = with_interests(&plays, &map, Spotify);
        assert_eq!(u.galaxies.len(), 2);
        assert_eq!(planet(&u, "X").galaxy, planet(&u, "A").galaxy);
        assert_ne!(planet(&u, "X").galaxy, planet(&u, "C").galaxy);
    }

    #[test]
    fn the_first_galaxy_holds_the_heaviest_planet() {
        let plays = [spotify("Light", 0, 1), spotify("Heavy", DAY, 9)];
        let u = from_plays(&plays, Spotify);
        assert_eq!(u.planets[0].name, "Heavy");
        assert_eq!(u.planets[0].galaxy, 0);
        assert_eq!(u.planets[1].galaxy, 1);
    }

    #[test]
    fn galaxy_names_default_and_can_be_replaced() {
        let plays = [
            spotify("A", 0, 9),
            spotify("B", DAY, 3),
            spotify("Nameless", 2 * DAY, 2),
        ];
        let map = interests(&[
            (Source::Spotify, "A", &["jazz", "rare"]),
            (Source::Spotify, "B", &["jazz", "other"]),
        ]);
        let u = with_interests(&plays, &map, Spotify);
        assert_eq!(u.galaxies[planet(&u, "A").galaxy as usize].name, "jazz");
        let nameless = planet(&u, "Nameless").galaxy;
        assert_eq!(u.galaxies[nameless as usize].name, "Nameless");

        // A name from a Roast wins, and one for a Galaxy that no longer exists does nothing.
        let names = HashMap::from([(0, "Late Night Jazz Cult".to_string()), (99, "Gone".into())]);
        let named = build(
            &UniverseInput {
                plays: &plays,
                affinities: &[],
                interests: &map,
                galaxy_names: &names,
            },
            Spotify,
        );
        assert_eq!(named.galaxies[0].name, "Late Night Jazz Cult");
        assert_eq!(named.galaxies[1].name, "Nameless");
    }

    #[test]
    fn galaxy_hues_differ() {
        let plays: Vec<_> = (0..12)
            .map(|i| spotify(&format!("C{i}"), i * DAY, 3))
            .collect();
        let u = from_plays(&plays, Spotify);
        assert_eq!(u.galaxies.len(), 12);
        let mut hues: Vec<f64> = u.galaxies.iter().map(|g| g.hue).collect();
        hues.sort_by(f64::total_cmp);
        assert!(hues.iter().all(|h| (0.0..1.0).contains(h)));
        assert!(hues.windows(2).all(|h| h[1] - h[0] > 0.02), "{hues:?}");
    }

    #[test]
    fn deleted_videos_make_no_planet() {
        let plays = [
            youtube("", 0),
            youtube("   ", 60),
            youtube("Cache Miss", DAY),
        ];
        let u = from_plays(&plays, Youtube);
        assert_eq!(u.planets.len(), 1);
        assert_eq!(u.planets[0].name, "Cache Miss");
    }

    fn affinity(source: Source, creator: &str, kind: &str, score: f64) -> Affinity {
        Affinity {
            source,
            creator: creator.into(),
            creator_ref: None,
            kind: kind.into(),
            score,
            fetched_at: 0,
        }
    }

    fn with_affinities(
        plays: &[Play],
        affinities: &[Affinity],
        filter: UniverseFilter,
    ) -> Universe {
        build(
            &UniverseInput {
                plays,
                affinities,
                interests: &InterestMap::new(),
                galaxy_names: &HashMap::new(),
            },
            filter,
        )
    }

    #[test]
    fn affinity_only_planet() {
        let a = [affinity(
            Source::Spotify,
            "Grim Parsley",
            "top_artist",
            500.0,
        )];
        let u = with_affinities(&[], &a, Spotify);
        let p = planet(&u, "Grim Parsley");
        assert_eq!((p.plays, p.minutes), (0, 0.0));
        assert!(close(p.radius, 4.0));
        assert!(with_affinities(&[], &a, Youtube).planets.is_empty());
    }

    #[test]
    fn affinity_score_adds_to_weight() {
        // 100 minutes and 2 scores of 150 make 400, against 100 minutes.
        let a = [
            affinity(Source::Spotify, "A", "top_artist", 150.0),
            affinity(Source::Spotify, "A", "other", 150.0),
        ];
        let plays = [spotify("A", 0, 100), spotify("B", DAY, 100)];
        let u = with_affinities(&plays, &a, Spotify);
        assert!(close(planet(&u, "A").radius, 4.0));
        assert!(
            close(planet(&u, "A").minutes, 100.0),
            "minutes stay minutes"
        );
        assert!(close(planet(&u, "B").radius, 2.3));
    }

    #[test]
    fn weight_of_nothing_stays_finite() {
        // A row with no duration, an instant skip, and scores no Source should ever send.
        let mut no_time = spotify("No Time", 0, 0);
        no_time.duration_ms = None;
        let mut backwards = spotify("Backwards", DAY, 0);
        backwards.duration_ms = Some(-60_000);
        let a = [
            affinity(Source::Spotify, "NaN", "top_artist", f64::NAN),
            affinity(Source::Spotify, "Infinite", "top_artist", f64::INFINITY),
            affinity(Source::Spotify, "Negative", "top_artist", -5.0),
        ];
        let u = with_affinities(&[no_time, backwards], &a, Spotify);
        assert_eq!(u.planets.len(), 5);
        for p in &u.planets {
            assert!(close(p.radius, 0.6), "{}: {}", p.name, p.radius);
            assert!(p.position.iter().all(|c| c.is_finite()), "{}", p.name);
            assert_eq!(p.minutes, 0.0);
        }
    }

    #[test]
    fn planet_cap_keeps_heaviest() {
        let plays: Vec<_> = (0..300)
            .map(|i| spotify(&format!("Creator {i:03}"), i * DAY, 1000 - i))
            .collect();
        let u = from_plays(&plays, Spotify);
        assert_eq!(u.planets.len(), 150);
        assert_eq!(u.planets[0].name, "Creator 000");
        assert_eq!(u.planets[149].name, "Creator 149");
        assert!(u
            .planets
            .iter()
            .enumerate()
            .all(|(i, p)| p.id as usize == i));
        assert!(u.planets.windows(2).all(|p| p[0].radius >= p[1].radius));
    }

    fn in_source(u: &Universe, source: Source) -> usize {
        u.planets.iter().filter(|p| p.source == source).count()
    }

    #[test]
    fn the_cap_in_both_goes_by_weight_alone() {
        // Ranking on share of each Source would keep 1 artist and 149 channels, and giving each
        // Source half would keep 75 of each. One watch is not months of listening.
        let mut plays: Vec<_> = (0..200)
            .map(|i| {
                let minutes = if i == 0 { 100_000 } else { 40_000 - i };
                spotify(&format!("Artist {i:03}"), i * DAY, minutes)
            })
            .collect();
        plays.extend((0..200).map(|i| youtube(&format!("Channel {i:03}"), (1000 + i) * DAY)));
        plays.push(youtube("Channel 199", 2000 * DAY));

        let u = from_plays(&plays, Both);
        assert_eq!(in_source(&u, Source::Spotify), 150);
        assert_eq!(in_source(&u, Source::Youtube), 0);
        assert_eq!(u.planets[149].name, "Artist 149");

        // The channels are still in their own view, heaviest first.
        let channels = from_plays(&plays, Youtube);
        assert_eq!(channels.planets.len(), 150);
        assert_eq!(channels.planets[0].name, "Channel 199");
    }

    #[test]
    fn the_cap_in_both_keeps_a_heavy_channel_over_a_lighter_artist() {
        // 200 artists of 1,000 minutes down to 801, and channels of 1,040, 880 and 800.
        let mut plays: Vec<_> = (0..200)
            .map(|i| spotify(&format!("Artist {i:03}"), i * DAY, 1000 - i))
            .collect();
        for (name, watches) in [("Big", 130), ("Middle", 110), ("Light", 100)] {
            plays.extend((0..watches).map(|w| youtube(name, 1000 * DAY + w)));
        }
        let u = from_plays(&plays, Both);
        assert_eq!(u.planets.len(), 150);
        assert_eq!(u.planets[0].name, "Big");
        assert!(u.planets.iter().any(|p| p.name == "Middle"));
        assert!(!u.planets.iter().any(|p| p.name == "Light"));
        assert_eq!(in_source(&u, Source::Spotify), 148);
        assert!(close(planet(&u, "Big").radius, 4.0));
        assert!(close(planet(&u, "Artist 000").radius, 4.0));
    }

    #[test]
    fn ids_in_both_run_by_real_weight_and_the_heaviest_names_its_galaxy() {
        // 16 minutes is all of Small Channel's Source, and 40,000 is under half of Big Artist's.
        let plays = [
            spotify("Top Artist", 0, 100_000),
            spotify("Big Artist", DAY, 20_000),
            youtube("Small Channel", DAY + 60),
            spotify("Big Artist", 2 * DAY, 20_000),
            youtube("Small Channel", 2 * DAY + 60),
        ];
        let u = from_plays(&plays, Both);
        let names: Vec<_> = u.planets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Top Artist", "Big Artist", "Small Channel"]);
        assert!(
            close(planet(&u, "Small Channel").radius, 4.0),
            "sizes stay per Source"
        );

        let small = planet(&u, "Small Channel");
        assert_eq!(small.galaxy, planet(&u, "Big Artist").galaxy);
        assert_eq!(u.galaxies[small.galaxy as usize].name, "Big Artist");
    }

    #[test]
    fn planet_urls_point_at_source() {
        let mut track = spotify("Moth Season", 0, 3);
        track.item_ref = Some("spotify:track:abc".into());
        let mut episode = spotify("Slow Show", DAY, 3);
        episode.item_ref = Some("spotify:episode:def".into());
        let mut video = youtube("Cache Miss", 2 * DAY);
        video.item_ref = Some("dQw4w9WgXcQ".into());
        let u = from_plays(&[track, episode, video], Both);
        let url = |name| planet(&u, name).top_items[0].url.as_deref();
        assert_eq!(
            url("Moth Season"),
            Some("https://open.spotify.com/track/abc")
        );
        assert_eq!(
            url("Slow Show"),
            Some("https://open.spotify.com/episode/def")
        );
        assert_eq!(
            url("Cache Miss"),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
    }

    #[test]
    fn planet_url_goes_back_to_its_source() {
        let mut artist = spotify("Moth Season", 0, 3);
        artist.creator_ref = Some("spotify:artist:4Z8W4fKeB5YxbusRsdQVPb".into());
        let mut channel = youtube("Cache Miss", DAY);
        channel.creator_ref = Some("https://www.youtube.com/channel/UCabc-123_x".into());
        let no_ref_spotify = spotify("AC/DC & Friends?", 2 * DAY, 3);
        let accented = spotify("Björk #1", 3 * DAY, 3);
        let no_ref_youtube = youtube("Plate Math", 4 * DAY);
        let u = from_plays(
            &[artist, channel, no_ref_spotify, accented, no_ref_youtube],
            Both,
        );
        let url = |name| planet(&u, name).url.as_deref();
        assert_eq!(
            url("Moth Season"),
            Some("https://open.spotify.com/artist/4Z8W4fKeB5YxbusRsdQVPb")
        );
        assert_eq!(
            url("Cache Miss"),
            Some("https://www.youtube.com/channel/UCabc-123_x")
        );
        assert_eq!(
            url("AC/DC & Friends?"),
            Some("https://open.spotify.com/search/AC%2FDC%20%26%20Friends%3F")
        );
        assert_eq!(
            url("Björk #1"),
            Some("https://open.spotify.com/search/Bj%C3%B6rk%20%231")
        );
        assert_eq!(url("Plate Math"), None);
    }

    #[test]
    fn a_ref_from_an_affinity_gives_a_url_too() {
        let mut a = affinity(Source::Spotify, "Moth Season", "top_artist", 5.0);
        a.creator_ref = Some("spotify:artist:abc".into());
        let u = with_affinities(&[spotify("Moth Season", 0, 3)], &[a], Spotify);
        assert_eq!(
            u.planets[0].url.as_deref(),
            Some("https://open.spotify.com/artist/abc")
        );
    }

    #[test]
    fn a_hostile_ref_never_becomes_a_url() {
        let hostile = [
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "http://www.youtube.com/channel/abc",
            "https://evil.example/channel/abc",
            "https://www.youtube.com.evil.example/channel/abc",
            "https://www.youtube.com/channel/../../redirect?q=https://evil.example",
            "https://www.youtube.com/channel/a b",
            "https://www.youtube.com/channel/",
            "spotify:artist:",
            "spotify:artist:a/../../b",
            "spotify:artist:a?b=c",
            "spotify:track:a\"><img src=x onerror=alert(1)>",
            "spotify:album:abc",
            "a/b",
            "a?b",
            "",
        ];
        let mut plays = vec![];
        for (i, r) in hostile.iter().enumerate() {
            let at = i as i64 * DAY;
            let mut s = spotify(&format!("S{i}"), at, 3);
            s.creator_ref = Some(r.to_string());
            s.item_ref = Some(r.to_string());
            let mut y = youtube(&format!("Y{i}"), at + 3600);
            y.creator_ref = Some(r.to_string());
            y.item_ref = Some(r.to_string());
            plays.extend([s, y]);
        }
        let u = from_plays(&plays, Both);
        assert_eq!(u.planets.len(), hostile.len() * 2);
        for p in &u.planets {
            let item = p.top_items[0].url.as_deref();
            let url = p.url.as_deref();
            match p.source {
                // With no usable artist ref, a Spotify Planet searches for its own name.
                Source::Spotify => {
                    assert_eq!(
                        url.unwrap(),
                        format!("https://open.spotify.com/search/{}", p.name)
                    );
                    assert_eq!(item, None, "{}", p.name);
                }
                Source::Youtube => {
                    assert_eq!(url, None, "{}", p.name);
                    assert_eq!(item, None, "{}", p.name);
                }
            }
        }
    }

    #[test]
    fn top_items_are_the_most_played_with_their_links() {
        let mut plays = vec![];
        for (i, (title, count)) in [
            ("One", 1),
            ("Six", 6),
            ("Two", 2),
            ("Three", 3),
            ("Four", 4),
            ("Five", 5),
            ("Also One", 1),
        ]
        .into_iter()
        .enumerate()
        {
            for n in 0..count {
                let mut p = spotify("Moth Season", (i as i64) * DAY + n * 600, 3);
                p.item = title.into();
                // A later copy of an Item can have the link an earlier one lacks.
                if title == "Six" && n == 5 {
                    p.item_ref = Some("spotify:track:six".into());
                }
                plays.push(p);
            }
        }
        let u = from_plays(&plays, Spotify);
        let top = &u.planets[0].top_items;
        let titles: Vec<_> = top.iter().map(|t| (t.title.as_str(), t.plays)).collect();
        assert_eq!(
            titles,
            [
                ("Six", 6),
                ("Five", 5),
                ("Four", 4),
                ("Three", 3),
                ("Two", 2)
            ]
        );
        assert_eq!(
            top[0].url.as_deref(),
            Some("https://open.spotify.com/track/six")
        );
        assert_eq!(top[1].url, None);
    }
}
