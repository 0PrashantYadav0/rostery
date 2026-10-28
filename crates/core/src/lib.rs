//! Rostery core: history, storage and (later) the Universe, Roast and Share Site.
//! No UI, keychain or async runtime lives here.

pub mod demo;
pub mod error;
pub mod ingest;
pub mod model;
pub mod store;

pub use error::{Error, Result};
