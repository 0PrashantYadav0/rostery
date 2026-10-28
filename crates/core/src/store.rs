use crate::error::Result;
use crate::model::{Origin, Play, Source, SourceTotals, Totals};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

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
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertCount {
    pub added: u64,
    pub duplicates: u64,
}

/// Minute precision lets Spotify's minutes-only and seconds formats collapse into one Play.
fn dedupe_key(p: &Play) -> String {
    format!(
        "{}|{}|{}|{}",
        p.source.as_str(),
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
    /// never loses the Play to the API copy it is about to replace.
    pub fn insert_plays(&mut self, plays: &[Play]) -> Result<InsertCount> {
        let tx = self.conn.transaction()?;
        let mut added = 0;
        {
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
                added += stmt.execute(params![
                    p.source.as_str(),
                    p.origin.as_str(),
                    p.played_at,
                    p.duration_ms,
                    p.creator,
                    p.creator_ref,
                    p.item,
                    p.item_ref,
                    p.skipped,
                    dedupe_key(p),
                ])? as u64;
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

    /// After an Export import: API Plays at or before the Export's newest Play are in the Export now.
    pub fn replace_api_plays_before(&mut self, source: Source, ts: i64) -> Result<u64> {
        Ok(self.conn.execute(
            "DELETE FROM plays WHERE source = ?1 AND origin = 'api' AND played_at <= ?2",
            params![source.as_str(), ts],
        )? as u64)
    }

    pub fn has_demo_data(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM plays WHERE origin = 'demo')",
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
    use crate::error::Error;

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
    fn replace_api_plays_before_is_inclusive_and_scoped() {
        let mut store = Store::open_in_memory().unwrap();
        let with = |source, origin, creator: &str, at| Play {
            origin,
            ..play(source, creator, "Song", at)
        };
        store
            .insert_plays(&[
                with(Source::Spotify, Origin::Api, "before", 100),
                with(Source::Spotify, Origin::Api, "at", 200),
                with(Source::Spotify, Origin::Api, "after", 300),
                with(Source::Spotify, Origin::Export, "export", 100),
                with(Source::Youtube, Origin::Api, "other source", 100),
            ])
            .unwrap();
        assert_eq!(
            store
                .replace_api_plays_before(Source::Spotify, 200)
                .unwrap(),
            2
        );
        let mut left: Vec<String> = store
            .plays(None)
            .unwrap()
            .into_iter()
            .map(|p| p.creator)
            .collect();
        left.sort();
        assert_eq!(left, ["after", "export", "other source"]);
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

    #[test]
    fn settings_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting::<i64>("missing").unwrap(), None);
        store.set_setting("n", &7_i64).unwrap();
        store.set_setting("n", &8_i64).unwrap();
        assert_eq!(store.get_setting::<i64>("n").unwrap(), Some(8));
    }
}
