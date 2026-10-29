//! Rostery core: history, storage, the Universe and (later) the Roast and Share Site.
//! No UI, keychain or async runtime lives here.

pub mod demo;
pub mod error;
pub mod ingest;
pub mod model;
pub mod store;
pub mod universe;

pub use error::{Error, Result};
