use crate::secrets;
use rostery_core::{
    demo,
    deploy::{self, DeployResult, DeployStatus},
    digest,
    export::{self, ExportOptions, ExportResult, SiteFile},
    ingest::ImportSummary,
    interests::{self, CreatorToTag},
    llm::{self, Disclosure, Provider, ProviderStatus, Settings},
    model::Totals,
    roast::{self, FinishedRoast, StoredRoast},
    store::Store,
    universe::{self, Universe, UniverseFilter},
    Error,
};
use std::path::PathBuf;
use std::sync::{mpsc, Mutex, TryLockError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

pub struct AppState {
    pub db_path: PathBuf,
    /// Held while a Roast runs, so a second one gets an error and not a second CLI.
    pub roast_running: Mutex<()>,
}

/// A Provider says nothing until it is done, so these only keep the person company while it works.
const WAITING: [&str; 5] = [
    "Reading your Digest",
    "Going through your late nights",
    "Writing the Burns",
    "Working out your score",
    "Still going. A long history takes a while.",
];
const WAITING_EVERY: Duration = Duration::from_secs(15);
/// Said while a Provider tags Creators, which only happens when some are new to it.
const TAGGING: &str = "Tagging your Creators with Interests";
/// A finished Roast cost someone minutes, and an import is what a person looks at while they wait. So
/// its one save outlasts any import, where everything else gets the usual 5 seconds.
const SAVE_WAIT: Duration = Duration::from_secs(120);

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
    universe::from_store(&Store::open(&state.db_path)?, filter)
}

fn run_roast(state: &AppState, progress: impl Fn(&str) + Sync) -> Result<FinishedRoast, Error> {
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
    let mut store = Store::open(&state.db_path)?;
    // Read once, here, so changing Settings during a Roast only changes the next one.
    let settings = Settings::load(&store)?;
    let provider = settings.provider_to_send_to()?;
    // Tags are worth having and not worth a Roast, so a failure becomes a sentence beside the finished
    // Roast, and the next Roast asks again.
    let not_tagged = tag_new_creators(provider, &mut store, &progress)
        .err()
        .map(|e| {
            format!(
                "Rostery could not tag your Creators this time, so this Roast added no new Links. It \
                 will ask again on your next Roast. {e}"
            )
        });
    // After that, since the demo can leave during the tag step, which has a long wait of its own.
    let from_demo = store.has_demo_data()?;
    let digest = digest::from_store(&store, now())?;
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
        let roast = roast::generate(provider, &digest, settings.voice(), provider.timeout());
        drop(finished);
        roast
    })
    .map_err(|e| with_what_is_left(e, &store))?;
    // Sources stays usable during a Roast, so the demo this one is about can leave before it lands.
    // One that cannot be read here is no reason to lose the Roast, and the save below will say so.
    if from_demo && matches!(store.has_demo_data(), Ok(false)) {
        return Err(Error::Provider(
            "Your history changed while this Roast was being written, so Rostery did not keep it. \
             Press Roast me to write one about what is stored now."
                .to_owned(),
        ));
    }
    Ok(FinishedRoast {
        not_tagged,
        ..store.keep_roast(provider.label(), roast, now(), from_demo, SAVE_WAIT)
    })
}

fn tag_new_creators(
    provider: &Provider,
    store: &mut Store,
    progress: &impl Fn(&str),
) -> Result<u32, Error> {
    let both = universe::from_store(store, UniverseFilter::Both)?;
    if interests::next_to_tag(store, &both)?.is_empty() {
        return Ok(0);
    }
    progress(TAGGING);
    interests::tag_missing(provider, store, &both, provider.tag_timeout())
}

/// Only said when it is so, because a first Roast has no last one.
fn with_what_is_left(e: Error, store: &Store) -> Error {
    match e {
        Error::Provider(message) if matches!(store.latest_roast(), Ok(Some(_))) => {
            Error::Provider(format!("{message} Your last Roast is unchanged."))
        }
        e => e,
    }
}

#[tauri::command(async)]
pub fn generate_roast(app: AppHandle, state: State<'_, AppState>) -> Result<FinishedRoast, Error> {
    run_roast(&state, |phrase| {
        // Only the words on screen are lost if this fails, and the Roast goes on.
        let _ = app.emit("roast-progress", phrase);
    })
}

#[tauri::command(async)]
pub fn latest_roast(state: State<'_, AppState>) -> Result<Option<StoredRoast>, Error> {
    Store::open(&state.db_path)?.latest_roast()
}

/// The exact Digest a Roast would send right now, or nothing when there is no history to send.
#[tauri::command(async)]
pub fn digest_preview(state: State<'_, AppState>) -> Result<Option<String>, Error> {
    digest::from_store(&Store::open(&state.db_path)?, now())?.preview()
}

/// The exact names the next tag step would send, before the Digest goes.
#[tauri::command(async)]
pub fn creators_to_tag(state: State<'_, AppState>) -> Result<Vec<CreatorToTag>, Error> {
    let store = Store::open(&state.db_path)?;
    interests::next_to_tag(&store, &universe::from_store(&store, UniverseFilter::Both)?)
}

/// Spawns CLIs and calls the network, so it stays off the main thread and the screen does not wait on it.
#[tauri::command(async)]
pub fn provider_statuses(state: State<'_, AppState>) -> Result<Vec<ProviderStatus>, Error> {
    let settings = Settings::load(&Store::open(&state.db_path)?)?;
    Ok(llm::statuses(&settings))
}

/// Where the next Roast would go, from what is saved. It spawns nothing, so the Roast screen can have
/// it before it offers the button.
#[tauri::command(async)]
pub fn provider_disclosure(state: State<'_, AppState>) -> Result<Disclosure, Error> {
    let settings = Settings::load(&Store::open(&state.db_path)?)?;
    Ok(settings.disclosure())
}

#[tauri::command(async)]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, Error> {
    Settings::load(&Store::open(&state.db_path)?)
}

#[tauri::command(async)]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, Error> {
    settings.save(&Store::open(&state.db_path)?)
}

#[tauri::command]
pub fn default_system_prompt() -> &'static str {
    roast::default_system_prompt()
}

// Debug only, because a release build ignores the override and would call the real CLI.
/// The Share Site as `pnpm build` made it. `pnpm build` runs before any cargo build, CI's too.
const SHARE_TEMPLATE: [(&str, &[u8]); 3] = [
    ("index.html", include_bytes!("../../dist/share/index.html")),
    (
        "assets/share.js",
        include_bytes!("../../dist/share/assets/share.js"),
    ),
    (
        "assets/share.css",
        include_bytes!("../../dist/share/assets/share.css"),
    ),
];

/// Beside the database, in Rostery's own folder, because an export replaces everything in it.
fn site_dir(state: &AppState) -> PathBuf {
    state.db_path.with_file_name("site")
}

fn export_to(state: &AppState, opts: &ExportOptions) -> Result<ExportResult, Error> {
    let store = Store::open(&state.db_path)?;
    let payload = export::build_payload(&store, opts, now())?;
    let template: Vec<SiteFile> = SHARE_TEMPLATE
        .iter()
        .map(|(path, bytes)| SiteFile {
            path: (*path).to_owned(),
            bytes: bytes.to_vec(),
        })
        .collect();
    let files = export::render_site(&template, &payload)?;
    let dir = site_dir(state);
    export::write_site(&files, &dir)?;
    Ok(ExportResult {
        dir: dir.display().to_string(),
        bytes: files.iter().map(|f| f.bytes.len() as u64).sum(),
    })
}

#[tauri::command(async)]
pub fn export_site(state: State<'_, AppState>, opts: ExportOptions) -> Result<ExportResult, Error> {
    export_to(&state, &opts)
}

fn exported_index(state: &AppState) -> Result<PathBuf, Error> {
    let index = site_dir(state).join("index.html");
    if index.exists() {
        Ok(index)
    } else {
        Err(Error::Export(
            "There is no Share Site yet. Press Export, then try again.".into(),
        ))
    }
}

fn not_opened(e: impl std::fmt::Display) -> Error {
    // The opener's own words are about scopes and system calls, so they go to the log.
    eprintln!("Rostery could not open the Share Site: {e}");
    Error::Export(
        "Rostery could not open it. Check that you have a default browser set, then try again."
            .into(),
    )
}

#[tauri::command]
pub fn open_site_preview(app: AppHandle, state: State<'_, AppState>) -> Result<(), Error> {
    let index = exported_index(&state)?;
    app.opener()
        .open_path(index.to_string_lossy(), None::<&str>)
        .map_err(not_opened)
}

#[tauri::command]
pub fn reveal_site(app: AppHandle, state: State<'_, AppState>) -> Result<(), Error> {
    let index = exported_index(&state)?;
    app.opener().reveal_item_in_dir(index).map_err(not_opened)
}

const GITHUB_API: &str = "https://api.github.com";
const GITHUB_TOKEN_PAGE: &str =
    "https://github.com/settings/tokens/new?scopes=public_repo&description=Rostery";
const VERCEL_TOKEN_PAGE: &str = "https://vercel.com/account/settings/tokens";

/// Where a token goes, by the name the webview uses for it.
fn token_name(target: &str) -> Result<&'static str, Error> {
    match target {
        "github" => Ok("github_token"),
        "vercel" => Ok("vercel_token"),
        _ => Err(Error::Deploy(format!(
            "Rostery cannot publish to {target}."
        ))),
    }
}

/// The keychain first, then a logged-in `gh`, which is how GitHub connects with one click.
fn github_token(gh: impl FnOnce() -> Option<String>) -> Result<Option<(String, bool)>, Error> {
    if let Some(token) = secrets::get(token_name("github")?)? {
        return Ok(Some((token, false)));
    }
    Ok(gh().map(|token| (token, true)))
}

fn gh_auth_token() -> Option<String> {
    let path = llm::login_shell_path()?;
    let out = std::process::Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .env("PATH", path)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let token = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !token.is_empty()).then_some(token)
}

fn status_with(api: &str, gh: impl FnOnce() -> Option<String>) -> Result<DeployStatus, Error> {
    let github = match github_token(gh)? {
        Some((token, through_gh)) => {
            deploy::github::login(api, &token)?.map(|login| (login, through_gh))
        }
        None => None,
    };
    Ok(DeployStatus {
        github_through_gh: github.as_ref().is_some_and(|(_, through_gh)| *through_gh),
        github: github.map(|(login, _)| login),
        vercel: secrets::get(token_name("vercel")?)?.is_some(),
    })
}

#[tauri::command(async)]
pub fn deploy_status() -> Result<DeployStatus, Error> {
    status_with(GITHUB_API, gh_auth_token)
}

/// Checked before it is kept, so a typo is caught here and not at the first publish.
#[tauri::command(async)]
pub fn save_deploy_token(target: String, token: String) -> Result<(), Error> {
    let token = token.trim();
    if target == "github" && deploy::github::login(GITHUB_API, token)?.is_none() {
        return Err(Error::Deploy(
            "GitHub did not accept that token. Make a new one with the public_repo scope, then paste \
             it again."
                .into(),
        ));
    }
    secrets::set(token_name(&target)?, token)
}

#[tauri::command(async)]
pub fn forget_deploy_token(target: String) -> Result<(), Error> {
    secrets::delete(token_name(&target)?)
}

/// Fixed pages only, opened from here so the webview's opener stays limited to Source links.
#[tauri::command]
pub fn open_token_page(app: AppHandle, target: String) -> Result<(), Error> {
    let url = match target.as_str() {
        "github" => GITHUB_TOKEN_PAGE,
        "vercel" => VERCEL_TOKEN_PAGE,
        _ => {
            return Err(Error::Deploy(format!(
                "Rostery cannot publish to {target}."
            )))
        }
    };
    app.opener().open_url(url, None::<&str>).map_err(not_opened)
}

/// Exports fresh from `opts` first, so what goes out is what the Share screen shows.
#[tauri::command(async)]
pub fn deploy_github(
    state: State<'_, AppState>,
    repo: String,
    opts: ExportOptions,
) -> Result<DeployResult, Error> {
    let (token, _) = github_token(gh_auth_token)?
        .ok_or_else(|| Error::Deploy("Connect GitHub first, then publish.".into()))?;
    if !deploy::github::valid_repo_name(&repo) {
        return Err(Error::Deploy(format!(
            "\"{repo}\" cannot be a GitHub repo name. Use up to 100 letters, digits, dots, hyphens \
             and underscores."
        )));
    }
    export_to(&state, &opts)?;
    let files = deploy::read_site(&site_dir(&state))?;
    deploy::github::publish(GITHUB_API, &token, &repo, &files)
}

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
    const NO_TAGS: &str = r#"{"type":"result","is_error":false,"result":"{\"creators\":[]}"}"#;
    /// Two Creators on two Sources with a word only the Provider gave them, and one nobody asked about.
    const TWO_TAGGED: &str = r#"{"type":"result","is_error":false,"result":"{\"creators\":[{\"source\":\"spotify\",\"name\":\"Moth Season\",\"interests\":[\" Zither \"]},{\"source\":\"youtube\",\"name\":\"Otter Academy\",\"interests\":[\"zither\"]},{\"source\":\"spotify\",\"name\":\"Nobody Asked About This One\",\"interests\":[\"zither\"]}]}"}"#;
    const CAT_REPLY: &str = "cat \"$here/reply\"";
    /// One argument per line, in a file beside the fake.
    const RECORD_ARGV: &str = "printf '%s\\n' \"$@\" > \"$here/argv\"";

    /// These point the Provider at a fake `claude` through the environment, so one runs at a time.
    static ENV: Mutex<()> = Mutex::new(());

    struct Rig {
        dir: TempDir,
        state: AppState,
        _env: MutexGuard<'static, ()>,
    }

    impl Rig {
        /// A `claude` that answers the tag step by itself, from a file that says nobody has any tags
        /// until a test says otherwise. For a Roast it notes the call and what it was sent in files
        /// beside it, then runs `body`.
        fn new(body: &str) -> Rig {
            let env = ENV.lock().unwrap_or_else(|e| e.into_inner());
            let dir = TempDir::new().unwrap();
            let bin = dir.path().join("claude");
            std::fs::write(
                &bin,
                format!(
                    "#!/bin/sh\nhere=$(dirname \"$0\")\ninput=$(cat)\n\
                     case \"$input\" in *'<creators>'*)\n\
                       echo call >> \"$here/tag_calls\"\n\
                       printf '%s' \"$input\" > \"$here/tag_input\"\n\
                       while [ -e \"$here/tag_hold\" ]; do sleep 0.02; done\n\
                       cat \"$here/tag_reply\"\n\
                       exit 0 ;;\n\
                     esac\n\
                     echo call >> \"$here/calls\"\n\
                     printf '%s' \"$input\" > \"$here/input\"\n\
                     {body}\n"
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
            let rig = Rig {
                dir,
                state,
                _env: env,
            };
            rig.tag_reply(NO_TAGS);
            rig
        }

        fn tag_reply(&self, reply: &str) {
            std::fs::write(self.dir.path().join("tag_reply"), reply).unwrap();
        }

        /// A tag reply naming everyone the next tag step will ask about, each with no tags, which
        /// settles them all.
        fn answer_everyone(&self) {
            let store = Store::open(&self.state.db_path).unwrap();
            let both = universe::from_store(&store, UniverseFilter::Both).unwrap();
            let creators: Vec<serde_json::Value> = interests::next_to_tag(&store, &both)
                .unwrap()
                .into_iter()
                .map(|c| serde_json::json!({"source": c.source, "name": c.name, "interests": []}))
                .collect();
            let result = serde_json::json!({ "creators": creators }).to_string();
            self.tag_reply(
                &serde_json::json!({"type": "result", "is_error": false, "result": result})
                    .to_string(),
            );
        }

        fn tag_calls(&self) -> usize {
            std::fs::read_to_string(self.dir.path().join("tag_calls"))
                .map_or(0, |calls| calls.lines().count())
        }

        fn universe(&self, filter: UniverseFilter) -> Universe {
            universe::from_store(&Store::open(&self.state.db_path).unwrap(), filter).unwrap()
        }

        /// The same, with demo history stored and `reply` ready for the fake to give.
        fn with_history(body: &str, reply: &str) -> Rig {
            let rig = Rig::new(body);
            rig.reply(reply);
            demo::load(&mut Store::open(&rig.state.db_path).unwrap(), now()).unwrap();
            rig
        }

        /// Three Creators from an Export, weeks apart so that none of them share a Session, and with no
        /// Interests yet for a new one to be diluted by.
        fn with_real_history(reply: &str) -> Rig {
            let rig = Rig::new(CAT_REPLY);
            rig.reply(reply);
            let play = |source, creator: &str, weeks: i64, minutes: i64| Play {
                source,
                origin: Origin::Export,
                played_at: 1_700_000_000 + weeks * 604_800,
                duration_ms: Some(minutes * 60_000),
                creator: creator.into(),
                creator_ref: None,
                item: "A Song".into(),
                item_ref: None,
                skipped: false,
            };
            Store::open(&rig.state.db_path)
                .unwrap()
                .insert_plays(&[
                    play(Source::Spotify, "Moth Season", 0, 90),
                    play(Source::Youtube, "Otter Academy", 4, 60),
                    play(Source::Spotify, "Loner", 8, 1),
                ])
                .unwrap();
            rig
        }

        fn reply(&self, reply: &str) {
            std::fs::write(self.dir.path().join("reply"), reply).unwrap();
        }

        fn calls(&self) -> usize {
            std::fs::read_to_string(self.dir.path().join("calls"))
                .map_or(0, |calls| calls.lines().count())
        }

        fn save(&self, settings: Settings) {
            settings
                .save(&Store::open(&self.state.db_path).unwrap())
                .unwrap();
        }

        fn argv(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("argv")).unwrap()
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
        .unwrap()
        .roast;

        assert_eq!(saved.roast.headline, "Heard");
        assert_eq!(saved.provider_label, "Claude Code");
        assert!(saved.created_at > 0);
        assert_eq!(rig.latest(), Some(saved));
        let said = said.into_inner().unwrap();
        assert_eq!(said[0], "Reading your history");
        assert_eq!(said[1], TAGGING);
        assert_eq!(said[2], WAITING[0]);
    }

    #[test]
    fn a_roast_with_nobody_left_to_tag_says_nothing_about_tagging() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        rig.answer_everyone();
        assert_eq!(run_roast(&rig.state, silent).unwrap().not_tagged, None);
        assert_eq!(rig.tag_calls(), 1);

        let said = Mutex::new(Vec::new());
        run_roast(&rig.state, |phrase| {
            said.lock().unwrap().push(phrase.to_owned())
        })
        .unwrap();

        assert_eq!(rig.tag_calls(), 1, "nobody new to ask about");
        assert!(!said.lock().unwrap().iter().any(|phrase| phrase == TAGGING));
    }

    #[test]
    fn tagging_failure_does_not_stop_the_roast() {
        for reply in [
            REFUSED,
            NO_TAGS,
            "not even JSON",
            r#"{"type":"result","is_error":false,"result":"no Interests here"}"#,
        ] {
            let rig = Rig::with_history(CAT_REPLY, REPLY);
            rig.tag_reply(reply);

            let finished = run_roast(&rig.state, silent).unwrap();

            assert_eq!(finished.roast.roast.headline, "Heard", "{reply}");
            let note = finished.not_tagged.expect(reply);
            assert!(note.contains("could not tag"), "{note}");
            assert_eq!(rig.tag_calls(), 1, "{reply}");
            assert_eq!(rig.calls(), 1, "{reply}");

            // A failed call settled nobody, so the next Roast asks again, and one that works sticks.
            rig.answer_everyone();
            run_roast(&rig.state, silent).unwrap();
            assert_eq!(rig.tag_calls(), 2, "{reply}");
            run_roast(&rig.state, silent).unwrap();
            assert_eq!(rig.tag_calls(), 2, "{reply}");
        }
    }

    #[test]
    fn a_roast_tags_first_and_its_digest_holds_the_new_interests() {
        let rig = Rig::with_real_history(REPLY);
        rig.tag_reply(TWO_TAGGED);
        run_roast(&rig.state, silent).unwrap();

        let sent = std::fs::read_to_string(rig.dir.path().join("input")).unwrap();
        assert!(sent.starts_with("<digest>"), "{sent}");
        assert!(
            sent.contains("zither"),
            "the Digest came after the tags: {sent}"
        );
        assert!(!sent.contains("Nobody Asked"), "{sent}");

        let names = std::fs::read_to_string(rig.dir.path().join("tag_input")).unwrap();
        assert!(names.contains("Moth Season"), "{names}");
        // Names and Sources, and no Digest or anything else to go with them.
        assert!(!names.contains("<digest>"), "{names}");
        assert!(!names.to_lowercase().contains("minutes"), "{names}");
    }

    /// Stop point 3 looks at this with a real Provider. These are the two things it shows.
    #[test]
    fn after_a_roast_there_are_new_links_and_galaxies_have_their_names() {
        let with_names = r#"{"type":"result","is_error":false,"result":"{\"headline\":\"Heard\",\"opening\":\"You.\",\"burns\":[{\"title\":\"Burn\",\"body\":\"Ouch.\"}],\"verdict\":{\"archetype\":\"Night Clerk\",\"score\":40,\"sentence\":\"Sit down.\"},\"galaxy_names\":[{\"id\":0,\"name\":\"The First Sky\"},{\"id\":99,\"name\":\"Long Gone\"},{\"id\":1,\"name\":\"  \"}]}"}"#;
        let rig = Rig::with_real_history(with_names);
        rig.tag_reply(TWO_TAGGED);
        let before = rig.universe(UniverseFilter::Both);
        assert!(before.links.is_empty());
        assert_eq!(before.galaxies.len(), 3);

        run_roast(&rig.state, silent).unwrap();

        // A Link for what the Provider said, between two who never shared a Session.
        let after = rig.universe(UniverseFilter::Both);
        assert_eq!(after.links.len(), 1, "{:?}", after.links);
        assert_eq!(after.links[0].shared, ["zither"]);
        assert_eq!(after.galaxies.len(), 2);
        let galaxy_of = |u: &Universe, name: &str| {
            let planet = u.planets.iter().find(|p| p.name == name).unwrap();
            u.galaxies[planet.galaxy as usize].name.clone()
        };
        assert_eq!(galaxy_of(&after, "Moth Season"), "The First Sky");
        assert_eq!(galaxy_of(&after, "Otter Academy"), "The First Sky");
        // A name that was blank keeps the default, and one for no Galaxy goes nowhere.
        assert_eq!(galaxy_of(&after, "Loner"), "Loner");

        // The Share Site's two views have Galaxy ids of their own, and go by their members.
        let artists = rig.universe(UniverseFilter::Spotify);
        assert_eq!(galaxy_of(&artists, "Moth Season"), "The First Sky");
        assert_eq!(galaxy_of(&artists, "Loner"), "Loner");
        let channels = rig.universe(UniverseFilter::Youtube);
        assert_eq!(galaxy_of(&channels, "Otter Academy"), "The First Sky");
    }

    /// The Provider takes a while, and Sources stays usable while it does.
    #[test]
    fn a_demo_that_leaves_during_the_tag_step_leaves_no_tags_and_is_not_roasted() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        rig.tag_reply(TWO_TAGGED);
        let hold = rig.dir.path().join("tag_hold");
        std::fs::write(&hold, "").unwrap();

        let result = std::thread::scope(|scope| {
            let run = scope.spawn(|| run_roast(&rig.state, silent));
            let until = Instant::now() + Duration::from_secs(10);
            while rig.tag_calls() == 0 && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(rig.tag_calls(), 1, "the tag step never began");

            demo::clear(&mut Store::open(&rig.state.db_path).unwrap()).unwrap();
            std::fs::remove_file(&hold).unwrap();
            run.join().unwrap()
        });

        let message = result.unwrap_err().to_string();
        assert!(message.contains("nothing to roast yet"), "{message}");
        assert_eq!(rig.calls(), 0);
        let store = Store::open(&rig.state.db_path).unwrap();
        assert!(store.interests().unwrap().is_empty());
        assert!(store.ai_asked().unwrap().is_empty());
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
            assert_eq!(first.join().unwrap().unwrap().roast.roast.headline, "Heard");
        });

        // It is over, so another may start.
        run_roast(&rig.state, silent).unwrap();
        assert_eq!(rig.calls(), 2);
    }

    #[test]
    fn a_failed_roast_keeps_the_last_one() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        let saved = run_roast(&rig.state, silent).unwrap().roast;

        rig.reply(REFUSED);
        let message = run_roast(&rig.state, silent).unwrap_err().to_string();

        assert!(message.contains("Usage limit reached"), "{message}");
        assert!(
            message.contains("Your last Roast is unchanged"),
            "{message}"
        );
        assert_eq!(rig.latest(), Some(saved));
    }

    #[test]
    fn a_first_roast_that_fails_says_nothing_of_a_last_one() {
        for reply in [
            REFUSED,
            r#"{"type":"result","is_error":false,"result":"no JSON here"}"#,
        ] {
            let rig = Rig::with_history(CAT_REPLY, reply);
            let message = run_roast(&rig.state, silent).unwrap_err().to_string();
            assert!(!message.contains("last Roast"), "{message}");
            assert_eq!(rig.latest(), None);
        }
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
        let saved = run_roast(&rig.state, silent).unwrap().roast;

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
        assert_eq!(rig.tag_calls(), 0);
        assert_eq!(rig.latest(), None);
    }

    #[test]
    fn a_roast_uses_the_provider_and_voice_in_settings_and_says_so() {
        let rig = Rig::with_history(&format!("{RECORD_ARGV}\n{CAT_REPLY}"), REPLY);
        rig.save(Settings {
            provider: Provider::ClaudeCode {
                model: Some("haiku".into()),
            },
            system_prompt: Some("Speak only of soup.".into()),
            ..Settings::default()
        });

        let saved = run_roast(&rig.state, silent).unwrap().roast;

        assert_eq!(saved.provider_label, "Claude Code");
        let argv = rig.argv();
        assert!(argv.contains("haiku"), "{argv}");
        assert!(argv.contains("Speak only of soup."), "{argv}");
        assert!(
            argv.contains("galaxy_names"),
            "the fixed part is still there: {argv}"
        );
    }

    #[test]
    fn a_roast_goes_on_with_what_it_started_with_when_settings_change() {
        let rig = Rig::with_history(
            &format!(
                "{RECORD_ARGV}\n\
                 touch \"$here/started\"\n\
                 while [ ! -e \"$here/go\" ]; do sleep 0.02; done\n\
                 cat \"$here/reply\""
            ),
            REPLY,
        );
        rig.save(Settings {
            system_prompt: Some("The first voice.".into()),
            ..Settings::default()
        });
        let (started, go) = (rig.dir.path().join("started"), rig.dir.path().join("go"));

        std::thread::scope(|scope| {
            let run = scope.spawn(|| run_roast(&rig.state, silent));
            let until = Instant::now() + Duration::from_secs(10);
            while !started.exists() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(started.exists(), "the Roast never began");

            rig.save(Settings {
                provider: Provider::Codex { model: None },
                system_prompt: Some("The second voice.".into()),
                ..Settings::default()
            });
            std::fs::write(&go, "").unwrap();

            let saved = run.join().unwrap().unwrap().roast;
            assert_eq!(saved.provider_label, "Claude Code");
        });
        let argv = rig.argv();
        assert!(argv.contains("The first voice."), "{argv}");
        assert!(!argv.contains("The second voice."), "{argv}");
    }

    #[test]
    fn settings_that_cannot_be_read_send_no_digest_until_a_provider_is_chosen() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        Store::open(&rig.state.db_path)
            .unwrap()
            .set_setting("settings", &"from the future")
            .unwrap();

        let refused = run_roast(&rig.state, silent).err().unwrap().to_string();
        assert!(refused.contains("could not read"), "{refused}");
        assert!(refused.contains("choose one in Settings"), "{refused}");
        assert_eq!(rig.calls(), 0);
        assert_eq!(rig.tag_calls(), 0, "the names go nowhere either");
        assert!(rig.latest().is_none());

        rig.save(Settings::default());
        run_roast(&rig.state, silent).unwrap();
        assert_eq!(rig.calls(), 1);
    }

    #[test]
    fn settings_that_are_refused_change_nothing_a_roast_reads() {
        let rig = Rig::new(CAT_REPLY);
        let store = Store::open(&rig.state.db_path).unwrap();
        let before = Settings::load(&store).unwrap();
        let refused = Settings {
            system_prompt: Some("x".repeat(llm::MAX_SYSTEM_PROMPT_CHARS + 1)),
            ..Settings::default()
        };
        assert!(refused.save(&store).is_err());
        assert_eq!(Settings::load(&store).unwrap(), before);
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

    #[test]
    fn export_writes_the_site_beside_the_database_and_replaces_the_last_one() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        let dir = rig.dir.path().join("site");
        std::fs::create_dir_all(dir.join("old")).unwrap();
        std::fs::write(dir.join("old/stale.html"), "left out since").unwrap();

        let result = export_to(&rig.state, &ExportOptions::default()).unwrap();

        assert_eq!(result.dir, dir.display().to_string());
        assert!(result.bytes > 0);
        let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(html.contains("rostery-data") && !html.contains("__ROSTERY_DATA__"));
        assert!(dir.join("assets/share.js").exists());
        assert!(!dir.join("old").exists());
        assert!(exported_index(&rig.state).is_ok());
    }

    #[test]
    fn preview_before_any_export_says_what_to_do() {
        let rig = Rig::with_history(CAT_REPLY, REPLY);
        let message = exported_index(&rig.state).unwrap_err().to_string();
        assert!(message.contains("Press Export"), "{message}");
    }
}
