//! Rostery core: history, storage, the Universe and (later) the Roast and Share Site.
//! No UI, keychain or async runtime lives here.

pub mod demo;
pub mod deploy;
pub mod digest;
pub mod error;
pub mod export;
pub mod ingest;
pub mod interests;
pub mod llm;
pub mod model;
pub mod oauth;
pub mod roast;
pub mod spotify_api;
pub mod store;
pub mod universe;
pub mod youtube_api;

pub use error::{Error, Result};
