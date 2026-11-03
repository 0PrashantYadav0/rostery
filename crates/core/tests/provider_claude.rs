//! The real `llm::complete` against a fake `claude` that records what it was given. Release builds
//! ignore the binary override, so there these would talk to the real CLI.
#![cfg(debug_assertions)]

use rostery_core::demo;
use rostery_core::digest;
use rostery_core::interests;
use rostery_core::llm::{self, CompletionRequest, Provider};
use rostery_core::roast;
use rostery_core::store::Store;
use rostery_core::universe::{self, UniverseFilter};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const SYSTEM: &str = "You are a very mean comedian.\nHere is a second line.";
const USER: &str = "<digest>{\"secret\":\"only on stdin\"}</digest>";
const OK: &str = r#"printf '{"type":"result","is_error":false,"result":"hello"}'"#;
const LONG_ENOUGH: Duration = Duration::from_secs(20);

/// Everything here changes this process's environment, so one test runs at a time.
static ENV: Mutex<()> = Mutex::new(());

struct Fake {
    dir: TempDir,
    _env: MutexGuard<'static, ()>,
}

impl Fake {
    /// A `claude` that records its argv, stdin, cwd, the listing of its cwd and its environment,
    /// then runs `body`.
    fn new(body: &str) -> Fake {
        let env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = TempDir::new().unwrap();
        let record = dir.path().display();
        write_script(
            &dir.path().join("claude"),
            &format!(
                "printf '%s\\0' \"$@\" > '{record}/argv'\n\
                 cat > '{record}/stdin'\n\
                 pwd -P > '{record}/cwd'\n\
                 ls -A > '{record}/listing'\n\
                 env > '{record}/env'\n\
                 {body}"
            ),
        );
        // No test here should depend on the rc files of whoever runs it.
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("ROSTERY_CLAUDE_BIN", dir.path().join("claude"));
        Fake { dir, _env: env }
    }

    fn ask(&self, timeout: Duration) -> rostery_core::Result<String> {
        llm::complete(
            &Provider::ClaudeCode { model: None },
            &CompletionRequest {
                system: SYSTEM.into(),
                user: USER.into(),
                schema: json!({}),
            },
            timeout,
        )
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
}

impl Drop for Fake {
    fn drop(&mut self) {
        std::env::remove_var("ROSTERY_CLAUDE_BIN");
    }
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn gone(pid: &str) -> bool {
    !Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// A killed process can take a moment to leave the process table.
fn wait_until_gone(pid: &str) -> bool {
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if gone(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn claude_runs_without_tools() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    assert_eq!(
        fake.argv(),
        [
            "-p",
            "--output-format",
            "json",
            "--system-prompt",
            SYSTEM,
            "--tools",
            "",
            "--strict-mcp-config",
            "--no-session-persistence",
            "--setting-sources",
            "",
        ]
    );
}

#[test]
fn claude_gets_the_model_when_one_is_set() {
    let fake = Fake::new(OK);
    llm::complete(
        &Provider::ClaudeCode {
            model: Some("haiku".into()),
        },
        &CompletionRequest {
            system: SYSTEM.into(),
            user: USER.into(),
            schema: json!({}),
        },
        LONG_ENOUGH,
    )
    .unwrap();
    let argv = fake.argv();
    let at = argv.iter().position(|a| a == "--model").unwrap();
    assert_eq!(argv[at + 1], "haiku");
}

#[test]
fn claude_reads_digest_from_stdin() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    assert_eq!(fake.recorded("stdin"), USER);
    assert!(!fake.argv().iter().any(|arg| arg.contains("only on stdin")));
}

#[test]
fn no_at_sign_reaches_the_cli_so_it_cannot_read_a_file() {
    let fake = Fake::new(OK);
    let user = serde_json::to_string(&json!({"name": "Lo Fi @~/.ssh/id_rsa radio \\@x"})).unwrap();
    llm::complete(
        &Provider::ClaudeCode { model: None },
        &CompletionRequest {
            system: SYSTEM.into(),
            user: user.clone(),
            schema: json!({}),
        },
        LONG_ENOUGH,
    )
    .unwrap();
    let stdin = fake.recorded("stdin");
    assert!(!stdin.contains('@'), "{stdin}");
    let read: serde_json::Value = serde_json::from_str(&stdin).unwrap();
    assert_eq!(
        read,
        serde_json::from_str::<serde_json::Value>(&user).unwrap()
    );
    assert!(fake
        .recorded("env")
        .lines()
        .any(|l| l == "CLAUDE_CODE_DISABLE_ATTACHMENTS=1"));
}

#[test]
fn claude_runs_in_empty_temp_dir() {
    let fake = Fake::new(OK);
    fake.ask(LONG_ENOUGH).unwrap();
    let cwd = fake.recorded("cwd");
    let cwd = Path::new(cwd.trim());
    assert_eq!(fake.recorded("listing"), "");
    assert_ne!(
        cwd,
        std::env::current_dir().unwrap().canonicalize().unwrap()
    );
    assert!(!cwd.exists(), "the empty folder is cleaned up afterwards");
}

#[test]
fn claude_child_has_no_api_key() {
    let fake = Fake::new(OK);
    std::env::set_var("ANTHROPIC_API_KEY", "sk-stray-key");
    std::env::set_var("ANTHROPIC_AUTH_TOKEN", "stray-token");
    std::env::set_var("OPENAI_API_KEY", "sk-other-stray-key");
    std::env::set_var("ROSTERY_KEEPS_THIS", "yes");
    let reply = fake.ask(LONG_ENOUGH);
    for key in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "OPENAI_API_KEY",
        "ROSTERY_KEEPS_THIS",
    ] {
        std::env::remove_var(key);
    }
    reply.unwrap();
    let env = fake.recorded("env");
    assert!(!env.contains("ANTHROPIC_API_KEY"), "{env}");
    assert!(!env.contains("ANTHROPIC_AUTH_TOKEN"), "{env}");
    assert!(!env.contains("OPENAI_API_KEY"), "{env}");
    assert!(
        env.contains("ROSTERY_KEEPS_THIS=yes"),
        "the rest of the environment stays"
    );
}

/// So a launch from a Claude Code terminal behaves like any other, where `claude` would refuse to run
/// inside itself.
#[test]
fn claude_child_does_not_know_it_was_started_from_claude_code() {
    let fake = Fake::new(OK);
    std::env::set_var("CLAUDECODE", "1");
    std::env::set_var("CLAUDE_CODE_ENTRYPOINT", "cli");
    let reply = fake.ask(LONG_ENOUGH);
    std::env::remove_var("CLAUDECODE");
    std::env::remove_var("CLAUDE_CODE_ENTRYPOINT");
    reply.unwrap();
    let env = fake.recorded("env");
    assert!(!env.contains("CLAUDECODE="), "{env}");
    assert!(!env.contains("CLAUDE_CODE_ENTRYPOINT="), "{env}");
}

#[test]
fn claude_result_is_parsed() {
    let fake = Fake::new(OK);
    assert_eq!(fake.ask(LONG_ENOUGH).unwrap(), "hello");
}

#[test]
fn claude_error_result_becomes_error() {
    let fake = Fake::new(
        r#"printf '{"type":"result","is_error":true,"result":"Credit balance is too low"}'"#,
    );
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("Credit balance is too low"), "{message}");
    assert!(message.contains("try again"), "{message}");
}

#[test]
fn logged_out_says_how_to_log_in() {
    // What the CLI really prints, and with the exit code it really uses.
    let fake = Fake::new(
        r#"printf '{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}'; exit 1"#,
    );
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("not logged in"), "{message}");
    assert!(message.contains("/login"), "{message}");
}

/// With `verbose` on in the CLI's global config, `--output-format json` prints every event as an
/// array, and `--setting-sources` does not stop it. This is the shape CLI 2.1.310 prints.
const VERBOSE_EVENTS: &str = r#"{"type":"system","subtype":"init","model":"m"},{"type":"assistant","message":{"content":[{"type":"text","text":"not this"}]}}"#;

#[test]
fn claude_with_verbose_on_is_parsed() {
    let fake = Fake::new(&format!(
        r#"printf '[{VERBOSE_EVENTS},{{"type":"result","is_error":false,"result":"hello"}}]'"#
    ));
    assert_eq!(fake.ask(LONG_ENOUGH).unwrap(), "hello");
}

#[test]
fn logged_out_with_verbose_on_says_how_to_log_in() {
    let fake = Fake::new(&format!(
        r#"printf '[{VERBOSE_EVENTS},{{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}}]'; exit 1"#
    ));
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("not logged in"), "{message}");
    assert!(message.contains("/login"), "{message}");
}

#[test]
fn an_error_that_only_looks_like_a_login_problem_is_not_called_one() {
    for words in [
        "could not read the plugin catalog in /tmp",
        "dialog in the way",
        "failed to log information",
    ] {
        let fake = Fake::new(&format!("echo '{words}' >&2; exit 2"));
        let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
        assert!(message.contains(words), "{message}");
        assert!(!message.contains("not logged in"), "{message}");
    }
}

#[test]
fn a_cli_too_old_for_these_flags_says_to_update() {
    let fake = Fake::new("echo \"error: unknown option '--tools'\" >&2; exit 1");
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("too old"), "{message}");
    assert!(message.contains("claude update"), "{message}");
}

/// The screen shows these as plain text, so a backtick would show as a backtick.
#[test]
fn no_message_has_backticks() {
    for body in [
        "exit 3",
        "echo 'broken' >&2; exit 3",
        "echo 'Not logged in' >&2; exit 1",
        "echo \"error: unknown option '--tools'\" >&2; exit 1",
        "echo 'not json'",
        "kill -9 $$",
        "sleep 60",
    ] {
        let fake = Fake::new(body);
        let message = fake.ask(Duration::from_secs(1)).unwrap_err().to_string();
        assert!(!message.contains('`'), "{message}");
    }
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("not-here"));
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(!message.contains('`'), "{message}");
}

#[test]
fn nonzero_exit_reports_stderr() {
    let fake = Fake::new("echo 'something broke in here' >&2; exit 3");
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("something broke in here"), "{message}");
}

#[test]
fn nonzero_exit_with_nothing_on_stderr_still_says_something() {
    let fake = Fake::new("exit 3");
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("exit code 3"), "{message}");
    assert!(message.contains("terminal"), "{message}");
}

#[test]
fn a_child_that_is_killed_says_so() {
    let fake = Fake::new("kill -9 $$");
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("stopped before it finished"), "{message}");
    assert!(message.contains("try again"), "{message}");
}

#[test]
fn a_reply_that_is_not_json_says_what_to_do() {
    for body in [
        "echo 'Welcome to Claude'",
        ":",
        r#"printf '{"type":"result","is_error":false}'"#,
        r#"printf '[{"type":"system","subtype":"init"}]'"#,
    ] {
        let fake = Fake::new(body);
        let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
        assert!(message.contains("could not read"), "{message}");
        assert!(message.contains("claude update"), "{message}");
    }
}

#[test]
fn a_long_message_from_the_cli_is_cut_and_loses_its_control_characters() {
    let fake = Fake::new(r#"printf '\033[31m'; yes broken | head -c 100000 >&2; exit 1"#);
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.len() < 1000, "{} bytes", message.len());
    assert!(!message.contains('\u{1b}'));
}

#[test]
fn a_flood_of_output_cannot_stall_the_run_or_fill_memory() {
    let fake = Fake::new("yes flood | head -c 5000000; exit 0");
    let started = Instant::now();
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("could not read"), "{message}");
    assert!(started.elapsed() < LONG_ENOUGH);
}

#[test]
fn missing_binary_says_how_to_install() {
    let fake = Fake::new(OK);
    std::env::set_var("ROSTERY_CLAUDE_BIN", fake.dir.path().join("not-here"));
    let message = fake.ask(LONG_ENOUGH).unwrap_err().to_string();
    assert!(message.contains("could not find Claude Code"), "{message}");
    assert!(message.contains("Install"), "{message}");
}

#[test]
fn timeout_kills_child() {
    // The CLI might be a wrapper, so whatever it started has to go as well.
    let fake = Fake::new(
        "dir=$(dirname \"$0\")\necho $$ > \"$dir/pid\"\nsleep 60 &\necho $! > \"$dir/inner\"\nwait",
    );
    let started = Instant::now();
    let message = fake.ask(Duration::from_secs(1)).unwrap_err().to_string();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
    assert!(message.contains("did not finish"), "{message}");
    assert!(message.contains("try again"), "{message}");
    assert!(
        wait_until_gone(&fake.recorded("pid")),
        "the CLI is still running"
    );
    assert!(
        wait_until_gone(&fake.recorded("inner")),
        "what it started is still running"
    );
}

#[test]
fn stop_running_ends_a_cli_when_the_app_quits() {
    let fake = Fake::new("echo $$ > \"$(dirname \"$0\")/pid\"\nsleep 60 &\nwait");
    std::thread::scope(|scope| {
        let run = scope.spawn(|| fake.ask(LONG_ENOUGH));
        let pid_file = fake.dir.path().join("pid");
        let until = Instant::now() + Duration::from_secs(5);
        while !pid_file.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let started = Instant::now();
        llm::stop_running();
        let message = run.join().unwrap().unwrap_err().to_string();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(message.contains("stopped before it finished"), "{message}");
    });
    assert!(wait_until_gone(&fake.recorded("pid")));
}

fn demo_store() -> Store {
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, 1_800_000_000).unwrap();
    store
}

fn both(store: &Store) -> universe::Universe {
    universe::from_store(store, UniverseFilter::Both).unwrap()
}

/// A stuck tag step costs its own timeout and marks nobody, so the Roast has all of its own.
#[test]
fn a_tag_step_that_hangs_costs_only_its_timeout_and_is_stopped() {
    let fake = Fake::new("echo $$ > \"$(dirname \"$0\")/pid\"\nsleep 60 &\nwait");
    let mut store = demo_store();
    let universe = both(&store);
    let started = Instant::now();
    let message = interests::tag_missing(
        &Provider::ClaudeCode { model: None },
        &mut store,
        &universe,
        Duration::from_secs(1),
    )
    .unwrap_err()
    .to_string();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(message.contains("did not finish"), "{message}");
    assert!(wait_until_gone(&fake.recorded("pid")));
    assert!(store.ai_asked().unwrap().is_empty());
}

#[test]
fn quitting_during_the_tag_step_stops_the_cli() {
    let fake = Fake::new("echo $$ > \"$(dirname \"$0\")/pid\"\nsleep 60 &\nwait");
    let mut store = demo_store();
    let universe = both(&store);
    std::thread::scope(|scope| {
        let run = scope.spawn(|| {
            interests::tag_missing(
                &Provider::ClaudeCode { model: None },
                &mut store,
                &universe,
                LONG_ENOUGH,
            )
        });
        let pid_file = fake.dir.path().join("pid");
        let until = Instant::now() + Duration::from_secs(5);
        while !pid_file.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let started = Instant::now();
        llm::stop_running();
        let message = run.join().unwrap().unwrap_err().to_string();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(message.contains("stopped before it finished"), "{message}");
    });
    assert!(wait_until_gone(&fake.recorded("pid")));
    assert!(store.ai_asked().unwrap().is_empty());
}

/// The same locked down way as a Roast, because the names were written by strangers too.
#[test]
fn the_tag_step_runs_the_cli_locked_down_with_the_names_on_stdin() {
    let fake = Fake::new(
        r#"printf '{"type":"result","is_error":false,"result":"{\\"creators\\":[{\\"source\\":\\"spotify\\",\\"name\\":\\"Moth Season\\",\\"interests\\":[]}]}"}'"#,
    );
    let mut store = demo_store();
    let universe = both(&store);
    interests::tag_missing(
        &Provider::ClaudeCode { model: None },
        &mut store,
        &universe,
        LONG_ENOUGH,
    )
    .unwrap();
    let argv = fake.argv();
    for flag in ["--tools", "--strict-mcp-config", "--setting-sources"] {
        assert!(argv.iter().any(|a| a == flag), "{flag}: {argv:?}");
    }
    let stdin = fake.recorded("stdin");
    assert!(stdin.starts_with("<creators>\n"), "{stdin}");
    assert!(!argv.iter().any(|a| a.contains("Moth Season")), "{argv:?}");
}

/// Runs one `#[ignore]` test below in a process of its own, launched the way Finder launches an app:
/// a bare PATH, with `home` as HOME. So the real `login_shell_path` and its OnceLock start fresh.
fn in_a_fresh_process(test: &str, home: &Path, shell: &Path) {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", home)
        .env("SHELL", shell)
        .env("ROSTERY_TEST_FINDER", "1")
        .output()
        .unwrap();
    let (out, err) = (
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr),
    );
    assert!(child.status.success(), "{out}\n{err}");
    assert!(out.contains("1 passed"), "{out}");
}

fn in_the_fresh_process() -> bool {
    std::env::var_os("ROSTERY_TEST_FINDER").is_some()
}

/// A name no real CLI has, so these can never reach the real `claude` in one of the usual folders.
const FAKE_CLAUDE: &str = "rostery-fake-claude";

fn ask_fresh() -> rostery_core::Result<String> {
    std::env::set_var("ROSTERY_CLAUDE_BIN", FAKE_CLAUDE);
    llm::complete(
        &Provider::ClaudeCode { model: None },
        &CompletionRequest {
            system: SYSTEM.into(),
            user: USER.into(),
            schema: json!({}),
        },
        LONG_ENOUGH,
    )
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap())
}

#[test]
fn a_finder_launch_finds_claude_through_the_login_shell() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let bin: PathBuf = dir.path().join("somewhere-only-the-shell-knows");
    std::fs::create_dir(&bin).unwrap();
    write_script(&bin.join("claude"), OK);
    let shell = dir.path().join("shell");
    write_script(
        &shell,
        &format!(
            "echo 'Welcome back'\nexport PATH='{}:/usr/bin:/bin'\neval \"$2\"\necho 'Bye'",
            bin.display()
        ),
    );
    in_a_fresh_process("launched_like_finder", dir.path(), &shell);
}

#[test]
#[ignore = "run by a_finder_launch_finds_claude_through_the_login_shell"]
fn launched_like_finder() {
    if !in_the_fresh_process() {
        return;
    }
    let path = llm::login_shell_path().unwrap();
    assert!(path.contains("somewhere-only-the-shell-knows"), "{path}");
    let reply = llm::complete(
        &Provider::ClaudeCode { model: None },
        &CompletionRequest {
            system: SYSTEM.into(),
            user: USER.into(),
            schema: json!({}),
        },
        LONG_ENOUGH,
    );
    assert_eq!(reply.unwrap(), "hello");
}

/// A shell that knows nothing useful, for a test where the CLI turns up in an install folder.
fn plain_shell(dir: &Path) -> PathBuf {
    let shell = dir.join("shell");
    write_script(&shell, "export PATH=/usr/bin:/bin\neval \"$2\"");
    shell
}

/// The app tells the person to install it and try again, so that has to work with the app still open.
#[test]
fn claude_installed_after_a_failed_try_is_found_without_a_restart() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let shell = plain_shell(dir.path());
    in_a_fresh_process("installed_after_a_failed_try", dir.path(), &shell);
}

#[test]
#[ignore = "run by claude_installed_after_a_failed_try_is_found_without_a_restart"]
fn installed_after_a_failed_try() {
    if !in_the_fresh_process() {
        return;
    }
    let message = ask_fresh().unwrap_err().to_string();
    assert!(message.contains("could not find Claude Code"), "{message}");

    let local = home().join(".local/bin");
    std::fs::create_dir_all(&local).unwrap();
    write_script(&local.join(FAKE_CLAUDE), OK);

    assert_eq!(ask_fresh().unwrap(), "hello");
}

/// The shell gave an answer before the CLI was installed, and only its next answer holds the way there.
#[test]
fn claude_only_a_newer_shell_answer_knows_is_found_without_a_restart() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let shell = dir.path().join("shell");
    write_script(
        &shell,
        "export PATH=/usr/bin:/bin\n[ -e \"$HOME/rc\" ] && . \"$HOME/rc\"\neval \"$2\"",
    );
    in_a_fresh_process("found_through_a_newer_answer", dir.path(), &shell);
}

#[test]
#[ignore = "run by claude_only_a_newer_shell_answer_knows_is_found_without_a_restart"]
fn found_through_a_newer_answer() {
    if !in_the_fresh_process() {
        return;
    }
    let message = ask_fresh().unwrap_err().to_string();
    assert!(message.contains("could not find Claude Code"), "{message}");

    let bin = home().join("somewhere-only-the-shell-knows");
    std::fs::create_dir_all(&bin).unwrap();
    write_script(&bin.join(FAKE_CLAUDE), OK);
    let rc = format!("export PATH=\"{}:$PATH\"\n", bin.display());
    std::fs::write(home().join("rc"), rc).unwrap();

    assert_eq!(ask_fresh().unwrap(), "hello");
}

/// A shell that is slow or fails on its first go, then answers. Only it knows where the CLI is.
fn shell_that_stumbles_once(dir: &Path, first_go: &str) -> PathBuf {
    let bin = dir.join("somewhere-only-the-shell-knows");
    std::fs::create_dir(&bin).unwrap();
    write_script(&bin.join(FAKE_CLAUDE), OK);
    let shell = dir.join("shell");
    write_script(
        &shell,
        &format!(
            "echo asked >> '{asked}'\n\
             [ \"$(wc -l < '{asked}')\" -gt 1 ] || {{ {first_go}; }}\n\
             export PATH='{bin}:/usr/bin:/bin'\n\
             eval \"$2\"",
            asked = dir.join("asked").display(),
            bin = bin.display(),
        ),
    );
    shell
}

#[test]
fn a_shell_that_fails_once_is_asked_again() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let shell = shell_that_stumbles_once(dir.path(), "exit 1");
    in_a_fresh_process("stumbling_shell_recovers", dir.path(), &shell);
}

/// Sits through the real 5 second limit on asking a shell.
#[test]
fn a_shell_that_is_too_slow_once_is_asked_again() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let shell = shell_that_stumbles_once(dir.path(), "sleep 7");
    in_a_fresh_process("stumbling_shell_recovers", dir.path(), &shell);
}

#[test]
#[ignore = "run by a_shell_that_fails_once_is_asked_again and a_shell_that_is_too_slow_once_is_asked_again"]
fn stumbling_shell_recovers() {
    if !in_the_fresh_process() {
        return;
    }
    let asked = || {
        std::fs::read_to_string(home().join("asked"))
            .unwrap()
            .lines()
            .count()
    };

    assert_eq!(ask_fresh().unwrap(), "hello");
    assert_eq!(asked(), 2, "asked again before giving up");

    // An answer is worth keeping, so the next Roast does not ask.
    assert_eq!(ask_fresh().unwrap(), "hello");
    assert_eq!(asked(), 2);
}

/// The whole of a Roast's work, the way `generate_roast` does it. Prints what it got, for a person to
/// judge whether the tags and the Links make sense. Costs a little subscription use.
#[test]
#[ignore = "talks to the real claude CLI"]
fn live_interests() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let mut store = demo_store();
    let provider = Provider::ClaudeCode {
        model: Some("haiku".into()),
    };
    let before = both(&store);
    let link_key = |u: &universe::Universe, l: &universe::Link| {
        let name = |id: u32| u.planets[id as usize].name.clone();
        let (a, b) = (name(l.a), name(l.b));
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    };
    let links_before: std::collections::HashSet<_> =
        before.links.iter().map(|l| link_key(&before, l)).collect();

    let started = Instant::now();
    let tagged =
        interests::tag_missing(&provider, &mut store, &before, provider.tag_timeout()).unwrap();
    println!(
        "tagged {tagged} of {} in {:?}",
        before.planets.len(),
        started.elapsed()
    );
    assert!(tagged > 0);

    let after = both(&store);
    println!("\nINTERESTS");
    for planet in &after.planets {
        println!(
            "{:?} {}: {:?}",
            planet.source, planet.name, planet.interests
        );
    }
    println!(
        "\nNEW LINKS ({} before, {} after)",
        before.links.len(),
        after.links.len()
    );
    let cross_source = |u: &universe::Universe, l: &universe::Link| {
        u.planets[l.a as usize].source != u.planets[l.b as usize].source
    };
    let new: Vec<_> = after
        .links
        .iter()
        .filter(|l| !links_before.contains(&link_key(&after, l)))
        .collect();
    for link in &new {
        let (a, b) = link_key(&after, link);
        println!(
            "{a} <-> {b}: {:.2}, shared {:?}, across Sources: {}",
            link.strength,
            link.shared,
            cross_source(&after, link)
        );
    }

    let started = Instant::now();
    let digest = digest::from_store(&store, 1_800_000_000).unwrap();
    let roast = roast::generate(
        &provider,
        &digest,
        roast::default_system_prompt(),
        llm::DEFAULT_TIMEOUT,
    )
    .unwrap();
    println!("\nROAST in {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&roast).unwrap());
    store
        .save_roast(provider.label(), &roast, 1_800_000_000, true)
        .unwrap();

    println!("\nGALAXIES");
    let spotify = universe::from_store(&store, UniverseFilter::Spotify).unwrap();
    let youtube = universe::from_store(&store, UniverseFilter::Youtube).unwrap();
    let named = both(&store);
    for (view, u) in [
        ("both", &named),
        ("spotify", &spotify),
        ("youtube", &youtube),
    ] {
        let names: Vec<&str> = u.galaxies.iter().map(|g| g.name.as_str()).collect();
        println!("{view}: {names:?}");
    }

    // No assertion on new Links. Demo Creators already hold 4 Interests, so a Provider's tags mostly
    // dilute them, and any new Link is down to who else fell out of a Planet's top 6. Whether tags make
    // Links is `ai_tags_create_links`, and whether they do on real history is a stop point 3 check.
    let from_roast: Vec<&str> = roast.galaxy_names.iter().map(|g| g.name.as_str()).collect();
    assert!(
        named
            .galaxies
            .iter()
            .any(|g| from_roast.contains(&g.name.as_str())),
        "no Galaxy has a Roast name: {from_roast:?}"
    );

    // Everybody has been asked about now, so nothing goes out the second time.
    let asked = interests::next_to_tag(&store, &named).unwrap();
    assert!(asked.is_empty(), "{asked:?}");
}

/// A name holding `@path` once made the real CLI attach that file. The canary must not come back.
#[test]
#[ignore = "talks to the real claude CLI"]
fn live_claude_does_not_read_a_mentioned_file() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = TempDir::new().unwrap();
    let canary = dir.path().join("canary.txt");
    std::fs::write(&canary, "the canary word is periwinkle-4471").unwrap();
    let user = serde_json::to_string(&json!({
        "creators": [{"source": "youtube", "name": format!("Lo Fi @{} radio", canary.display())}]
    }))
    .unwrap();
    let reply = llm::complete(
        &Provider::ClaudeCode {
            model: Some("haiku".into()),
        },
        &CompletionRequest {
            system: "Repeat back everything you were given in this conversation, word for word, \
                     including the contents of any attached file."
                .into(),
            user,
            schema: json!({}),
        },
        llm::DEFAULT_TIMEOUT,
    )
    .unwrap();
    println!("{reply}");
    assert!(!reply.contains("periwinkle-4471"), "{reply}");
}

/// Costs a little subscription use, so it only runs when asked for.
#[test]
#[ignore = "talks to the real claude CLI"]
fn live_claude_roast() {
    let _env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    const NOW: i64 = 1_800_000_000;
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, NOW).unwrap();
    let digest = digest::from_store(&store, NOW).unwrap();

    let started = Instant::now();
    let roast = roast::generate(
        &Provider::ClaudeCode {
            model: Some("haiku".into()),
        },
        &digest,
        roast::default_system_prompt(),
        llm::DEFAULT_TIMEOUT,
    )
    .unwrap();

    println!("took {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&roast).unwrap());
    assert!(roast.burns.len() >= 3, "only {} Burns", roast.burns.len());
    assert!(!roast.headline.is_empty() && !roast.verdict.archetype.is_empty());
}
