//! Reads an Export (a `.json`, a `.zip` or a folder of them) into the Store.
//! Formats are found by content, never by file name.

mod spotify_export;
mod takeout;

use crate::error::{Error, Result};
use crate::model::{Play, Source};
use crate::store::{dedupe_key, Store};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashSet;
use std::fmt::Display;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// How much of a file is looked at to tell which Export it is from.
const SNIFF_BYTES: usize = 4096;
/// The biggest single history file Rostery will hold in memory.
const MAX_FILE_BYTES: u64 = 1 << 30;
/// A Takeout page opens with a long stylesheet, so its first links are further in than a JSON row.
const HTML_SNIFF_BYTES: u64 = 1 << 20;
/// A file value decides which API Plays get deleted, so a Play from the future is never believed.
/// A day of slack covers a computer whose clock runs a little behind.
const CLOCK_SLACK: i64 = 86_400;
/// Longer than any real ID. A video's has 11 characters, a Spotify Item's 22 and a channel's 24.
const MAX_ID_BYTES: usize = 64;
/// How to get history out of Takeout in the one format Rostery reads.
const TAKEOUT_STEPS: &str = "go to takeout.google.com, click Deselect all, tick YouTube and \
     YouTube Music, click Multiple formats and set history to JSON";

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
    pub files_skipped: u32,
    pub added: u64,
    pub duplicates: u64,
    pub excluded_private: u64,
    pub excluded_ads: u64,
    pub rejected: u64,
    /// Why the first skipped file was left out, for when the import still found other files.
    pub skipped_reason: Option<String>,
}

/// Looks only at the first 4 KB of a file.
pub fn detect_format(head: &[u8]) -> Option<Format> {
    let head = &head[..head.len().min(SNIFF_BYTES)];
    let has = |needle: &str| contains(head, needle);
    if has("\"ms_played\"") {
        Some(Format::SpotifyExtended)
    } else if has("\"msPlayed\"") {
        Some(Format::SpotifyAccount)
    // A removed video has no `titleUrl`, so a file of only those is known by its `header`.
    } else if has("\"products\"") && (has("\"titleUrl\"") || has("\"header\"")) {
        Some(Format::TakeoutActivity)
    } else {
        None
    }
}

/// Imports every history file found in `path`: a `.json`, a `.zip` or a folder, recursively.
pub fn import_path(store: &mut Store, path: &Path) -> Result<ImportSummary> {
    import_paths(store, &[path])
}

/// One import of everything found in `paths`. A problem with the only path the person chose is an
/// error. Among several, that path is one more stray file, like any inside a folder or a zip.
pub fn import_paths(store: &mut Store, paths: &[impl AsRef<Path>]) -> Result<ImportSummary> {
    let mut run = Import {
        store,
        summary: ImportSummary::default(),
        spans: Vec::new(),
        private: HashSet::new(),
        removed_private: 0,
        first_skip: None,
    };
    if let [only] = paths {
        run.path(only.as_ref())?;
    } else {
        for path in paths {
            let result = run.path(path.as_ref());
            run.skip_bad_file(result)?;
        }
    }
    if run.summary.files_read == 0 {
        return Err(run.first_skip.unwrap_or_else(|| {
            Error::Import(format!(
                "Rostery did not find a Spotify or YouTube Export there. For Spotify, go to Account, \
                 Privacy settings, Download your data, and request Extended streaming history. For \
                 YouTube, {TAKEOUT_STEPS}. Each one emails you a link when the Export is ready. Choose \
                 the zip, the JSON files or the folder it gives you."
            ))
        }));
    }
    // Inside a file's own span an Export holds what the API saw, so those copies can go. Anywhere
    // else, including a gap between two files, an API Play may be held nowhere else. With a file
    // left unread the Export is not whole, and an API Play cannot be fetched again.
    // ponytail: an API Play between two files of one Export stays too, and can count twice if its
    // Export copy landed in another minute. At most one per file boundary, and losing it is worse.
    if run.summary.files_skipped == 0 {
        for &(source, oldest, newest) in &run.spans {
            run.store
                .replace_api_plays_in_span(source, oldest, newest)?;
        }
    }
    // Net of what private rows took back out, so `added` matches what the dials gain.
    run.summary.added = run.summary.added.saturating_sub(run.removed_private);
    run.summary.skipped_reason = run.first_skip.map(|e| e.to_string());
    Ok(run.summary)
}

struct Import<'a> {
    store: &'a mut Store,
    summary: ImportSummary,
    /// Each file's Source, with its oldest and newest Play.
    spans: Vec<(Source, i64, i64)>,
    /// Dedupe keys of the private rows seen so far in this import.
    private: HashSet<String>,
    /// Stored Plays taken back out because a private row named them.
    removed_private: u64,
    /// Becomes the error when no file at all could be read, and otherwise the summary's reason.
    first_skip: Option<Error>,
}

impl Import<'_> {
    /// A bad file must not cost the files around it. It is counted, and the import carries on.
    /// Anything else, such as a database failure, still stops the import.
    fn skip_bad_file(&mut self, result: Result<()>) -> Result<()> {
        match result {
            Err(e @ Error::Import(_)) => {
                self.summary.files_skipped += 1;
                self.first_skip.get_or_insert(e);
                Ok(())
            }
            other => other,
        }
    }

    fn path(&mut self, path: &Path) -> Result<()> {
        std::fs::metadata(path).map_err(|e| unreadable(path.display(), e))?;
        if path.is_dir() {
            let mut entries = std::fs::read_dir(path)
                .and_then(|dir| dir.collect::<std::io::Result<Vec<_>>>())
                .map_err(|e| unreadable(path.display(), e))?;
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                // file_type does not follow links, so a link back up the tree cannot loop.
                let kind = entry
                    .file_type()
                    .map_err(|e| unreadable(path.display(), e))?;
                if kind.is_dir() || kind.is_file() {
                    let result = self.path(&entry.path());
                    self.skip_bad_file(result)?;
                }
            }
            return Ok(());
        }
        match extension(path).as_deref() {
            Some("zip") => self.zip(path),
            Some("json" | "html") => {
                let file = File::open(path).map_err(|e| unreadable(path.display(), e))?;
                self.file(&path.display().to_string(), file)
            }
            _ => Ok(()),
        }
    }

    fn zip(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path).map_err(|e| unreadable(path.display(), e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| bad_zip(path))?;
        for i in 0..archive.len() {
            let result = self.zip_entry(path, &mut archive, i);
            self.skip_bad_file(result)?;
        }
        Ok(())
    }

    fn zip_entry(
        &mut self,
        path: &Path,
        archive: &mut zip::ZipArchive<File>,
        i: usize,
    ) -> Result<()> {
        let mut entry = archive.by_index(i).map_err(|_| bad_zip(path))?;
        // A name that is not valid text cannot be a history file from a Source.
        let Ok(name) = entry.name().map(|n| n.into_owned()) else {
            return Ok(());
        };
        if !entry.is_file() {
            return Ok(());
        }
        self.file(&format!("{}/{name}", path.display()), &mut entry)
    }

    /// One file from a folder or a zip. Anything that is not history is not a problem either.
    fn file(&mut self, label: &str, mut reader: impl Read) -> Result<()> {
        match extension(Path::new(label)).as_deref() {
            Some("json") => match read_history(reader, label)? {
                Some((format, bytes)) => self.add(label, format, &bytes),
                None => Ok(()),
            },
            Some("html") if is_takeout_html(&mut reader, label)? => {
                Err(Error::Import(format!(
                    "Rostery cannot read {label}. It is a Google Takeout in HTML, which is what Takeout \
                     gives you for history unless you change it. To get JSON, {TAKEOUT_STEPS}, then \
                     create the Export again. Google emails you a link when it is ready."
                )))
            }
            _ => Ok(()),
        }
    }

    /// One transaction per file.
    fn add(&mut self, label: &str, format: Format, bytes: &[u8]) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let parsed = match format {
            Format::SpotifyExtended => spotify_export::parse_extended(bytes, now),
            Format::SpotifyAccount => spotify_export::parse_account(bytes, now),
            Format::TakeoutActivity => takeout::parse(bytes, now),
        };
        let mut parsed = match parsed {
            Ok(parsed) => parsed,
            // Valid JSON that is not a list of rows, such as a Wrapped summary, only looked like
            // history. Sniffing is a substring match, so this is not a problem with the file.
            Err(e) if e.is_data() => return Ok(()),
            Err(_) => {
                return Err(Error::Import(format!(
                    "Rostery could not read {label}. It looks damaged or cut short. Download the \
                     Export again from the Source and retry."
                )))
            }
        };
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
        // A parser reads one Source, so the first Play speaks for the file.
        let times = parsed.plays.iter().map(|p| p.played_at);
        if let (Some(play), Some(oldest), Some(newest)) =
            (parsed.plays.first(), times.clone().min(), times.max())
        {
            self.spans.push((play.source, oldest, newest));
        }
        let count = self.store.insert_plays(&parsed.plays)?;
        let s = &mut self.summary;
        s.files_read += 1;
        s.added += count.added;
        s.duplicates += count.duplicates;
        s.excluded_private += parsed.excluded_private;
        s.excluded_ads += parsed.excluded_ads;
        s.rejected += parsed.rejected;
        Ok(())
    }
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn bad_zip(path: &Path) -> Error {
    Error::Import(format!(
        "Rostery could not open {}. It looks damaged or only part downloaded. Download the zip again \
         from the Source and retry.",
        path.display()
    ))
}

fn unreadable(label: impl Display, e: std::io::Error) -> Error {
    Error::Import(match e.kind() {
        ErrorKind::NotFound => {
            format!("Rostery could not find {label}. Check that it is still there and choose it again.")
        }
        ErrorKind::PermissionDenied => format!(
            "Rostery does not have permission to read {label}. Allow it in your system settings, or \
             move the file somewhere you can open it, and try again."
        ),
        _ => format!(
            "Rostery could not read {label}. Check that it is the original file from the Source and \
             that you can open it."
        ),
    })
}

/// Reads the head, and the rest only when the head looks like a known Export.
fn read_history(mut reader: impl Read, label: &str) -> Result<Option<(Format, Vec<u8>)>> {
    let mut bytes = Vec::new();
    (&mut reader)
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(label, e))?;
    let Some(format) = detect_format(&bytes) else {
        return Ok(None);
    };
    reader
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(label, e))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Import(format!(
            "Rostery cannot read {label}, because it is over 1 GiB. Check that it is the original file \
             from the Source."
        )));
    }
    Ok(Some((format, bytes)))
}

/// A page of YouTube history names the product in Takeout's header and links to what was watched.
/// Anything else is not a problem: a Chrome or Search page and a bookmarks file can link to a video,
/// and the index page every Takeout carries, `archive_browser.html`, has no such links.
// ponytail: the header's exact bytes come from parsers of these pages, not from a real file. Check
// at stop point 1. If they differ, a page alone gets the general message, which has the same steps.
fn is_takeout_html(reader: impl Read, label: &str) -> Result<bool> {
    let mut head = Vec::new();
    reader
        .take(HTML_SNIFF_BYTES)
        .read_to_end(&mut head)
        .map_err(|e| unreadable(label, e))?;
    Ok(contains(&head, "mdl-typography--title\">YouTube")
        && (contains(&head, "youtube.com/watch") || contains(&head, "youtube.com/shorts")))
}

/// An ID from a file goes on to build URLs, and a file is untrusted, so it has to look like an ID.
pub(crate) fn looks_like_id(id: &str) -> bool {
    (1..=MAX_ID_BYTES).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// What one history file holds, whichever Source wrote it.
#[derive(Default)]
struct Parsed {
    plays: Vec<Play>,
    /// Private rows readable enough to name the Play. They are never stored.
    private: Vec<Play>,
    excluded_private: u64,
    excluded_ads: u64,
    rejected: u64,
}

impl Parsed {
    fn keep(&mut self, play: Option<Play>) {
        match play {
            Some(play) => self.plays.push(play),
            None => self.rejected += 1,
        }
    }
}

/// The file must be a list. Each row is read on its own, so a row of the wrong shape costs that
/// row and not the file. `RawValue` only marks where a row is, which keeps the fields a row
/// struct does not name out of memory as parsed data.
fn each_row<T: DeserializeOwned>(
    bytes: &[u8],
    mut read: impl FnMut(&mut Parsed, T),
) -> serde_json::Result<Parsed> {
    let mut out = Parsed::default();
    for raw in serde_json::from_slice::<Vec<&RawValue>>(bytes)? {
        match serde_json::from_str(raw.get()) {
            Ok(row) => read(&mut out, row),
            Err(_) => out.rejected += 1,
        }
    }
    Ok(out)
}

/// A file's time as Unix seconds, if it could be a real Play.
fn played_at(time: &str, earliest: i64, now: i64) -> Option<i64> {
    parse_utc(time).filter(|t| (earliest..=now + CLOCK_SLACK).contains(t))
}

/// `2024-03-01T13:05:42Z` or `2024-03-01 13:05` as Unix seconds. Both Sources write UTC, and
/// Spotify's account format has no seconds. Anything after the seconds (fraction, `Z`) is ignored.
fn parse_utc(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let digits = b.get(from..to)?;
        digits
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| digits.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let shape_ok = b.get(4) == Some(&b'-')
        && b.get(7) == Some(&b'-')
        && matches!(b.get(10), Some(b'T' | b' '))
        && b.get(13) == Some(&b':');
    if !shape_ok {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute) = (num(11, 13)?, num(14, 16)?);
    let second = if b.get(16) == Some(&b':') {
        num(17, 19)?
    } else {
        0
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last_day = match month {
        2 => 28 + i64::from(leap),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=12).contains(&month)
        || !(1..=last_day).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    // Days since 1970-01-01 from the civil date (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}
