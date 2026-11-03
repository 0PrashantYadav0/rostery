//! Where each Planet goes: Galaxies on a sphere, then a seeded force simulation, then a final pass
//! that pushes apart anything still overlapping. The same input always gives the same picture.

use super::{Link, Planet};
use crate::demo::Lcg;

const STEPS: usize = 300;
/// How much room a Galaxy gets for the Planets in it, compared with their packed size.
const GALAXY_ROOM: f64 = 2.5;
const REPULSION: f64 = 1.0;
const LINK_PULL: f64 = 0.12;
/// A linked pair rests this much further apart than their surfaces touching.
const LINK_GAP: f64 = 4.0;
const GALAXY_PULL: f64 = 0.06;
const STEP_SIZE: f64 = 0.35;
/// The furthest a Planet moves in one step, so a tight start cannot fling it away.
const MAX_MOVE: f64 = 2.0;
/// Planets end at least this far apart, surface to surface.
const CLEARANCE: f64 = 0.4;
const PUSH_SWEEPS: usize = 2000;
/// Below this a pair is in one spot, with no direction between them to push along.
const TOUCHING: f64 = 1e-9;
/// Repulsion stops growing at this distance, so two Planets that start close cannot explode.
const CLOSEST: f64 = 0.1;

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// How far apart two points are, and the way from `b` to `a`. A pair in one spot has no such way,
/// so it gets a fixed one, which keeps the numbers finite and the result the same every time.
fn gap(a: [f64; 3], b: [f64; 3]) -> (f64, [f64; 3]) {
    let between = sub(a, b);
    let d = length(between);
    if d < TOUCHING {
        (d, [1.0, 0.0, 0.0])
    } else {
        (d, between.map(|c| c / d))
    }
}

/// Adds `amount` to `a` along `way` and takes it from `b`.
fn apart(values: &mut [[f64; 3]], a: usize, b: usize, way: [f64; 3], amount: f64) {
    for axis in 0..3 {
        values[a][axis] += way[axis] * amount;
        values[b][axis] -= way[axis] * amount;
    }
}

/// Evenly spread points on a sphere, so no Galaxy has to sit on top of another.
fn fibonacci_sphere(i: usize, n: usize, radius: f64) -> [f64; 3] {
    let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
    let around = (1.0 - y * y).sqrt();
    let angle = i as f64 * golden_angle;
    [
        radius * around * angle.cos(),
        radius * y,
        radius * around * angle.sin(),
    ]
}

/// FNV-1a. The standard library's hasher may change between releases, and the layout must not.
fn seed(planet: &Planet) -> u64 {
    let source = planet.source.as_str().bytes();
    source
        .chain(planet.name.bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, b| {
            (hash ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// From -1 up to 1.
fn jitter(rng: &mut Lcg) -> f64 {
    rng.next() as f64 / (1_u64 << 30) as f64 - 1.0
}

pub fn place(planets: &mut [Planet], links: &[Link], galaxy_count: usize) {
    if planets.is_empty() {
        return;
    }
    // Each Galaxy gets a ball sized by what is in it, and the sphere has room for those balls.
    let mut volume = vec![0.0; galaxy_count];
    for p in planets.iter() {
        volume[p.galaxy as usize] += p.radius.powi(3);
    }
    let room: Vec<f64> = volume.iter().map(|v| GALAXY_ROOM * v.cbrt()).collect();
    let sphere = room.iter().map(|r| r * r).sum::<f64>().sqrt();
    let centres: Vec<[f64; 3]> = (0..galaxy_count)
        .map(|i| fibonacci_sphere(i, galaxy_count, sphere))
        .collect();

    let mut positions: Vec<[f64; 3]> = planets
        .iter()
        .map(|p| {
            let mut rng = Lcg(seed(p));
            let centre = centres[p.galaxy as usize];
            let spread = room[p.galaxy as usize];
            centre.map(|c| c + spread * jitter(&mut rng))
        })
        .collect();

    let radii: Vec<f64> = planets.iter().map(|p| p.radius).collect();
    let n = planets.len();
    for step in 0..STEPS {
        let cooling = 1.0 - step as f64 / STEPS as f64;
        let mut forces = vec![[0.0; 3]; n];
        for a in 0..n {
            for b in a + 1..n {
                let (d, way) = gap(positions[a], positions[b]);
                let reach = radii[a] + radii[b] + LINK_GAP;
                let push = REPULSION * reach * reach / d.max(CLOSEST).powi(2);
                apart(&mut forces, a, b, way, push);
            }
        }
        for l in links {
            let (a, b) = (l.a as usize, l.b as usize);
            let (d, way) = gap(positions[a], positions[b]);
            let rest = radii[a] + radii[b] + LINK_GAP;
            apart(&mut forces, a, b, way, -LINK_PULL * l.strength * (d - rest));
        }
        for (i, position) in positions.iter_mut().enumerate() {
            let home = sub(centres[planets[i].galaxy as usize], *position);
            let moved = [0, 1, 2]
                .map(|axis| (forces[i][axis] + GALAXY_PULL * home[axis]) * STEP_SIZE * cooling);
            let far = length(moved);
            let scale = if far > MAX_MOVE { MAX_MOVE / far } else { 1.0 };
            for axis in 0..3 {
                position[axis] += moved[axis] * scale;
            }
        }
    }

    // The final pass. Nothing holds a Planet in place, so each push makes room. It takes 3 sweeps
    // with 150 of the largest Planets in one Galaxy, and none on the demo data.
    // ponytail: past the limit a pair could stay overlapped. If a real Universe ever gets there,
    // scale every position outward by the worst overlap.
    for _ in 0..PUSH_SWEEPS {
        let mut moved = false;
        for a in 0..n {
            for b in a + 1..n {
                let wanted = radii[a] + radii[b] + CLEARANCE;
                let (d, way) = gap(positions[a], positions[b]);
                if d < wanted - TOUCHING {
                    apart(&mut positions, a, b, way, (wanted - d) / 2.0);
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    for (planet, position) in planets.iter_mut().zip(positions) {
        planet.position = position;
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::super::*;
    use crate::{demo, store::Store};
    use UniverseFilter::{Both, Spotify, Youtube};

    fn assert_sound(u: &Universe) {
        for p in &u.planets {
            assert!(p.position.iter().all(|c| c.is_finite()), "{p:?}");
        }
        for (i, a) in u.planets.iter().enumerate() {
            for b in &u.planets[i + 1..] {
                let gap = distance(a.position, b.position);
                assert!(
                    gap >= a.radius + b.radius,
                    "{} and {} overlap: {gap} apart with radii {} and {}",
                    a.name,
                    b.name,
                    a.radius,
                    b.radius
                );
            }
        }
    }

    fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
        (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
    }

    fn demo_universe(filter: UniverseFilter) -> Universe {
        let mut store = Store::open_in_memory().unwrap();
        demo::load(&mut store, 1_800_000_000).unwrap();
        build(
            &UniverseInput {
                plays: &store.plays(None).unwrap(),
                affinities: &store.affinities().unwrap(),
                interests: &store.interests().unwrap(),
                galaxy_names: &HashMap::new(),
            },
            filter,
        )
    }

    /// Many Creators in several Galaxies, with a long tail of sizes.
    fn crowd(creators: i64) -> (Vec<Play>, InterestMap) {
        let plays: Vec<Play> = (0..creators)
            .map(|i| spotify(&format!("Creator {i:03}"), i * DAY, 100_000 / (i + 1) + 1))
            .collect();
        let tags: [&[&str]; 4] = [&["jazz"], &["metal"], &["kids music"], &["cooking"]];
        let names: Vec<String> = (0..creators).map(|i| format!("Creator {i:03}")).collect();
        let rows: Vec<(Source, &str, &[&str])> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (Source::Spotify, n.as_str(), tags[i % 4]))
            .collect();
        (plays, interests(&rows))
    }

    #[test]
    fn layout_is_deterministic() {
        let (plays, map) = crowd(150);
        let first = with_interests(&plays, &map, Spotify);
        let second = with_interests(&plays, &map, Spotify);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        assert!(!first.links.is_empty());
        assert_eq!(
            serde_json::to_string(&demo_universe(Both)).unwrap(),
            serde_json::to_string(&demo_universe(Both)).unwrap()
        );
    }

    #[test]
    fn no_nan_and_no_overlap() {
        let (plays, map) = crowd(150);
        let u = with_interests(&plays, &map, Spotify);
        assert_eq!(u.planets.len(), 150);
        assert_sound(&u);
    }

    #[test]
    fn the_hardest_crowds_still_do_not_overlap() {
        // Every Planet as big as it can be, all in one Galaxy.
        let names: Vec<String> = (0..150).map(|i| format!("Creator {i:03}")).collect();
        let plays: Vec<Play> = names
            .iter()
            .enumerate()
            .map(|(i, n)| spotify(n, i as i64 * DAY, 60))
            .collect();
        let rows: Vec<(Source, &str, &[&str])> = names
            .iter()
            .map(|n| (Source::Spotify, n.as_str(), &["jazz"] as &[&str]))
            .collect();
        let one_galaxy = with_interests(&plays, &interests(&rows), Spotify);
        assert_eq!(one_galaxy.galaxies.len(), 1);
        assert!(one_galaxy.planets.iter().all(|p| p.radius == 4.0));
        assert_sound(&one_galaxy);

        // No Links at all, so 150 Galaxies of one Planet each.
        let scattered = from_plays(&plays, Spotify);
        assert_eq!(scattered.galaxies.len(), 150);
        assert_sound(&scattered);
    }

    #[test]
    fn sparse_inputs() {
        for filter in [Spotify, Youtube, Both] {
            let empty = from_plays(&[], filter);
            assert!(
                empty.planets.is_empty() && empty.links.is_empty() && empty.galaxies.is_empty()
            );
        }

        let one = from_plays(&[spotify("Moth Season", 0, 3)], Both);
        assert_eq!(one.planets.len(), 1);
        assert_eq!(one.galaxies.len(), 1);
        assert_sound(&one);

        // One Source under Both, and a filter for the Source with nothing in it.
        let plays = [
            spotify("A", 0, 3),
            spotify("B", 60, 2),
            spotify("C", DAY, 1),
        ];
        let u = from_plays(&plays, Both);
        assert_eq!(u.planets.len(), 3);
        assert_sound(&u);
        assert!(from_plays(&plays, Youtube).planets.is_empty());

        // A single Session of one Creator, then only a deleted video.
        let u = from_plays(&[youtube("Solo", 0), youtube("Solo", 60)], Youtube);
        assert_eq!(u.planets.len(), 1);
        assert_sound(&u);
        assert!(from_plays(&[youtube("", 0)], Youtube).planets.is_empty());
    }

    #[test]
    fn the_same_name_on_two_sources_does_not_start_in_the_same_place() {
        let u = from_plays(&[spotify("Twin", 0, 3), youtube("Twin", 60)], Both);
        assert_sound(&u);
    }

    #[test]
    fn a_pair_in_one_spot_still_has_a_way_apart() {
        let (d, way) = super::gap([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]);
        assert_eq!(d, 0.0);
        assert_eq!(way, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn demo_universe_reads_as_galaxies() {
        // In Both, a group's artists and channels share 2 of their 4 tags, so a group may become one
        // Galaxy across Sources. No Galaxy holds more than one group.
        for (filter, planets, galaxies) in [
            (Spotify, 30, 5..=5),
            (Youtube, 30, 5..=5),
            (Both, 60, 5..=10),
        ] {
            let u = demo_universe(filter);
            assert_eq!(u.planets.len(), planets, "{filter:?}");
            assert!(
                galaxies.contains(&u.galaxies.len()),
                "{filter:?}: {}",
                u.galaxies.len()
            );
            for g in &u.galaxies {
                let size = u.planets.iter().filter(|p| p.galaxy == g.id).count();
                assert!(size <= 12, "{filter:?}: a Galaxy of {size}");
            }
            assert!(
                u.links.len() >= planets,
                "{filter:?}: {} Links",
                u.links.len()
            );
            assert_sound(&u);

            // Every Planet is nearer the middle of its own Galaxy than the middle of any other.
            let centre = |g: &Galaxy| {
                let members: Vec<_> = u.planets.iter().filter(|p| p.galaxy == g.id).collect();
                let mean = |axis: usize| {
                    members.iter().map(|p| p.position[axis]).sum::<f64>() / members.len() as f64
                };
                [mean(0), mean(1), mean(2)]
            };
            let centres: Vec<[f64; 3]> = u.galaxies.iter().map(centre).collect();
            for p in &u.planets {
                let own = distance(p.position, centres[p.galaxy as usize]);
                for (id, c) in centres.iter().enumerate() {
                    if id as u32 != p.galaxy {
                        let other = distance(p.position, *c);
                        assert!(own < other, "{filter:?}: {} drifts to Galaxy {id}", p.name);
                    }
                }
            }
        }
    }

    #[test]
    fn links_join_planets_that_exist() {
        let u = demo_universe(Both);
        for l in &u.links {
            assert!(l.a < l.b);
            assert!((l.b as usize) < u.planets.len());
            assert!((0.12..=1.0).contains(&l.strength));
        }
        let sizes: u32 = u.galaxies.iter().map(|g| g.size).sum();
        assert_eq!(sizes as usize, u.planets.len());
    }
}
