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
    /// A file the user picked. The message already says what to do next.
    #[error("{0}")]
    Import(String),
    /// A Provider or a Roast went wrong. The message already says what to do next.
    #[error("{0}")]
    Provider(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Lets a Tauri command return `Result<T, Error>`; the webview gets the display string.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
