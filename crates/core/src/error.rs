use serde::{Serialize, Serializer};

/// Messages are written for the person using the app, with what to do next.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Rostery could not use its local database: {0}. Close any other copy of Rostery and try again.")]
    Db(#[from] rusqlite::Error),
    #[error("Rostery could not read or write its data folder: {0}. Check that the folder exists and is writable.")]
    Io(#[from] std::io::Error),
    #[error("Rostery found saved data it could not read: {0}. Reset that setting and try again.")]
    Json(#[from] serde_json::Error),
    #[error("Your own history is already here, so Rostery keeps demo data out of it, because a Roast of the two mixed would be wrong. The Universe and Roast screens are ready for what you have.")]
    RealHistoryStored,
    #[error("Rostery could not read your last Roast. You can write a new one, and it takes the old one's place.")]
    UnreadableRoast,
    /// A file the user picked. The message already says what to do next.
    #[error("{0}")]
    Import(String),
    /// A Provider or a Roast went wrong. The message already says what to do next.
    #[error("{0}")]
    Provider(String),
    /// Settings that cannot be kept as they are. The message already says what to do next.
    #[error("{0}")]
    Settings(String),
    /// A Share Site that could not be built or written. The message already says what to do next.
    #[error("{0}")]
    Export(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Lets a Tauri command return `Result<T, Error>`; the webview gets the display string.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
