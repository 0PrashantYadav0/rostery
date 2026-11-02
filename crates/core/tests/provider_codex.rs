//! The real `llm::complete` and `llm::statuses` against a fake `codex` that records what it was given.
//! Release builds ignore the binary override, so there these would talk to the real CLI.
#![cfg(debug_assertions)]

use rostery_core::demo;
use rostery_core::digest::{self, Digest};
use rostery_core::interests;
use rostery_core::llm::{self, CompletionRequest, LocalServer, Provider, ProviderStatus, Settings};
use rostery_core::roast;
use rostery_core::store::Store;
use rostery_core::universe::{self, UniverseFilter};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SYSTEM: &str = "You are a very mean comedian.\nHe said \"sit down\" and C:\\ stayed.";
const USER: &str = "<digest>{\"secret\":\"only on stdin\"}</digest>";
const OK: &str = r#"printf 'hello from the file' > "$out"; echo 'not the reply'"#;
const LONG_ENOUGH: Duration = Duration::from_secs(20);

/// Everything here changes this process's environment, so one test runs at a time.
static ENV: Mutex<()> = Mutex::new(());

struct Fake {
    dir: TempDir,
    _env: MutexGuard<'static, ()>,
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

impl Fake {
    /// A `codex` that records its argv, stdin, cwd, the listing of its cwd, its environment and the
    /// schema it was given, works out where it should write its reply, then runs `body`.
    fn new(body: &str) -> Fake {
        let env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = TempDir::new().unwrap();
        let record = dir.path().display();
        write_script(
            &dir.path().join("codex"),
            &format!(
                "printf '%s\\0' \"$@\" > '{record}/argv'\n\
                 cat > '{record}/stdin'\n\
                 pwd -P > '{record}/cwd'\n\
                 ls -A > '{record}/listing'\n\
                 env > '{record}/env'\n\
                 prev=''\n\
                 for a in \"$@\"; do\n\
                   case \"$prev\" in -o) out=$a ;; --output-schema) cp \"$a\" '{record}/schema' ;; esac\n\
                   prev=$a\n\
                 done\n\
                 {body}"
            ),
        );
        // No test here should depend on the rc files of whoever runs it.
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("ROSTERY_CODEX_BIN", dir.path().join("codex"));
        Fake { dir, _env: env }
    }

    fn ask(&self, timeout: Duration) -> rostery_core::Result<String> {
        llm::complete(&Provider::Codex { model: None }, &request(), timeout)
    }

    fn recorded(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(name)).unwrap()
    }

    fn argv(&self) -> Vec<String> {
        let raw = self.recorded("argv");
        raw.strip_suffix('\0')
            .unwrap()
            .split('\0')
            .map(String::from)
            .collect()
    }

    /// The value after a flag.
    fn after(&self, flag: &str) -> String {
        let argv = self.argv();
        let at = argv.iter().position(|a| a == flag).unwrap();
        argv[at + 1].clone()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        std::env::remove_var("ROSTERY_CODEX_BIN");
        std::env::remove_var("ROSTERY_CLAUDE_BIN");
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        system: SYSTEM.into(),
        user: USER.into(),
        schema: json!({"type": "object"}),
    }
}

fn demo_digest() -> Digest {
    const NOW: i64 = 1_800_000_000;
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, NOW).unwrap();
    digest::from_store(&store, NOW).unwrap()
}

/// The arguments in a run of `-c` overrides, one `key=value` each.
fn overrides(argv: &[String]) -> Vec<&str> {
    argv.windows(2)
        .filter(|pair| pair[0] == "-c")
        .map(|pair| pair[1].as_str())
        .collect()
}

#[test]
fn codex_runs_read_only() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    let argv = fake.argv();
    assert_eq!(argv[0], "exec");
    for flag in [
        "--ephemeral",
        "--ignore-rules",
        "--ignore-user-config",
        "--skip-git-repo-check",
        "--output-schema",
        "-o",
        "-C",
    ] {
        assert!(
            argv.iter().any(|a| a == flag),
            "{flag} is missing: {argv:?}"
        );
    }
    assert_eq!(fake.after("--sandbox"), "read-only");
    assert_eq!(argv.last().unwrap(), "-", "the prompt is read from stdin");
    assert!(
        overrides(&argv)
            .iter()
            .any(|o| o.starts_with("developer_instructions=")),
        "{argv:?}"
    );
}

/// Each of these was tried against codex-cli 0.164.0. ADR 0003 has what is left.
#[test]
fn codex_has_its_shell_web_apps_and_image_tools_turned_off() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    let argv = fake.argv();
    let given = overrides(&argv);
    for off in [
        "features.shell_tool=false",
        "web_search=\"disabled\"",
        "features.apps=false",
        "features.image_generation=false",
        "features.view_image=false",
        "features.multi_agent=false",
        "features.goals=false",
        "skills.include_instructions=false",
        "include_environment_context=false",
    ] {
        assert!(given.contains(&off), "{off} is missing: {given:?}");
    }
}

/// With colour forced from the environment, a logged out Codex's `ERROR:` line starts with escape
/// codes, nothing matches it, and the person gets "exit code 1" and no way to log in.
#[test]
fn codex_is_not_allowed_to_colour_its_errors() {
    let fake = Fake::new(OK);
    let forced = ["FORCE_COLOR", "CLICOLOR_FORCE"];
    for key in forced {
        std::env::set_var(key, "1");
    }
    let reply = fake.ask(LONG_ENOUGH);
    for key in forced {
        std::env::remove_var(key);
    }
    reply.unwrap();
    let env = fake.recorded("env");
    for key in forced {
        assert!(!env.contains(&format!("{key}=")), "{key} in {env}");
    }
    assert!(env.lines().any(|line| line == "NO_COLOR=1"), "{env}");
}

#[test]
fn codex_gets_the_system_prompt_as_a_toml_string() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    let argv = fake.argv();
    let given = overrides(&argv);
    let instructions = given
        .iter()
        .find_map(|o| o.strip_prefix("developer_instructions="))
        .unwrap();
    assert_eq!(
        instructions,
        r#""You are a very mean comedian.\nHe said \"sit down\" and C:\\ stayed.""#
    );
}

#[test]
fn a_character_toml_will_not_take_bare_is_escaped() {
    let fake = Fake::new(OK);
    let mut req = request();
    req.system = "a\u{7f}b\u{1}c".into();
    llm::complete(&Provider::Codex { model: None }, &req, LONG_ENOUGH).unwrap();
    let argv = fake.argv();
    let given = overrides(&argv);
    assert!(
        given.contains(&r#"developer_instructions="a\u007fb\u0001c""#),
        "{given:?}"
    );
}

#[test]
fn codex_gets_the_model_when_one_is_set() {
    let fake = Fake::new(OK);
    llm::complete(
        &Provider::Codex {
            model: Some("gpt-test".into()),
        },
        &request(),
        LONG_ENOUGH,
    )
    .unwrap();
    assert_eq!(fake.after("-m"), "gpt-test");
    assert_eq!(fake.argv().last().unwrap(), "-");
}

#[test]
fn codex_reads_prompt_from_stdin() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    assert_eq!(fake.recorded("stdin"), USER);
    assert!(!fake.argv().iter().any(|arg| arg.contains("only on stdin")));
}

#[test]
fn codex_runs_in_an_empty_temp_dir_that_it_is_also_pointed_at() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    assert_eq!(fake.recorded("listing"), "");
    let cwd = fake.recorded("cwd");
    let told = Path::new(&fake.after("-C")).to_path_buf();
    let cwd = Path::new(cwd.trim());
    // The temp folder can be a symlink, and `pwd -P` is not.
    assert!(cwd.ends_with(told.file_name().unwrap()), "{cwd:?} {told:?}");
    assert!(!cwd.exists(), "the empty folder is cleaned up afterwards");
    let out = fake.after("-o");
    assert!(!Path::new(&out).starts_with(&told), "{out}");
}

#[test]
fn codex_child_has_no_api_key() {
    let fake = Fake::new(OK);
    let stray = [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "ANTHROPIC_API_KEY",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
    ];
    for key in stray {
        std::env::set_var(key, "stray");
    }
    std::env::set_var("ROSTERY_KEEPS_THIS", "yes");
    let reply = fake.ask(LONG_ENOUGH);
    for key in stray {
        std::env::remove_var(key);
    }
    std::env::remove_var("ROSTERY_KEEPS_THIS");
    reply.unwrap();
    let env = fake.recorded("env");
    for key in stray {
        assert!(!env.contains(&format!("{key}=")), "{key} in {env}");
    }
    assert!(
        env.contains("ROSTERY_KEEPS_THIS=yes"),
        "the rest of the environment stays"
    );
}

#[test]
fn codex_reads_output_file() {
    let fake = Fake::new(OK);
    assert_eq!(fake.ask(LONG_ENOUGH).unwrap(), "hello from the file");
}

/// Every object, however deep, the way `codex exec --output-schema` demands it.
fn assert_strict(schema: &Value, path: &str) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        assert_eq!(
            schema["additionalProperties"],
            json!(false),
            "{path} allows extra properties"
        );
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{path} has no required"))
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        for key in properties.keys() {
            assert!(
                required.contains(&key.as_str()),
                "{path}.{key} not required"
            );
        }
        for (key, value) in properties {
            assert_strict(value, &format!("{path}.{key}"));
        }
    }
    if let Some(items) = schema.get("items") {
        assert_strict(items, &format!("{path}[]"));
    }
}

const A_ROAST: &str = r#"{"headline":"H","opening":"O","burns":[{"title":"T","body":"B"}],"verdict":{"archetype":"A","score":1,"sentence":"S","evidence":[]},"galaxy_names":[]}"#;

#[test]
fn codex_schema_is_strict() {
    let fake = Fake::new(&format!("printf '%s' '{A_ROAST}' > \"$out\""));
    let roast = roast::generate(
        &Provider::Codex { model: None },
        &demo_digest(),
        "Be mean",
        LONG_ENOUGH,
    )
    .unwrap();
    assert_eq!(roast.headline, "H");

    let schema: Value = serde_json::from_str(&fake.recorded("schema")).unwrap();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["verdict"]["properties"]["score"].is_object());
    assert_strict(&schema, "roast");
}

#[test]
fn codex_schema_for_interests_is_strict() {
    let fake = Fake::new(r#"printf '%s' '{"creators":[]}' > "$out""#);
    const NOW: i64 = 1_800_000_000;
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, NOW).unwrap();
    let both = universe::from_store(&store, UniverseFilter::Both).unwrap();

    interests::tag_missing(
        &Provider::Codex { model: None },
        &mut store,
        &both,
        LONG_ENOUGH,
    )
    .unwrap();

    let schema: Value = serde_json::from_str(&fake.recorded("schema")).unwrap();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["creators"]["items"]["properties"]["interests"].is_object());
    assert_strict(&schema, "interests");
    assert!(fake.recorded("stdin").starts_with("<creators>"));
}

/// What codex-cli 0.164.0 really prints with nobody logged in: the prompt, then each try at connecting.
const LOGGED_OUT: &str = "cat >&2 <<'EOF'\n\
OpenAI Codex v0.164.0\n\
--------\n\
workdir: /tmp/x\n\
--------\n\
user\n\
{\"secret\":\"on stderr\"}\n\
2026-11-07T06:40:09Z ERROR codex_api::endpoint: failed to connect to websocket: HTTP error: 401 Unauthorized\n\
ERROR: Reconnecting... 5/5\n\
ERROR: unexpected status 401 Unauthorized: Missing bearer or basic authentication in header\n\
EOF\n\
exit 1";

#[test]
fn logged_out_says_how_to_log_in() {
    let fake = Fake::new(LOGGED_OUT);
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("not logged in"), "{message}");
    assert!(message.contains("codex login"), "{message}");
    assert!(!message.contains('`'), "{message}");
}

/// Codex echoes the whole prompt on stderr, so the first lines of it would put the Digest on screen.
#[test]
fn an_error_is_its_last_error_line_and_never_the_prompt() {
    let fake = Fake::new(
        "printf 'OpenAI Codex v0.164.0\\nuser\\n{\"secret\":\"on stderr\"}\\n' >&2\n\
         printf 'ERROR: You have hit your usage limit\\n' >&2\n\
         exit 1",
    );
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("hit your usage limit"), "{message}");
    assert!(!message.contains("on stderr"), "{message}");
    assert!(message.contains("try again"), "{message}");
}

#[test]
fn a_failure_with_no_error_line_says_the_exit_code_and_not_the_prompt() {
    let fake = Fake::new("printf 'user\\n{\"secret\":\"on stderr\"}\\n' >&2; exit 3");
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("exit code 3"), "{message}");
    assert!(!message.contains("on stderr"), "{message}");
}

#[test]
fn a_cli_too_old_for_these_flags_says_to_update() {
    let fake = Fake::new(
        "echo \"error: unexpected argument '--ignore-user-config' found\" >&2\n\
         echo >&2\n\
         echo \"Usage: codex exec [OPTIONS] [PROMPT]\" >&2\n\
         exit 2",
    );
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("too old"), "{message}");
    assert!(message.contains("codex update"), "{message}");
}

#[test]
fn no_output_file_says_what_to_do() {
    for body in [":", r#": > "$out""#, r#"printf ' \n' > "$out""#] {
        let fake = Fake::new(body);
        let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
        assert!(message.contains("could not read"), "{message}");
        assert!(message.contains("codex update"), "{message}");
    }
}

#[test]
fn missing_binary_says_how_to_install() {
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CODEX_BIN", fake.dir.path().join("not-here"));
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("could not find Codex"), "{message}");
    assert!(message.contains("Install"), "{message}");
}

fn gone(pid: &str) -> bool {
    !Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn timeout_kills_child() {
    let fake = Fake::new("echo $$ > \"$(dirname \"$0\")/pid\"\nsleep 60 &\nwait");
    let started = Instant::now();
    let message = fake.ask(Duration::from_secs(1)).unwrap_err().to_string();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(message.contains("Codex did not finish"), "{message}");
    let until = Instant::now() + Duration::from_secs(3);
    let pid = fake.recorded("pid");
    while !gone(&pid) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gone(&pid), "the CLI is still running");
}

/// Both fakes answer only to the question they should be asked, and nothing is listening locally.
fn statuses_with(fake: &Fake, claude_body: &str, codex_body: &str) -> Vec<ProviderStatus> {
    let claude = fake.dir.path().join("claude");
    write_script(
        &claude,
        &format!(
            "[ \"$1 $2\" = 'auth status' ] || {{ echo \"asked: $*\" >&2; exit 64; }}\n{claude_body}"
        ),
    );
    let codex = fake.dir.path().join("codex-status");
    write_script(
        &codex,
        &format!(
            "[ \"$1 $2\" = 'login status' ] || {{ echo \"asked: $*\" >&2; exit 64; }}\n{codex_body}"
        ),
    );
    std::env::set_var("ROSTERY_CLAUDE_BIN", &claude);
    std::env::set_var("ROSTERY_CODEX_BIN", &codex);
    llm::statuses(&settings_with_nothing_listening())
}

fn settings_with_nothing_listening() -> Settings {
    Settings {
        provider: Provider::ClaudeCode { model: None },
        system_prompt: None,
        others: vec![
            Provider::Codex { model: None },
            Provider::Local {
                server: LocalServer::Ollama,
                base_url: "http://127.0.0.1:1/v1".into(),
                model: String::new(),
                api_key: None,
            },
            Provider::Local {
                server: LocalServer::LmStudio,
                base_url: "http://127.0.0.1:1/v1".into(),
                model: String::new(),
                api_key: None,
            },
        ],
        choose_again: false,
    }
}

fn status<'a>(all: &'a [ProviderStatus], id: &str) -> &'a ProviderStatus {
    all.iter().find(|s| s.id == id).unwrap()
}

#[test]
fn statuses_detect_cli_login() {
    let fake = Fake::new(OK);
    let all = statuses_with(
        &fake,
        r#"printf '{"loggedIn":true}'"#,
        "echo 'Logged in using ChatGPT'",
    );
    let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["claude-code", "codex", "ollama", "lm-studio"]);
    for id in ["claude-code", "codex"] {
        let s = status(&all, id);
        assert!(s.available && s.signed_in, "{s:?}");
    }
    assert!(status(&all, "claude-code").sends_to.contains("Anthropic"));
    assert!(status(&all, "codex").sends_to.contains("OpenAI"));

    let all = statuses_with(
        &fake,
        r#"printf '{"loggedIn":false,"authMethod":"none"}'; exit 1"#,
        "echo 'Not logged in'; exit 1",
    );
    let claude = status(&all, "claude-code");
    assert!(claude.available && !claude.signed_in, "{claude:?}");
    assert!(claude.detail.contains("/login"), "{claude:?}");
    let codex = status(&all, "codex");
    assert!(codex.available && !codex.signed_in, "{codex:?}");
    assert!(codex.detail.contains("codex login"), "{codex:?}");
    for s in [claude, codex] {
        assert!(s.detail.contains("check again"), "{s:?}");
    }
}

/// The words on each row have to be about that row's saved model, chosen or not.
#[test]
fn a_status_says_where_the_model_saved_on_that_row_would_send_a_digest() {
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("no-claude"));
    std::env::set_var("ROSTERY_CODEX_BIN", fake.dir.path().join("no-codex"));
    let mut settings = settings_with_nothing_listening();
    let Provider::Local { model, .. } = &mut settings.others[1] else {
        panic!("the Ollama row")
    };
    *model = "gpt-oss:20b-cloud".into();

    let all = llm::statuses(&settings);

    let cloud = status(&all, "ollama");
    assert!(cloud.sends_to.starts_with("Warning"), "{cloud:?}");
    assert!(cloud.sends_to.contains("ollama.com"), "{cloud:?}");
    assert!(status(&all, "lm-studio").sends_to.contains("stays on"));
}

/// `127.1` reaches the same port on this machine, and does not count as this computer, which makes it a
/// way to see whether a server elsewhere gets called. A saved address can be a device on someone else's
/// network by now, and just opening Settings should not call it.
#[test]
fn a_server_elsewhere_is_only_asked_once_it_is_the_chosen_provider() {
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("no-claude"));
    std::env::set_var("ROSTERY_CODEX_BIN", fake.dir.path().join("no-codex"));
    let mut server = mockito::Server::new();
    let elsewhere = Provider::Local {
        server: LocalServer::Ollama,
        base_url: format!("http://127.1:{}/v1", server.socket_address().port()),
        model: "tiny".into(),
        api_key: None,
    };

    // Not chosen, and with nothing chosen at all.
    let never = server.mock("GET", "/v1/models").expect(0).create();
    let unchosen = Settings {
        others: vec![elsewhere.clone()],
        ..Settings::default()
    };
    let lost = Settings {
        provider: elsewhere.clone(),
        choose_again: true,
        ..Settings::default()
    };
    for settings in [unchosen, lost] {
        let all = llm::statuses(&settings);
        let row = status(&all, "ollama");
        assert!(!row.available, "{row:?}");
        assert!(row.models.is_empty(), "{row:?}");
        assert!(row.detail.starts_with("Not asked"), "{row:?}");
        assert!(row.detail.ends_with('.'), "{row:?}");
        assert!(row.sends_to.starts_with("Warning"), "{row:?}");
    }
    never.assert();
    never.remove();

    let asked = server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"tiny"}]}"#)
        .create();
    let all = llm::statuses(&Settings {
        provider: elsewhere,
        ..Settings::default()
    });
    let row = status(&all, "ollama");
    assert!(row.available, "{row:?}");
    assert_eq!(row.models, ["tiny"]);
    asked.assert();
}

/// One on this computer is still asked, chosen or not, because nothing leaves for that.
#[test]
fn a_server_on_this_computer_is_asked_whether_or_not_it_is_chosen() {
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("no-claude"));
    std::env::set_var("ROSTERY_CODEX_BIN", fake.dir.path().join("no-codex"));
    let mut server = mockito::Server::new();
    let asked = server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"tiny"}]}"#)
        .create();
    let all = llm::statuses(&Settings {
        others: vec![Provider::Local {
            server: LocalServer::LmStudio,
            base_url: format!("{}/v1", server.url()),
            model: String::new(),
            api_key: None,
        }],
        ..Settings::default()
    });
    assert_eq!(status(&all, "lm-studio").models, ["tiny"]);
    asked.assert();
}

#[test]
fn a_cli_saying_exit_0_without_logged_in_is_not_taken_at_its_word() {
    let fake = Fake::new(OK);
    let all = statuses_with(&fake, r#"printf '{"loggedIn":false}'"#, "exit 0");
    let claude = status(&all, "claude-code");
    assert!(!claude.signed_in, "{claude:?}");
}

#[test]
fn a_cli_not_installed_says_how_to_install_and_a_too_old_one_how_to_update() {
    let fake = Fake::new(OK);
    let settings = settings_with_nothing_listening();
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("no-claude"));
    std::env::set_var("ROSTERY_CODEX_BIN", fake.dir.path().join("no-codex"));
    let all = llm::statuses(&settings);
    for (id, place) in [
        ("claude-code", "claude.com/claude-code"),
        ("codex", "github.com/openai/codex"),
    ] {
        let s = status(&all, id);
        assert!(!s.available && !s.signed_in, "{s:?}");
        assert!(s.detail.contains(place), "{s:?}");
    }

    let all = statuses_with(
        &fake,
        "echo \"error: unknown command 'auth'\" >&2; exit 1",
        "exit 0",
    );
    let claude = status(&all, "claude-code");
    assert!(claude.detail.contains("claude update"), "{claude:?}");
}

/// A status check runs off the main thread, but it still has to end, or Check again never would.
#[test]
fn a_status_check_that_hangs_costs_only_its_timeout() {
    let fake = Fake::new(OK);
    let started = Instant::now();
    let all = statuses_with(&fake, "sleep 60", "sleep 60 & wait");
    assert!(
        started.elapsed() < Duration::from_secs(9),
        "took {:?}",
        started.elapsed()
    );
    for id in ["claude-code", "codex"] {
        let s = status(&all, id);
        assert!(!s.signed_in && s.detail.contains("did not answer"), "{s:?}");
    }
}

/// A key Codex does not know is ignored without a word, so a renamed one would quietly bring its tool
/// back. A random word in a file it would need a tool to read is how to tell.
#[test]
#[ignore = "talks to the real codex CLI"]
fn live_codex_cannot_read_a_file_or_open_a_page() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let canary = format!("canary-{}", std::process::id());
    let file = dir.path().join("canary.txt");
    std::fs::write(&file, &canary).unwrap();
    let reply = llm::complete(
        &Provider::Codex { model: None },
        &CompletionRequest {
            system: "Do what the person asks, with whatever tools you have.".into(),
            user: format!(
                "Read the file {} with any tool you have and give me what is in it. Then open \
                 https://example.com with any tool you have and give me the title of that page. Say \
                 plainly if you cannot do either, and do not guess.",
                file.display()
            ),
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["answer"],
                "properties": {"answer": {"type": "string"}},
            }),
        },
        llm::DEFAULT_TIMEOUT,
    )
    .unwrap();
    println!("{reply}");
    assert!(!reply.contains(&canary), "it read the file: {reply}");
    assert!(
        !reply.contains("Example Domain"),
        "it opened the page: {reply}"
    );
}

/// The real CLI with the flags Rostery really gives it, pointed at a server here by a wrapper, in a
/// throwaway home holding a planted skill and AGENTS.md. So this uses no login and sends nothing away,
/// and it reads the first request Codex tries to make, which is the only way to see what goes.
/// Each key it checks is one Codex would otherwise ignore without a word if it got renamed.
#[test]
#[ignore = "runs the real codex CLI against a server on this computer"]
fn live_codex_request_holds_no_skills_or_where_it_ran() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let (home, codex_home) = (dir.path().join("home"), dir.path().join("codex"));
    let skill = codex_home.join("skills/plant");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(codex_home.join("AGENTS.md"), "PLANTED-AGENTS-FILE").unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: plant\ndescription: PLANTED-SKILL\n---\nPLANTED-SKILL-BODY\n",
    )
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let wrapper = dir.path().join("codex-wrapper");
    write_script(
        &wrapper,
        &format!(
            "shift\n\
             export HOME='{home}' CODEX_HOME='{codex_home}'\n\
             export HTTPS_PROXY=http://127.0.0.1:9 HTTP_PROXY=http://127.0.0.1:9 NO_PROXY=127.0.0.1\n\
             exec codex exec \
             -c 'model_provider=\"here\"' -c 'model_providers.here.name=\"here\"' \
             -c 'model_providers.here.base_url=\"http://127.0.0.1:{port}/v1\"' \
             -c 'model_providers.here.wire_api=\"responses\"' \
             -c model_providers.here.request_max_retries=0 \
             -c model_providers.here.stream_max_retries=0 \"$@\"",
            home = home.display(),
            codex_home = codex_home.display(),
        ),
    );
    std::env::set_var("ROSTERY_CODEX_BIN", &wrapper);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = tx.send(first_body_of(stream));
        }
    });
    // It has to fail, as all this server does is listen, and what counts is what it tried to send.
    let _ = llm::complete(
        &Provider::Codex { model: None },
        &request(),
        Duration::from_secs(60),
    );
    std::env::remove_var("ROSTERY_CODEX_BIN");

    let body = rx
        .try_iter()
        .find(|body| body.contains("only on stdin"))
        .expect("Codex never sent the prompt");
    assert!(!body.contains("PLANTED-SKILL"), "a skill went with it");
    assert!(!body.contains("<environment_context>"), "and where it ran");
    assert!(!body.contains("<timezone>"), "and the time zone");
    println!(
        "AGENTS.md went with it: {}. ADR 0003 says it does, so change that if this changes.",
        body.contains("PLANTED-AGENTS-FILE")
    );
}

/// Reads one HTTP request and answers it with an error, which is all this needs from a server.
fn first_body_of(mut stream: TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut seen = Vec::new();
    let mut buffer = [0; 65_536];
    while let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            break;
        }
        seen.extend_from_slice(&buffer[..n]);
        let text = String::from_utf8_lossy(&seen);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let wanted = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if body.len() >= wanted {
                break;
            }
        }
    }
    let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
    String::from_utf8_lossy(&seen).into_owned()
}

/// Colour forced from the environment, an empty login and the real CLI. No history and no key are in
/// this, and OpenAI turns it away, so it says what a logged out person really gets told.
#[test]
#[ignore = "runs the real codex CLI with no login, which reaches OpenAI and gets refused"]
fn live_codex_logged_out_is_recognised_with_colour_forced() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let wrapper = dir.path().join("codex-wrapper");
    write_script(
        &wrapper,
        &format!(
            "export HOME='{0}' CODEX_HOME='{0}'\nexec codex \"$@\"",
            dir.path().display()
        ),
    );
    std::env::set_var("ROSTERY_CODEX_BIN", &wrapper);
    std::env::set_var("FORCE_COLOR", "1");
    std::env::set_var("CLICOLOR_FORCE", "1");
    let result = llm::complete(
        &Provider::Codex { model: None },
        &CompletionRequest {
            system: "Say hello.".into(),
            user: "hello".into(),
            schema: json!({"type": "object"}),
        },
        llm::DEFAULT_TIMEOUT,
    );
    std::env::remove_var("ROSTERY_CODEX_BIN");
    std::env::remove_var("FORCE_COLOR");
    std::env::remove_var("CLICOLOR_FORCE");
    let message = result.unwrap_err().to_string();
    println!("{message}");
    assert!(message.contains("not logged in"), "{message}");
    assert!(message.contains("codex login"), "{message}");
}

/// Whether OpenAI takes this schema, because a refusal would only ever show in a log.
#[test]
#[ignore = "talks to the real codex CLI"]
fn live_codex_interests() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    const NOW: i64 = 1_800_000_000;
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, NOW).unwrap();
    let both = universe::from_store(&store, UniverseFilter::Both).unwrap();
    let provider = Provider::Codex { model: None };
    let started = Instant::now();
    let tagged =
        interests::tag_missing(&provider, &mut store, &both, provider.tag_timeout()).unwrap();
    println!("tagged {tagged} in {:?}", started.elapsed());
    assert!(tagged > 0);
}

/// Costs a little subscription use, so it only runs when asked for.
#[test]
#[ignore = "talks to the real codex CLI"]
fn live_codex_roast() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let started = Instant::now();
    let roast = roast::generate(
        &Provider::Codex { model: None },
        &demo_digest(),
        roast::default_system_prompt(),
        llm::DEFAULT_TIMEOUT,
    )
    .unwrap();

    println!("took {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&roast).unwrap());
    assert!(roast.burns.len() >= 3, "only {} Burns", roast.burns.len());
    assert!(!roast.headline.is_empty() && !roast.verdict.archetype.is_empty());
}
