use rostery_core::{demo, ingest::ImportSummary, model::Totals, store::Store, Error};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

pub struct AppState {
    pub db_path: PathBuf,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// Each command opens its own Store, so there is no shared connection to lock.

#[tauri::command(async)]
pub fn totals(state: State<'_, AppState>) -> Result<Totals, Error> {
    Store::open(&state.db_path)?.totals()
}

#[tauri::command(async)]
pub fn load_demo(state: State<'_, AppState>) -> Result<Totals, Error> {
    let mut store = Store::open(&state.db_path)?;
    demo::load(&mut store, now())?;
    store.totals()
}

#[tauri::command(async)]
pub fn clear_demo(state: State<'_, AppState>) -> Result<Totals, Error> {
    let mut store = Store::open(&state.db_path)?;
    demo::clear(&mut store)?;
    store.totals()
}

#[tauri::command(async)]
pub fn has_demo_data(state: State<'_, AppState>) -> Result<bool, Error> {
    Store::open(&state.db_path)?.has_demo_data()
}

#[tauri::command(async)]
pub fn import_path(state: State<'_, AppState>, path: String) -> Result<ImportSummary, Error> {
    let mut store = Store::open(&state.db_path)?;
    rostery_core::ingest::import_path(&mut store, Path::new(&path))
}
