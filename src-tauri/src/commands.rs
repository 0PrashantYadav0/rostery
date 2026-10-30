use rostery_core::{
    demo,
    ingest::ImportSummary,
    model::Totals,
    store::Store,
    universe::{self, Universe, UniverseFilter, UniverseInput},
    Error,
};
use std::collections::HashMap;
use std::path::PathBuf;
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
pub fn import_paths(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<ImportSummary, Error> {
    let mut store = Store::open(&state.db_path)?;
    rostery_core::ingest::import_paths(&mut store, &paths)
}

#[tauri::command(async)]
pub fn get_universe(state: State<'_, AppState>, filter: UniverseFilter) -> Result<Universe, Error> {
    let store = Store::open(&state.db_path)?;
    Ok(universe::build(
        &UniverseInput {
            plays: &store.plays(None)?,
            affinities: &store.affinities()?,
            interests: &store.interests()?,
            galaxy_names: &HashMap::new(),
        },
        filter,
    ))
}
