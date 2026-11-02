use crate::error::{Error, Result};
use crate::model::{Affinity, InterestMap, Origin, Play, Source, SourceTotals, Totals};
use crate::roast::{FinishedRoast, Roast, StoredRoast};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS plays (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    origin TEXT NOT NULL,
    played_at INTEGER NOT NULL,
    duration_ms INTEGER,
    creator TEXT NOT NULL,
    creator_ref TEXT,
    item TEXT NOT NULL,
    item_ref TEXT,
    skipped INTEGER NOT NULL DEFAULT 0,
    dedupe_key TEXT NOT NULL UNIQUE
);
CREATE INDEX IF NOT EXISTS plays_by_time ON plays (played_at);
CREATE TABLE IF NOT EXISTS affinities (
    source TEXT NOT NULL,
    creator TEXT NOT NULL,
    creator_ref TEXT,
    kind TEXT NOT NULL,
    score REAL NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (source, creator, kind)
);
CREATE TABLE IF NOT EXISTS interests (
    source TEXT NOT NULL,
    creator TEXT NOT NULL,
    interest TEXT NOT NULL,
    origin TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (source, creator, interest, origin)
);
CREATE TABLE IF NOT EXISTS roasts (
    id INTEGER PRIMARY KEY,
    created_at INTEGER NOT NULL,
    provider_label TEXT NOT NULL,
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS demo_roasts (
    roast_id INTEGER PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Affinities have no Origin of their own, so this `kind` is what marks a demo one.
pub const DEMO_KIND: &str = "demo";
/// The `origin` of an Interest a Provider wrote.
const AI_ORIGIN: &str = "ai";
/// An AI row with this for its Interest says a Provider answered and had no tags for that Creator, so
/// it is not asked again. In this table so that whatever clears a Creator's tags clears this too, and
/// `Store::interests` is the one thing that reads the table and leaves it out.
const NO_TAGS: &str = "";

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertCount {
    pub added: u64,
    pub duplicates: u64,
}

/// Minute precision lets Spotify's minutes-only and seconds formats collapse into one Play.
/// Takeout writes a watch's title, channel and wording when it exports, so a later Takeout can
/// repeat a watch in other words, and only its exact second says it is the same watch. Demo Plays
/// keep the first form, so made-up history cannot hide a real watch.
// ponytail: two watches begun in the same second count once. The video ID cannot tell them apart,
// because a later Takeout has no link for a video that has since been removed.
pub(crate) fn dedupe_key(p: &Play) -> String {
    let source = p.source.as_str();
    if p.source == Source::Youtube && p.origin == Origin::Export {
        return format!("{source}|{}", p.played_at);
    }
    format!(
        "{source}|{}|{}|{}",
        p.played_at.div_euclid(60),
        p.creator,
        p.item
    )
}

/// A stored value this build does not know is an error, never a guess.
fn unknown(column: usize, what: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        format!("unknown {what} {value:?}").into(),
    )
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Store::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        // journal_mode returns a row, so read it instead of executing.
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// One transaction. A Play that is already stored (same dedupe key) counts as a duplicate,
    /// except that an Export Play takes over an API Play it repeats, so an Export import
    /// never loses the Play to the API copy it is about to replace. A duplicate with an
    /// `item_ref` also fills in an Export copy that has none, which is how an extended row
    /// improves on the account row of the same Play.
    pub fn insert_plays(&mut self, plays: &[Play]) -> Result<InsertCount> {
        let tx = self.conn.transaction()?;
        let mut added = 0;
        {
            let mut fill_in = tx.prepare(
                "UPDATE plays SET played_at = ?1, duration_ms = ?2, item_ref = ?3, skipped = ?4
                 WHERE dedupe_key = ?5 AND origin = 'export' AND item_ref IS NULL",
            )?;
            let mut stmt = tx.prepare(
                "INSERT INTO plays
                 (source, origin, played_at, duration_ms, creator, creator_ref,
                  item, item_ref, skipped, dedupe_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (dedupe_key) DO UPDATE SET
                    origin = excluded.origin, played_at = excluded.played_at,
                    duration_ms = excluded.duration_ms, creator_ref = excluded.creator_ref,
                    item_ref = excluded.item_ref, skipped = excluded.skipped
                 WHERE plays.origin = 'api' AND excluded.origin = 'export'",
            )?;
            for p in plays {
                let key = dedupe_key(p);
                let stored = stmt.execute(params![
                    p.source.as_str(),
                    p.origin.as_str(),
                    p.played_at,
                    p.duration_ms,
                    p.creator,
                    p.creator_ref,
                    p.item,
                    p.item_ref,
                    p.skipped,
                    key,
                ])?;
                added += stored as u64;
                if stored == 0 && p.origin == Origin::Export && p.item_ref.is_some() {
                    fill_in.execute(params![
                        p.played_at,
                        p.duration_ms,
                        p.item_ref,
                        p.skipped,
                        key
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(InsertCount {
            added,
            duplicates: plays.len() as u64 - added,
        })
    }

    pub fn totals(&self) -> Result<Totals> {
        Ok(Totals {
            spotify: self.source_totals(Source::Spotify)?,
            youtube: self.source_totals(Source::Youtube)?,
        })
    }

    fn source_totals(&self, source: Source) -> Result<SourceTotals> {
        // An empty Creator (a deleted YouTube video) is a Play but never a Creator.
        Ok(self.conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(duration_ms), 0) / 60000,
                    COUNT(DISTINCT CASE WHEN creator <> '' THEN creator END),
                    MIN(played_at), MAX(played_at)
             FROM plays WHERE source = ?1",
            [source.as_str()],
            |r| {
                Ok(SourceTotals {
                    plays: r.get::<_, i64>(0)? as u64,
                    minutes: r.get::<_, i64>(1)? as u64,
                    creators: r.get::<_, i64>(2)? as u64,
                    first_play: r.get(3)?,
                    last_play: r.get(4)?,
                })
            },
        )?)
    }

    /// Ordered by `played_at`, ties by insertion order.
    pub fn plays(&self, source: Option<Source>) -> Result<Vec<Play>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, origin, played_at, duration_ms, creator, creator_ref,
                    item, item_ref, skipped
             FROM plays WHERE ?1 IS NULL OR source = ?1
             ORDER BY played_at, id",
        )?;
        let rows = stmt.query_map([source.map(Source::as_str)], |r| {
            let (source, origin): (String, String) = (r.get(0)?, r.get(1)?);
            Ok(Play {
                source: Source::from_db(&source).ok_or_else(|| unknown(0, "source", &source))?,
                origin: Origin::from_db(&origin).ok_or_else(|| unknown(1, "origin", &origin))?,
                played_at: r.get(2)?,
                duration_ms: r.get(3)?,
                creator: r.get(4)?,
                creator_ref: r.get(5)?,
                item: r.get(6)?,
                item_ref: r.get(7)?,
                skipped: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// After an Export import, once per file: API Plays from the file's oldest Play to its newest,
    /// inclusive, are in the Export now. Outside that span an API Play may be held nowhere else, so
    /// it stays.
    pub fn replace_api_plays_in_span(
        &mut self,
        source: Source,
        oldest: i64,
        newest: i64,
    ) -> Result<u64> {
        Ok(self.conn.execute(
            "DELETE FROM plays
             WHERE source = ?1 AND origin = 'api' AND played_at BETWEEN ?2 AND ?3",
            params![source.as_str(), oldest, newest],
        )? as u64)
    }

    /// One transaction. Returns how many of these Plays were stored.
    pub(crate) fn delete_by_dedupe_key(&mut self, keys: &[String]) -> Result<u64> {
        let tx = self.conn.transaction()?;
        let mut removed = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM plays WHERE dedupe_key = ?1")?;
            for key in keys {
                removed += stmt.execute([key])? as u64;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn has_demo_data(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM plays WHERE origin = 'demo')",
            [],
            |r| r.get(0),
        )?)
    }

    /// Any Play with an Origin other than demo, whether from an Export or a Connection.
    pub fn has_real_plays(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM plays WHERE origin <> 'demo')",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn delete_origin(&mut self, source: Source, origin: Origin) -> Result<u64> {
        Ok(self.conn.execute(
            "DELETE FROM plays WHERE source = ?1 AND origin = ?2",
            [source.as_str(), origin.as_str()],
        )? as u64)
    }

    /// One transaction, so demo data never half leaves. That includes any Roast written from it, which
    /// is jokes about made-up names, and a Share Site could otherwise publish it as this person's.
    /// Returns how many Plays went.
    pub fn delete_demo(&mut self) -> Result<u64> {
        let tx = self.conn.transaction()?;
        // First, while the demo is there to say who it was about. A real Creator with a demo name would
        // lose a tag here, which only means being asked about once more.
        tx.execute(
            &format!(
                "DELETE FROM interests WHERE origin = '{AI_ORIGIN}' AND (source, creator) IN (
                    SELECT source, creator FROM plays WHERE origin = 'demo'
                    UNION SELECT source, creator FROM affinities WHERE kind = ?1)"
            ),
            [DEMO_KIND],
        )?;
        let plays = tx.execute("DELETE FROM plays WHERE origin = 'demo'", [])?;
        tx.execute("DELETE FROM interests WHERE origin = 'demo'", [])?;
        tx.execute("DELETE FROM affinities WHERE kind = ?1", [DEMO_KIND])?;
        tx.execute(
            "DELETE FROM roasts WHERE id IN (SELECT roast_id FROM demo_roasts)",
            [],
        )?;
        tx.execute("DELETE FROM demo_roasts", [])?;
        tx.commit()?;
        Ok(plays as u64)
    }

    pub fn affinities(&self) -> Result<Vec<Affinity>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, creator, creator_ref, kind, score, fetched_at
             FROM affinities ORDER BY source, kind, creator",
        )?;
        let rows = stmt.query_map([], |r| {
            let source: String = r.get(0)?;
            Ok(Affinity {
                source: Source::from_db(&source).ok_or_else(|| unknown(0, "source", &source))?,
                creator: r.get(1)?,
                creator_ref: r.get(2)?,
                kind: r.get(3)?,
                score: r.get(4)?,
                fetched_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// One transaction. What a sync says about one Source and kind replaces what it said before,
    /// so a Creator who fell off the list goes. Each row's own `source` and `kind` are not read.
    pub fn set_affinities(
        &mut self,
        source: Source,
        kind: &str,
        affinities: &[Affinity],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM affinities WHERE source = ?1 AND kind = ?2",
            [source.as_str(), kind],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO affinities
                 (source, creator, creator_ref, kind, score, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for a in affinities {
                stmt.execute(params![
                    source.as_str(),
                    a.creator,
                    a.creator_ref,
                    kind,
                    a.score,
                    a.fetched_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Every origin's Interests together, since a tag means the same wherever it came from.
    pub fn interests(&self) -> Result<InterestMap> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT source, creator, interest FROM interests WHERE interest <> '{NO_TAGS}'
             ORDER BY interest"
        ))?;
        let mut rows = stmt.query([])?;
        let mut all = InterestMap::new();
        while let Some(r) = rows.next()? {
            let source: String = r.get(0)?;
            let source = Source::from_db(&source).ok_or_else(|| unknown(0, "source", &source))?;
            all.entry((source, r.get(1)?)).or_default().push(r.get(2)?);
        }
        Ok(all)
    }

    /// One transaction. Replaces what this origin says about this Creator, and leaves what other
    /// origins say.
    pub fn set_interests(
        &mut self,
        source: Source,
        creator: &str,
        interests: &[String],
        origin: &str,
        fetched_at: i64,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM interests WHERE source = ?1 AND creator = ?2 AND origin = ?3",
            [source.as_str(), creator, origin],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO interests (source, creator, interest, origin, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for interest in interests {
                stmt.execute(params![
                    source.as_str(),
                    creator,
                    interest,
                    origin,
                    fetched_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Who a Provider has answered about, tags or not.
    pub fn ai_asked(&self) -> Result<HashSet<(Source, String)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT source, creator FROM interests WHERE origin = '{AI_ORIGIN}'"
        ))?;
        let rows = stmt.query_map([], |r| {
            let source: String = r.get(0)?;
            Ok((
                Source::from_db(&source).ok_or_else(|| unknown(0, "source", &source))?,
                r.get(1)?,
            ))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// What a Provider said about each Creator it was asked about, where saying nothing still counts
    /// as asked. One transaction, so a reply lands whole, and only for a Creator still stored: the
    /// Provider takes a while, and an AI tag for a made-up artist must not outlive the demo it came with.
    pub fn keep_ai_interests(
        &mut self,
        rows: &[(Source, &str, Vec<String>)],
        fetched_at: i64,
    ) -> Result<()> {
        // Immediate, because a transaction that reads first cannot wait for the write lock later.
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut stored = tx.prepare(
                "SELECT source, creator FROM plays UNION SELECT source, creator FROM affinities",
            )?;
            let stored: HashSet<(String, String)> = stored
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            let mut clear = tx.prepare(&format!(
                "DELETE FROM interests WHERE source = ?1 AND creator = ?2 AND origin = '{AI_ORIGIN}'"
            ))?;
            let mut keep = tx.prepare(&format!(
                "INSERT OR REPLACE INTO interests (source, creator, interest, origin, fetched_at)
                 VALUES (?1, ?2, ?3, '{AI_ORIGIN}', ?4)"
            ))?;
            let asked_only = [NO_TAGS.to_owned()];
            for (source, creator, tags) in rows {
                let source = source.as_str();
                if !stored.contains(&(source.to_owned(), (*creator).to_owned())) {
                    continue;
                }
                clear.execute([source, creator])?;
                let tags = if tags.is_empty() {
                    &asked_only
                } else {
                    &tags[..]
                };
                for tag in tags {
                    keep.execute(params![source, creator, tag, fetched_at])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The names the last Roast gave, by the id of a Galaxy in the Both view. A Roast that cannot be read
    /// names nothing, because the Roast screen is the place to say so and a Universe is still worth drawing.
    pub fn latest_galaxy_names(&self) -> Result<HashMap<u32, String>> {
        match self.latest_roast() {
            Ok(latest) => Ok(latest
                .into_iter()
                .flat_map(|stored| stored.roast.galaxy_names)
                .map(|galaxy| (galaxy.id, galaxy.name))
                .collect()),
            Err(Error::UnreadableRoast) => Ok(HashMap::new()),
            Err(e) => Err(e),
        }
    }

    /// Saves a finished Roast, waiting for up to `wait` on a busy database in place of the usual 5
    /// seconds. Whatever happens there is a Roast to read, with a sentence when it was not kept.
    pub fn keep_roast(
        &self,
        provider_label: &str,
        roast: Roast,
        created_at: i64,
        from_demo: bool,
        wait: Duration,
    ) -> FinishedRoast {
        let saved = self
            .conn
            .busy_timeout(wait)
            .map_err(Error::from)
            .and_then(|()| self.save_roast(provider_label, &roast, created_at, from_demo));
        match saved {
            Ok(saved) => FinishedRoast {
                roast: saved,
                not_saved: None,
            },
            Err(e) => FinishedRoast {
                roast: StoredRoast {
                    id: 0,
                    created_at,
                    provider_label: provider_label.to_owned(),
                    roast,
                },
                not_saved: Some(format!(
                    "This Roast was not saved, so it will be gone when you close Rostery. {e}"
                )),
            },
        }
    }

    /// Only a finished Roast gets here, so a failed run can never replace the one before it.
    /// `from_demo` says demo Plays were stored when its Digest was built, so it leaves with them. That
    /// is in a table of its own because `CREATE TABLE IF NOT EXISTS` is all an older store needs, while
    /// an `ALTER TABLE` could run twice when two commands open that store at the same moment.
    pub fn save_roast(
        &self,
        provider_label: &str,
        roast: &Roast,
        created_at: i64,
        from_demo: bool,
    ) -> Result<StoredRoast> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO roasts (created_at, provider_label, json) VALUES (?1, ?2, ?3)",
            params![created_at, provider_label, serde_json::to_string(roast)?],
        )?;
        let id = tx.last_insert_rowid();
        if from_demo {
            tx.execute("INSERT INTO demo_roasts (roast_id) VALUES (?1)", [id])?;
        }
        tx.commit()?;
        Ok(StoredRoast {
            id,
            created_at,
            provider_label: provider_label.to_owned(),
            roast: roast.clone(),
        })
    }

    pub fn latest_roast(&self) -> Result<Option<StoredRoast>> {
        let row: Option<(i64, i64, String, String)> = self
            .conn
            .query_row(
                "SELECT id, created_at, provider_label, json FROM roasts ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        row.map(|(id, created_at, provider_label, json)| {
            Ok(StoredRoast {
                id,
                created_at,
                provider_label,
                roast: serde_json::from_str(&json).map_err(|_| Error::UnreadableRoast)?,
            })
        })
        .transpose()
    }

    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(source: Source, creator: &str, item: &str, played_at: i64) -> Play {
        Play {
            source,
            origin: Origin::Demo,
            played_at,
            duration_ms: None,
            creator: creator.into(),
            creator_ref: None,
            item: item.into(),
            item_ref: None,
            skipped: false,
        }
    }

    #[test]
    fn insert_is_idempotent() {
        let mut store = Store::open_in_memory().unwrap();
        let plays = [
            play(Source::Spotify, "The Paper Lanterns", "Low Tide", 1000),
            play(Source::Spotify, "The Paper Lanterns", "Soft Static", 2000),
            play(Source::Youtube, "Slow Math Club", "Why Zero Matters", 3000),
        ];
        let first = store.insert_plays(&plays).unwrap();
        assert_eq!((first.added, first.duplicates), (3, 0));
        let second = store.insert_plays(&plays).unwrap();
        assert_eq!((second.added, second.duplicates), (0, 3));
        assert_eq!(store.plays(None).unwrap().len(), 3);
    }

    #[test]
    fn same_minute_counts_once() {
        let mut store = Store::open_in_memory().unwrap();
        let at = |t| play(Source::Spotify, "Vanta Kids", "Sleepy Sprocket", t);
        // 1000 and 1019 share minute 16; 1020 is minute 17.
        let first = store.insert_plays(&[at(1000), at(1019)]).unwrap();
        assert_eq!((first.added, first.duplicates), (1, 1));
        let second = store.insert_plays(&[at(1020)]).unwrap();
        assert_eq!(second.added, 1);
        assert_eq!(store.plays(None).unwrap().len(), 2);
    }

    #[test]
    fn a_youtube_export_play_is_known_by_its_second() {
        let mut store = Store::open_in_memory().unwrap();
        let watch = |creator, item, at| Play {
            origin: Origin::Export,
            ..play(Source::Youtube, creator, item, at)
        };
        let first = store
            .insert_plays(&[watch("Old Name", "Old Title", 1000)])
            .unwrap();
        assert_eq!((first.added, first.duplicates), (1, 0));
        let reworded = store
            .insert_plays(&[watch("New Name", "New Title", 1000)])
            .unwrap();
        assert_eq!((reworded.added, reworded.duplicates), (0, 1));
        let next_second = store
            .insert_plays(&[watch("Old Name", "Old Title", 1001)])
            .unwrap();
        assert_eq!(next_second.added, 1);
    }

    #[test]
    fn demo_data_never_hides_a_watch_at_the_same_second() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[play(Source::Youtube, "Cache Miss", "Tiny Compilers", 1000)])
            .unwrap();
        let watch = Play {
            origin: Origin::Export,
            ..play(Source::Youtube, "Real Channel", "Real Video", 1000)
        };
        assert_eq!(store.insert_plays(&[watch]).unwrap().added, 1);
        assert_eq!(store.plays(None).unwrap().len(), 2);
    }

    #[test]
    fn totals_are_per_source() {
        let mut store = Store::open_in_memory().unwrap();
        let mut a = play(Source::Spotify, "The Paper Lanterns", "Low Tide", 600);
        a.duration_ms = Some(180_000);
        let mut b = play(Source::Spotify, "The Paper Lanterns", "Soft Static", 1200);
        b.duration_ms = Some(240_000);
        let c = play(Source::Youtube, "Slow Math Club", "Why Zero Matters", 900);
        let d = play(Source::Youtube, "Cache Miss", "Tiny Compilers", 1800);
        store.insert_plays(&[a, b, c, d]).unwrap();

        let totals = store.totals().unwrap();
        // The Creator appears twice in Spotify but counts once.
        assert_eq!(totals.spotify.plays, 2);
        assert_eq!(totals.spotify.creators, 1);
        assert_eq!(totals.spotify.minutes, 7);
        assert_eq!(totals.spotify.first_play, Some(600));
        assert_eq!(totals.spotify.last_play, Some(1200));
        assert_eq!(totals.youtube.plays, 2);
        assert_eq!(totals.youtube.creators, 2);
        assert_eq!(totals.youtube.minutes, 0);
        assert_eq!(
            Store::open_in_memory().unwrap().totals().unwrap(),
            Totals::default()
        );
    }

    #[test]
    fn plays_are_ordered() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .insert_plays(&[
                play(Source::Spotify, "A", "late", 9000),
                play(Source::Youtube, "B", "early", 60),
                play(Source::Spotify, "C", "middle", 4000),
            ])
            .unwrap();
        let times: Vec<i64> = store
            .plays(None)
            .unwrap()
            .iter()
            .map(|p| p.played_at)
            .collect();
        assert_eq!(times, [60, 4000, 9000]);
        let spotify = store.plays(Some(Source::Spotify)).unwrap();
        assert_eq!(spotify.len(), 2);
        assert!(spotify.iter().all(|p| p.source == Source::Spotify));
    }

    #[test]
    fn delete_origin_keeps_other_origins() {
        let mut store = Store::open_in_memory().unwrap();
        let mut real = play(Source::Spotify, "Real Band", "Real Song", 100_000);
        real.origin = Origin::Export;
        let demo = play(Source::Spotify, "The Paper Lanterns", "Low Tide", 200_000);
        store.insert_plays(&[real, demo]).unwrap();
        assert_eq!(
            store.delete_origin(Source::Spotify, Origin::Demo).unwrap(),
            1
        );
        let left = store.plays(None).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].origin, Origin::Export);
    }

    #[test]
    fn has_real_plays_is_any_origin_but_demo() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(!store.has_real_plays().unwrap());
        store
            .insert_plays(&[play(
                Source::Spotify,
                "The Paper Lanterns",
                "Low Tide",
                1000,
            )])
            .unwrap();
        assert!(!store.has_real_plays().unwrap());
        for origin in [Origin::Export, Origin::Api] {
            let mut store = Store::open_in_memory().unwrap();
            let mut real = play(Source::Spotify, "Real Band", "Real Song", 2000);
            real.origin = origin;
            store.insert_plays(&[real]).unwrap();
            assert!(store.has_real_plays().unwrap(), "{origin:?}");
        }
    }

    #[test]
    fn has_demo_data_ignores_other_origins() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(!store.has_demo_data().unwrap());
        let mut real = play(Source::Spotify, "Real Band", "Real Song", 100_000);
        real.origin = Origin::Export;
        store.insert_plays(&[real]).unwrap();
        assert!(!store.has_demo_data().unwrap());
        store
            .insert_plays(&[play(
                Source::Youtube,
                "Slow Math Club",
                "Why Zero Matters",
                200_000,
            )])
            .unwrap();
        assert!(store.has_demo_data().unwrap());
        store.delete_origin(Source::Youtube, Origin::Demo).unwrap();
        assert!(!store.has_demo_data().unwrap());
    }

    #[test]
    fn replace_api_plays_in_span_is_inclusive_and_scoped() {
        let mut store = Store::open_in_memory().unwrap();
        let with = |source, origin, creator: &str, at| Play {
            origin,
            ..play(source, creator, "Song", at)
        };
        store
            .insert_plays(&[
                with(Source::Spotify, Origin::Api, "before", 100),
                with(Source::Spotify, Origin::Api, "at oldest", 200),
                with(Source::Spotify, Origin::Api, "inside", 250),
                with(Source::Spotify, Origin::Api, "at newest", 300),
                with(Source::Spotify, Origin::Api, "after", 400),
                with(Source::Spotify, Origin::Export, "export", 250),
                with(Source::Youtube, Origin::Api, "other source", 250),
            ])
            .unwrap();
        assert_eq!(
            store
                .replace_api_plays_in_span(Source::Spotify, 200, 300)
                .unwrap(),
            3
        );
        let mut left: Vec<String> = store
            .plays(None)
            .unwrap()
            .into_iter()
            .map(|p| p.creator)
            .collect();
        left.sort();
        assert_eq!(left, ["after", "before", "export", "other source"]);
    }

    #[test]
    fn export_play_takes_over_the_api_play_it_repeats() {
        let mut store = Store::open_in_memory().unwrap();
        let with = |origin, at| Play {
            origin,
            ..play(Source::Spotify, "Moth Season", "Soft Static", at)
        };
        store.insert_plays(&[with(Origin::Api, 1000)]).unwrap();
        let count = store.insert_plays(&[with(Origin::Export, 1010)]).unwrap();
        assert_eq!((count.added, count.duplicates), (1, 0));
        // A second Export copy is just a duplicate.
        let again = store.insert_plays(&[with(Origin::Export, 1015)]).unwrap();
        assert_eq!((again.added, again.duplicates), (0, 1));
        let plays = store.plays(None).unwrap();
        assert_eq!(plays.len(), 1);
        assert_eq!(
            (plays[0].origin, plays[0].played_at),
            (Origin::Export, 1010)
        );
    }

    #[test]
    fn unknown_source_or_origin_is_an_error() {
        for (source, origin) in [("tidal", "export"), ("spotify", "scrape")] {
            let store = Store::open_in_memory().unwrap();
            store
                .conn
                .execute(
                    "INSERT INTO plays (source, origin, played_at, creator, item, dedupe_key)
                     VALUES (?1, ?2, 1, 'A', 'T', 'k')",
                    [source, origin],
                )
                .unwrap();
            assert!(matches!(
                store.plays(None),
                Err(Error::Db(rusqlite::Error::FromSqlConversionFailure(..)))
            ));
        }
    }

    fn affinity(source: Source, creator: &str, kind: &str, score: f64) -> Affinity {
        Affinity {
            source,
            creator: creator.into(),
            creator_ref: Some("spotify:artist:abc".into()),
            kind: kind.into(),
            score,
            fetched_at: 5,
        }
    }

    #[test]
    fn set_affinities_replaces_one_source_and_kind() {
        let mut store = Store::open_in_memory().unwrap();
        let top = affinity(Source::Spotify, "Moth Season", "top_artist", 500.0);
        let gone = affinity(Source::Spotify, "Grim Parsley", "top_artist", 490.0);
        let other_kind = affinity(Source::Youtube, "Cache Miss", "liked", 15.0);
        let other_source = affinity(Source::Youtube, "Cache Miss", "top_artist", 60.0);
        store
            .set_affinities(Source::Spotify, "top_artist", &[top.clone(), gone])
            .unwrap();
        store
            .set_affinities(Source::Youtube, "liked", std::slice::from_ref(&other_kind))
            .unwrap();
        store
            .set_affinities(
                Source::Youtube,
                "top_artist",
                std::slice::from_ref(&other_source),
            )
            .unwrap();

        // The Creator who fell off the list goes, and every other list stays.
        store
            .set_affinities(Source::Spotify, "top_artist", std::slice::from_ref(&top))
            .unwrap();
        assert_eq!(
            store.affinities().unwrap(),
            [top, other_kind.clone(), other_source]
        );

        store
            .set_affinities(Source::Youtube, "top_artist", &[])
            .unwrap();
        store
            .set_affinities(Source::Spotify, "top_artist", &[])
            .unwrap();
        assert_eq!(store.affinities().unwrap(), [other_kind]);
    }

    #[test]
    fn set_interests_replaces_one_origin_and_interests_merges_them() {
        let mut store = Store::open_in_memory().unwrap();
        let tags = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let key = (Source::Spotify, "Moth Season".to_string());
        store
            .set_interests(
                Source::Spotify,
                "Moth Season",
                &tags(&["shoegaze", "dream pop"]),
                "ai",
                1,
            )
            .unwrap();
        store
            .set_interests(
                Source::Spotify,
                "Moth Season",
                &tags(&["shoegaze", "indie"]),
                "spotify_api",
                1,
            )
            .unwrap();
        // The same Creator on the other Source is another Creator.
        store
            .set_interests(Source::Youtube, "Moth Season", &tags(&["cooking"]), "ai", 1)
            .unwrap();

        let all = store.interests().unwrap();
        assert_eq!(all[&key], ["dream pop", "indie", "shoegaze"]);
        assert_eq!(
            all[&(Source::Youtube, "Moth Season".to_string())],
            ["cooking"]
        );

        store
            .set_interests(Source::Spotify, "Moth Season", &tags(&["noise"]), "ai", 2)
            .unwrap();
        assert_eq!(
            store.interests().unwrap()[&key],
            ["indie", "noise", "shoegaze"]
        );

        store
            .set_interests(Source::Spotify, "Moth Season", &[], "ai", 3)
            .unwrap();
        assert_eq!(store.interests().unwrap()[&key], ["indie", "shoegaze"]);
    }

    #[test]
    fn a_repeated_interest_is_kept_once() {
        let mut store = Store::open_in_memory().unwrap();
        let tags = ["lo-fi".to_string(), "lo-fi".to_string()];
        store
            .set_interests(Source::Youtube, "Lighthouse Lofi", &tags, "ai", 1)
            .unwrap();
        assert_eq!(
            store.interests().unwrap()[&(Source::Youtube, "Lighthouse Lofi".to_string())],
            ["lo-fi"]
        );
    }

    #[test]
    fn an_unknown_source_in_affinities_or_interests_is_an_error() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute_batch(
                "INSERT INTO affinities VALUES ('tidal', 'A', NULL, 'top_artist', 1.0, 1);
                 INSERT INTO interests VALUES ('tidal', 'A', 'jazz', 'ai', 1);",
            )
            .unwrap();
        assert!(matches!(store.affinities(), Err(Error::Db(_))));
        assert!(matches!(store.interests(), Err(Error::Db(_))));
    }

    fn a_roast(headline: &str) -> Roast {
        let burn = crate::roast::Burn {
            title: "Burn".into(),
            body: "Ouch.".into(),
        };
        Roast {
            headline: headline.into(),
            opening: "You.".into(),
            burns: vec![burn],
            verdict: crate::roast::Verdict {
                archetype: "Night Clerk".into(),
                score: 40,
                sentence: "Sit down.".into(),
                evidence: Vec::new(),
            },
            galaxy_names: Vec::new(),
        }
    }

    #[test]
    fn delete_demo_takes_a_roast_of_the_demo_and_leaves_a_roast_of_real_history() {
        let mut store = Store::open_in_memory().unwrap();
        let real = store
            .save_roast("Claude Code", &a_roast("Real"), 1000, false)
            .unwrap();
        store
            .save_roast("Claude Code", &a_roast("Made Up"), 2000, true)
            .unwrap();
        assert_eq!(
            store.latest_roast().unwrap().unwrap().roast.headline,
            "Made Up"
        );

        store.delete_demo().unwrap();
        assert_eq!(store.latest_roast().unwrap(), Some(real.clone()));

        // A Roast saved later is not marked by what came before it, even if it gets the old id.
        let next = store
            .save_roast("Claude Code", &a_roast("Next"), 3000, false)
            .unwrap();
        store.delete_demo().unwrap();
        assert_eq!(store.latest_roast().unwrap(), Some(next));
    }

    #[test]
    fn a_stored_roast_that_cannot_be_read_says_so_and_the_next_one_replaces_it() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO roasts (created_at, provider_label, json)
                 VALUES (1, 'Claude Code', '{\"from\": \"another version\"}')",
                [],
            )
            .unwrap();

        let message = store.latest_roast().unwrap_err().to_string();
        assert!(message.contains("your last Roast"), "{message}");
        assert!(!message.contains("setting"), "{message}");

        let next = store
            .save_roast("Claude Code", &a_roast("Next"), 2000, false)
            .unwrap();
        assert_eq!(store.latest_roast().unwrap(), Some(next));
    }

    #[test]
    fn a_store_from_before_demo_roasts_were_marked_still_works() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE roasts (
                id INTEGER PRIMARY KEY,
                created_at INTEGER NOT NULL,
                provider_label TEXT NOT NULL,
                json TEXT NOT NULL
             );",
        )
        .unwrap();
        let mut store = Store::init(conn).unwrap();
        store
            .save_roast("Claude Code", &a_roast("Made Up"), 1000, true)
            .unwrap();
        store.delete_demo().unwrap();
        assert_eq!(store.latest_roast().unwrap(), None);
    }

    /// Another connection holds the one write lock SQLite has, as an import does.
    #[test]
    fn a_finished_roast_outlasts_a_busy_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rostery.db");
        let store = Store::open(&path).unwrap();
        let other = Store::open(&path).unwrap();
        other.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let releasing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            other.conn.execute_batch("COMMIT").unwrap();
        });
        let roast = a_roast("Waited For");

        let finished =
            store.keep_roast("Codex", roast.clone(), 1000, false, Duration::from_secs(20));
        releasing.join().unwrap();

        assert_eq!(finished.not_saved, None);
        assert!(finished.roast.id > 0);
        assert_eq!(store.latest_roast().unwrap().unwrap().roast, roast);
    }

    #[test]
    fn a_roast_that_cannot_be_saved_is_still_there_to_read_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rostery.db");
        let store = Store::open(&path).unwrap();
        let other = Store::open(&path).unwrap();
        other.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let roast = a_roast("Never Kept");

        let started = std::time::Instant::now();
        let finished = store.keep_roast(
            "Codex",
            roast.clone(),
            1000,
            false,
            Duration::from_millis(50),
        );

        // Well under the usual 5 seconds, so it is this wait that counts, however long it is told to be.
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(finished.roast.roast, roast);
        assert_eq!(finished.roast.provider_label, "Codex");
        assert_eq!(finished.roast.created_at, 1000);
        let note = finished.not_saved.unwrap();
        assert!(note.contains("not saved"), "{note}");
        assert!(note.contains("Close any other copy"), "{note}");
        assert_eq!(store.latest_roast().unwrap(), None);
    }

    fn named_roast(names: &[(u32, &str)]) -> Roast {
        Roast {
            galaxy_names: names
                .iter()
                .map(|(id, name)| crate::roast::GalaxyName {
                    id: *id,
                    name: (*name).into(),
                })
                .collect(),
            ..a_roast("Named")
        }
    }

    #[test]
    fn the_galaxy_names_are_the_last_roasts() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.latest_galaxy_names().unwrap().is_empty());
        store
            .save_roast("Claude Code", &named_roast(&[(0, "Old")]), 1000, false)
            .unwrap();
        store
            .save_roast("Claude Code", &named_roast(&[(1, "New")]), 2000, false)
            .unwrap();
        assert_eq!(
            store.latest_galaxy_names().unwrap(),
            HashMap::from([(1, "New".to_owned())])
        );
    }

    /// A Universe is worth drawing whatever has become of the last Roast.
    #[test]
    fn a_last_roast_that_cannot_be_read_names_nothing_and_is_no_error() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO roasts (created_at, provider_label, json)
                 VALUES (1, 'Claude Code', '{\"from\": \"another version\"}')",
                [],
            )
            .unwrap();
        assert!(store.latest_galaxy_names().unwrap().is_empty());
    }

    #[test]
    fn asked_about_with_no_tags_is_remembered_and_is_not_an_interest() {
        let mut store = Store::open_in_memory().unwrap();
        let (tagged, empty) = (spotify_creator("Tagged"), spotify_creator("Asked About"));
        store.insert_plays(&[tagged, empty]).unwrap();

        store
            .keep_ai_interests(
                &[
                    (Source::Spotify, "Tagged", vec!["jazz".to_owned()]),
                    (Source::Spotify, "Asked About", Vec::new()),
                ],
                5,
            )
            .unwrap();

        let asked = store.ai_asked().unwrap();
        assert!(asked.contains(&(Source::Spotify, "Tagged".to_owned())));
        assert!(asked.contains(&(Source::Spotify, "Asked About".to_owned())));
        let all = store.interests().unwrap();
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[&(Source::Spotify, "Tagged".to_owned())], ["jazz"]);
    }

    #[test]
    fn an_interest_from_elsewhere_does_not_count_as_asked() {
        let store = Store::open_in_memory().unwrap();
        let mut store = store;
        store
            .set_interests(Source::Spotify, "A", &["jazz".into()], "spotify_api", 1)
            .unwrap();
        store
            .set_interests(Source::Spotify, "B", &["jazz".into()], "demo", 1)
            .unwrap();
        assert!(store.ai_asked().unwrap().is_empty());
    }

    /// The Provider takes a while, and a Creator can leave while it does.
    #[test]
    fn a_tag_for_a_creator_no_longer_stored_is_not_kept_and_an_affinity_counts_as_stored() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .set_affinities(
                Source::Spotify,
                "top_artist",
                &[affinity(Source::Spotify, "Only Liked", "top_artist", 5.0)],
            )
            .unwrap();
        store
            .keep_ai_interests(
                &[
                    (Source::Spotify, "Gone", vec!["jazz".to_owned()]),
                    (Source::Youtube, "Gone Too", Vec::new()),
                    (Source::Spotify, "Only Liked", vec!["chess".to_owned()]),
                ],
                5,
            )
            .unwrap();
        let all = store.interests().unwrap();
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[&(Source::Spotify, "Only Liked".to_owned())], ["chess"]);
        assert_eq!(store.ai_asked().unwrap().len(), 1);
    }

    #[test]
    fn delete_demo_takes_ai_tags_on_demo_creators_and_leaves_the_rest() {
        let mut store = Store::open_in_memory().unwrap();
        let real = Play {
            origin: Origin::Export,
            ..play(Source::Spotify, "Real Band", "Real Song", 100_000)
        };
        let demo = play(Source::Spotify, "Moth Season", "Low Tide", 200_000);
        store.insert_plays(&[real, demo]).unwrap();
        store
            .set_affinities(
                Source::Youtube,
                DEMO_KIND,
                &[affinity(Source::Youtube, "Demo Liked", DEMO_KIND, 5.0)],
            )
            .unwrap();
        store
            .keep_ai_interests(
                &[
                    (Source::Spotify, "Real Band", vec!["jazz".to_owned()]),
                    (Source::Spotify, "Moth Season", vec!["shoegaze".to_owned()]),
                    (Source::Youtube, "Demo Liked", Vec::new()),
                ],
                5,
            )
            .unwrap();

        store.delete_demo().unwrap();

        let asked = store.ai_asked().unwrap();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked.contains(&(Source::Spotify, "Real Band".to_owned())));
        let all = store.interests().unwrap();
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[&(Source::Spotify, "Real Band".to_owned())], ["jazz"]);
    }

    /// What a Play by this Creator looks like when only its name matters.
    fn spotify_creator(creator: &str) -> Play {
        play(Source::Spotify, creator, "Song", 1000)
    }

    #[test]
    fn settings_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting::<i64>("missing").unwrap(), None);
        store.set_setting("n", &7_i64).unwrap();
        store.set_setting("n", &8_i64).unwrap();
        assert_eq!(store.get_setting::<i64>("n").unwrap(), Some(8));
    }
}
