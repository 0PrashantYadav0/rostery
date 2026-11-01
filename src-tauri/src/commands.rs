use rostery_core::{
    demo,
    digest::{self, Digest},
    ingest::ImportSummary,
    llm::{self, Provider},
    model::Totals,
    roast::{self, StoredRoast},
    store::Store,
    universe::{self, Universe, UniverseFilter, UniverseInput},
    Error,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex, TryLockError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub db_path: PathBuf,
    /// Held while a Roast runs, so a second one gets an error and not a second CLI.
    pub roast_running: Mutex<()>,
}

/// A CLI says nothing until it is done, so these only keep the person company while it works.
const WAITING: [&str; 5] = [
    "Reading your Digest",
    "Going through your late nights",
    "Writing the Burns",
    "Working out your score",
    "Still going. A long history takes a while.",
];
const WAITING_EVERY: Duration = Duration::from_secs(15);

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

fn run_roast(state: &AppState, progress: impl Fn(&str) + Sync) -> Result<StoredRoast, Error> {
    // A Roast that panicked is over, so its poisoned lock does not count as one running.
    let _running =
        match state.roast_running.try_lock() {
            Ok(running) => running,
            Err(TryLockError::Poisoned(running)) => running.into_inner(),
            Err(TryLockError::WouldBlock) => return Err(Error::Provider(
                "A Roast is already running. Its result will show on the Roast screen when it is \
                 done."
                    .to_owned(),
            )),
        };
    progress("Reading your history");
    let store = Store::open(&state.db_path)?;
    let from_demo = store.has_demo_data()?;
    let digest = digest::from_store(&store, now())?;
    let provider = Provider::ClaudeCode { model: None };
    let (finished, waiting) = mpsc::channel::<()>();
    let roast = std::thread::scope(|scope| {
        let progress = &progress;
        scope.spawn(move || {
            for phrase in WAITING {
                progress(phrase);
                if waiting.recv_timeout(WAITING_EVERY) != Err(mpsc::RecvTimeoutError::Timeout) {
                    return;
                }
            }
        });
        let roast = roast::generate(
            &provider,
            &digest,
            roast::default_system_prompt(),
            llm::DEFAULT_TIMEOUT,
        );
        drop(finished);
        roast
    })?;
    // Sources stays usable during a Roast, so the demo this one is about can leave before it lands.
    if from_demo && !store.has_demo_data()? {
        return Err(Error::Provider(
            "Your history changed while this Roast was being written, so Rostery did not keep it. \
             Press Roast me to write one about what is stored now."
                .to_owned(),
        ));
    }
    store.save_roast(provider.label(), &roast, now(), from_demo)
}

#[tauri::command(async)]
pub fn generate_roast(app: AppHandle, state: State<'_, AppState>) -> Result<StoredRoast, Error> {
    run_roast(&state, |phrase| {
        // Only the words on screen are lost if this fails, and the Roast goes on.
        let _ = app.emit("roast-progress", phrase);
    })
}

#[tauri::command(async)]
pub fn latest_roast(state: State<'_, AppState>) -> Result<Option<StoredRoast>, Error> {
    Store::open(&state.db_path)?.latest_roast()
}

#[tauri::command(async)]
pub fn digest_preview(state: State<'_, AppState>) -> Result<Digest, Error> {
    digest::from_store(&Store::open(&state.db_path)?, now())
}

// Debug only, because a release build ignores the override and would call the real CLI.
#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use rostery_core::model::{Origin, Play, Source};
    use std::os::unix::fs::PermissionsExt;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::MutexGuard;
    use std::time::Instant;
    use tempfile::TempDir;

    const REPLY: &str = r#"{"type":"result","is_error":false,"result":"{\"headline\":\"Heard\",\"opening\":\"You.\",\"burns\":[{\"title\":\"Burn\",\"body\":\"Ouch.\"}],\"verdict\":{\"archetype\":\"Night Clerk\",\"score\":40,\"sentence\":\"Sit down.\"}}"}"#;
    const REFUSED: &str = r#"{"type":"result","is_error":true,"result":"Usage limit reached"}"#;
    const CAT_REPLY: &str = "cat \"$here/reply\"";

    /// These point the Provider at a fake `claude` through the environment, so one runs at a time.
    static ENV: Mutex<()> = Mutex::new(());

    struct Rig {
        dir: TempDir,
        state: AppState,
        _env: MutexGuard<'static, ()>,
    }

    impl Rig {
        /// A `claude` that notes each call in a file beside it, then runs `body`.
        fn new(body: &str) -> Rig {
            let env = ENV.lock().unwrap_or_else(|e| e.into_inner());
            let dir = TempDir::new().unwrap();
            let bin = dir.path().join("claude");
            std::fs::write(
                &bin,
                format!(
                    "#!/bin/sh\nhere=$(dirname \"$0\")\necho call >> \"$here/calls\"\n{body}\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::env::set_var("SHELL", "/bin/sh");
            std::env::set_var("ROSTERY_CLAUDE_BIN", &bin);
            let state = AppState {
                db_path: dir.path().join("rostery.db"),
                roast_running: Mutex::new(()),
            };
            Rig {
                dir,
                state,
                _env: env,
            }
        }

        /// The same, with demo history stored and `reply` ready for the fake to give.
        fn with_history(body: &str, reply: &str) -> Rig {
            let rig = Rig::new(body);
            rig.reply(reply);
            demo::load(&mut Store::open(&rig.state.db_path).unwrap(), now()).unwrap();
            rig
        }

        fn reply(&self, reply: &str) {
            std::fs::write(self.dir.path().join("reply"), reply).unwrap();
        }

        fn calls(&self) -> usize {
            std::fs::read_to_string(self.dir.path().join("calls"))
                .map_or(0, |calls| calls.lines().count())
        }

        fn latest(&self) -> Option<StoredRoast> {
            Store::open(&self.state.db_path)
                .unwrap()
                .latest_roast()
                .unwrap()
        }
    }

    fn silent(_: &str) {}

    #[test]
    fn a_roast_is_saved_with_its_provider_and_says_how_it_is_going() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);

        let said = Mutex::new(Vec::new());
        let saved = run_roast(&rig.state, |phrase| {
            said.lock().unwrap().push(phrase.to_owned())
        })
        .unwrap();

        assert_eq!(saved.roast.headline, "Heard");
        assert_eq!(saved.provider_label, "Claude Code");
        assert!(saved.created_at > 0);
        assert_eq!(rig.latest(), Some(saved));
        let said = said.into_inner().unwrap();
        assert_eq!(said[0], "Reading your history");
        assert_eq!(said[1], WAITING[0]);
    }

    #[test]
    fn a_second_roast_gets_an_error_while_the_first_is_running() {
        let rig = Rig::with_history(
            "touch \"$here/started\"\n\
             while [ ! -e \"$here/go\" ]; do sleep 0.02; done\n\
             cat \"$here/reply\"",
            REPLY,
        );
        let (started, go) = (rig.dir.path().join("started"), rig.dir.path().join("go"));

        std::thread::scope(|scope| {
            let first = scope.spawn(|| run_roast(&rig.state, silent));
            let until = Instant::now() + Duration::from_secs(10);
            while !started.exists() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(started.exists(), "the first Roast never began");

            let message = run_roast(&rig.state, silent).unwrap_err().to_string();
            assert!(message.contains("already running"), "{message}");
            assert_eq!(rig.calls(), 1, "the second must not start a CLI");

            std::fs::write(&go, "").unwrap();
            assert_eq!(first.join().unwrap().unwrap().roast.headline, "Heard");
        });

        // It is over, so another may start.
        run_roast(&rig.state, silent).unwrap();
        assert_eq!(rig.calls(), 2);
    }

    #[test]
    fn a_failed_roast_keeps_the_last_one() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        let saved = run_roast(&rig.state, silent).unwrap();

        rig.reply(REFUSED);
        let message = run_roast(&rig.state, silent).unwrap_err().to_string();

        assert!(message.contains("Usage limit reached"), "{message}");
        assert_eq!(rig.latest(), Some(saved));
    }

    #[test]
    fn a_roast_of_demo_data_goes_when_the_demo_does() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        run_roast(&rig.state, silent).unwrap();
        assert!(rig.latest().is_some());

        demo::clear(&mut Store::open(&rig.state.db_path).unwrap()).unwrap();

        assert_eq!(rig.latest(), None);
    }

    #[test]
    fn a_roast_of_real_history_stays_when_demo_data_is_cleared() {
        let rig = Rig::new(CAT_REPLY);
        rig.reply(REPLY);
        let mut store = Store::open(&rig.state.db_path).unwrap();
        store
            .insert_plays(&[Play {
                source: Source::Spotify,
                origin: Origin::Export,
                played_at: 1_700_000_000,
                duration_ms: Some(200_000),
                creator: "Real Band".into(),
                creator_ref: None,
                item: "Real Song".into(),
                item_ref: None,
                skipped: false,
            }])
            .unwrap();
        let saved = run_roast(&rig.state, silent).unwrap();

        demo::clear(&mut store).unwrap();

        assert_eq!(rig.latest(), Some(saved));
    }

    /// The Digest went out while the demo was stored, so the Roast that comes back is about it.
    #[test]
    fn a_roast_of_demo_data_that_lands_after_the_demo_left_is_not_kept() {
        let rig = Rig::with_history(
            "touch \"$here/started\"\n\
             while [ ! -e \"$here/go\" ]; do sleep 0.02; done\n\
             cat \"$here/reply\"",
            REPLY,
        );
        let (started, go) = (rig.dir.path().join("started"), rig.dir.path().join("go"));

        let result = std::thread::scope(|scope| {
            let run = scope.spawn(|| run_roast(&rig.state, silent));
            let until = Instant::now() + Duration::from_secs(10);
            while !started.exists() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(started.exists(), "the Roast never began");

            demo::clear(&mut Store::open(&rig.state.db_path).unwrap()).unwrap();
            std::fs::write(&go, "").unwrap();
            run.join().unwrap()
        });

        let message = result.unwrap_err().to_string();
        assert!(message.contains("Roast me"), "{message}");
        assert_eq!(rig.latest(), None);
    }

    #[test]
    fn no_history_asks_for_some_and_never_calls_the_provider() {
        let rig = Rig::new("exit 1");
        let message = run_roast(&rig.state, silent).unwrap_err().to_string();
        assert!(message.contains("nothing to roast yet"), "{message}");
        assert_eq!(rig.calls(), 0);
        assert_eq!(rig.latest(), None);
    }

    #[test]
    fn a_roast_that_panics_does_not_block_the_next_one() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);

        let run = catch_unwind(AssertUnwindSafe(|| {
            run_roast(&rig.state, |_| panic!("broken on purpose"))
        }));
        assert!(run.is_err());

        run_roast(&rig.state, silent).unwrap();
    }
}
