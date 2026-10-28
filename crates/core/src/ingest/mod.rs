//! Reads an Export (a `.json`, a `.zip` or a folder of them) into the Store.
//! Formats are found by content, never by file name.

mod spotify_export;

use crate::error::{Error, Result};
use crate::model::Source;
use crate::store::{dedupe_key, Store};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// How much of a file is looked at to tell which Export it is from.
const SNIFF_BYTES: usize = 4096;
/// The biggest single history file Rostery will hold in memory.
const MAX_FILE_BYTES: u64 = 1 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    SpotifyExtended,
    SpotifyAccount,
    TakeoutActivity,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub files_read: u32,
    pub added: u64,
    pub duplicates: u64,
    pub excluded_private: u64,
    pub excluded_ads: u64,
    pub rejected: u64,
}

/// Looks only at the first 4 KB of a file.
pub fn detect_format(head: &[u8]) -> Option<Format> {
    let head = &head[..head.len().min(SNIFF_BYTES)];
    let has = |needle: &str| head.windows(needle.len()).any(|w| w == needle.as_bytes());
    if has("\"ms_played\"") {
        Some(Format::SpotifyExtended)
    } else if has("\"msPlayed\"") {
        Some(Format::SpotifyAccount)
    } else if has("\"titleUrl\"") && has("\"products\"") {
        Some(Format::TakeoutActivity)
    } else {
        None
    }
}

/// Imports every history file found in `path`: a `.json`, a `.zip` or a folder, recursively.
pub fn import_path(store: &mut Store, path: &Path) -> Result<ImportSummary> {
    std::fs::metadata(path).map_err(|e| unreadable(path, e))?;
    let mut run = Import {
        store,
        summary: ImportSummary::default(),
        newest_spotify: None,
        private: HashSet::new(),
        removed_private: 0,
    };
    run.path(path)?;
    if run.summary.files_read == 0 {
        return Err(Error::Import(
            "Rostery did not find a Spotify or YouTube Export there. Choose the .zip, the .json files \
             or the folder from Spotify's \"Download your data\" (Extended streaming history or \
             Account data), or the watch history JSON from Google Takeout. A Takeout in HTML cannot \
             be read, so request it again as JSON."
                .into(),
        ));
    }
    // The Export now holds every API Play up to its newest, so those copies can go.
    if let Some(ts) = run.newest_spotify {
        run.store.replace_api_plays_before(Source::Spotify, ts)?;
    }
    run.summary.added = run.summary.added.saturating_sub(run.removed_private);
    Ok(run.summary)
}

struct Import<'a> {
    store: &'a mut Store,
    summary: ImportSummary,
    newest_spotify: Option<i64>,
    /// Dedupe keys of the private rows seen so far in this import.
    private: HashSet<String>,
    /// Stored Plays taken back out because a private row named them.
    removed_private: u64,
}

impl Import<'_> {
    fn path(&mut self, path: &Path) -> Result<()> {
        if path.is_dir() {
            let mut entries = std::fs::read_dir(path)
                .and_then(|dir| dir.collect::<std::io::Result<Vec<_>>>())
                .map_err(|e| unreadable(path, e))?;
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                // file_type does not follow links, so a link back up the tree cannot loop.
                let kind = entry.file_type().map_err(|e| unreadable(path, e))?;
                if kind.is_dir() || kind.is_file() {
                    self.path(&entry.path())?;
                }
            }
            return Ok(());
        }
        match extension(path).as_deref() {
            Some("zip") => self.zip(path),
            Some("json") => {
                let file = File::open(path).map_err(|e| unreadable(path, e))?;
                match read_history(file).map_err(|e| unreadable(path, e))? {
                    Some((format, bytes)) => self.add(&path.display().to_string(), format, &bytes),
                    None => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }

    fn zip(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path).map_err(|e| unreadable(path, e))?;
        let bad_zip = |e: zip::result::ZipError| {
            Error::Import(format!(
                "Rostery could not open {}: {e}. Download the zip from Spotify again and retry.",
                path.display()
            ))
        };
        let mut archive = zip::ZipArchive::new(file).map_err(bad_zip)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(bad_zip)?;
            // A name that is not valid text cannot be a Spotify history file.
            let Ok(name) = entry.name().map(|n| n.into_owned()) else {
                continue;
            };
            if !entry.is_file() || extension(Path::new(&name)).as_deref() != Some("json") {
                continue;
            }
            let label = format!("{}/{name}", path.display());
            if let Some((format, bytes)) =
                read_history(&mut entry).map_err(|e| unreadable(Path::new(&label), e))?
            {
                self.add(&label, format, &bytes)?;
            }
        }
        Ok(())
    }

    /// One transaction per file.
    fn add(&mut self, label: &str, format: Format, bytes: &[u8]) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut parsed = match format {
            Format::SpotifyExtended => spotify_export::parse_extended(bytes, now),
            Format::SpotifyAccount => spotify_export::parse_account(bytes, now),
            // Task 3 replaces this arm.
            Format::TakeoutActivity => {
                return Err(Error::Import(format!(
                    "{label} is a Google Takeout file. Importing YouTube history is not available yet."
                )))
            }
        }
        .map_err(|e| {
            Error::Import(format!(
                "Rostery could not read {label}: {e}. Download the Export again from the Source and retry."
            ))
        })?;
        // Account data has no private flag, so an extended private row also speaks for the
        // account copy of the same Play, stored already or later in this import.
        // ponytail: an account file imported on its own after the extended one still stores the
        // Play. Catching that means writing private Plays to disk, which they must never reach.
        let private: Vec<String> = parsed.private.iter().map(dedupe_key).collect();
        self.removed_private += self.store.delete_by_dedupe_key(&private)?;
        self.private.extend(private);
        if !self.private.is_empty() {
            parsed
                .plays
                .retain(|p| !self.private.contains(&dedupe_key(p)));
        }
        let newest = parsed.plays.iter().map(|p| p.played_at).max();
        self.newest_spotify = self.newest_spotify.max(newest);
        let count = self.store.insert_plays(&parsed.plays)?;
        let s = &mut self.summary;
        s.files_read += 1;
        s.added += count.added;
        s.duplicates += count.duplicates;
        s.excluded_private += parsed.excluded_private;
        s.rejected += parsed.rejected;
        Ok(())
    }
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

fn unreadable(path: &Path, e: impl std::fmt::Display) -> Error {
    Error::Import(format!(
        "Rostery could not read {}: {e}. Check that it is the original file from the Source and that you can open it.",
        path.display()
    ))
}

/// Reads the head, and the rest only when the head looks like a known Export.
fn read_history(mut reader: impl Read) -> std::io::Result<Option<(Format, Vec<u8>)>> {
    let mut bytes = Vec::new();
    (&mut reader)
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut bytes)?;
    let Some(format) = detect_format(&bytes) else {
        return Ok(None);
    };
    reader.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(std::io::Error::other("it is over 1 GiB"));
    }
    Ok(Some((format, bytes)))
}
