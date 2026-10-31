//! Providers: the AI that writes a Roast. A CLI Provider is the person's own binary, which handles
//! its own login. A Digest holds text written by strangers, so the CLI gets no tools, no settings and
//! an empty folder. ADR 0003 has the reasons.

use crate::digest::is_hidden;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, ErrorKind, Read, Seek, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// An rc file can hang, and the shell is only asked for its PATH.
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);
/// Interactive rc files print whatever they like, so the PATH is what sits between two of these.
const SENTINEL: &str = "__ROSTERY_PATH__";
const INSTALL_DIRS_IN_HOME: [&str; 6] = [
    ".local/bin",
    ".claude/local",
    ".npm-global/bin",
    ".bun/bin",
    ".volta/bin",
    ".cargo/bin",
];
const INSTALL_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
/// A stray key would move the person from their subscription to metered billing.
const REMOVED_ENV: [&str; 3] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
];
/// Far more than any real reply.
const MAX_OUTPUT_BYTES: u64 = 4 << 20;
const MAX_MESSAGE_CHARS: usize = 400;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Provider {
    ClaudeCode {
        model: Option<String>,
    },
    Codex {
        model: Option<String>,
    },
    Local {
        base_url: String,
        model: String,
        api_key: Option<String>,
    },
}

impl Provider {
    /// The name the person knows it by.
    pub fn label(&self) -> &'static str {
        match self {
            Provider::ClaudeCode { .. } => "Claude Code",
            Provider::Codex { .. } => "Codex",
            Provider::Local { .. } => "Local LLM",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionRequest {
    pub system: String,
    pub user: String,
    /// For a Provider that can enforce it. Claude Code cannot do so with no tools (ADR 0003), so
    /// its prompt carries the contract and the reply gets validated either way.
    pub schema: serde_json::Value,
}

pub fn complete(p: &Provider, req: &CompletionRequest, timeout: Duration) -> Result<String> {
    match p {
        Provider::ClaudeCode { model } => claude(model.as_deref(), req, timeout, &child_path()),
        Provider::Codex { .. } | Provider::Local { .. } => Err(Error::Provider(format!(
            "{} is not available yet. Rostery can only use Claude Code for now.",
            p.label()
        ))),
    }
}

fn claude(
    model: Option<&str>,
    req: &CompletionRequest,
    timeout: Duration,
    path: &str,
) -> Result<String> {
    let empty = tempfile::tempdir()?;
    let mut cmd = Command::new(binary("claude"));
    cmd.args(["-p", "--output-format", "json", "--system-prompt"])
        .arg(&req.system)
        .args(["--tools", ""])
        .args(["--strict-mcp-config", "--no-session-persistence"])
        .args(["--setting-sources", ""])
        .current_dir(empty.path())
        .env("PATH", path);
    if let Some(model) = model {
        cmd.args(["--model", model]);
    }
    for key in REMOVED_ENV {
        cmd.env_remove(key);
    }
    match run(&mut cmd, Some(&req.user), timeout) {
        Ok(Some(finished)) => claude_reply(&finished),
        Ok(None) => Err(Error::Provider(format!(
            "Claude Code did not finish within {}, so Rostery stopped it. Check that `claude` works \
             in a terminal, then try again.",
            how_long(timeout)
        ))),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(Error::Provider(
            "Rostery could not find Claude Code. Install it from claude.com/claude-code, log in once \
             in a terminal, then try again."
                .to_owned(),
        )),
        Err(e) => Err(Error::Provider(format!(
            "Rostery could not start Claude Code: {e}. Check that `claude` works in a terminal, then \
             try again."
        ))),
    }
}

fn how_long(timeout: Duration) -> String {
    let secs = timeout.as_secs();
    let (n, unit) = if secs >= 60 {
        (secs / 60, "minute")
    } else {
        (secs, "second")
    };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

#[derive(Deserialize)]
struct ClaudeReply {
    #[serde(default)]
    is_error: bool,
    result: Option<String>,
}

/// A CLI in trouble can still print its JSON, and the words in that say more than an exit code does.
fn claude_reply(done: &Finished) -> Result<String> {
    let reply: Option<ClaudeReply> = serde_json::from_str(done.stdout.trim()).ok();
    if let Some(ClaudeReply {
        is_error: true,
        result,
    }) = &reply
    {
        return Err(claude_problem(result.as_deref().unwrap_or("no details")));
    }
    if !done.status.success() {
        return Err(match done.status.code() {
            None => Error::Provider(
                "Claude Code was stopped before it finished. If you did not stop it, try again."
                    .to_owned(),
            ),
            Some(code) if done.stderr.trim().is_empty() => {
                claude_problem(&format!("exit code {code}"))
            }
            Some(_) => claude_problem(&done.stderr),
        });
    }
    reply
        .and_then(|r| r.result)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            Error::Provider(
                "Claude Code gave a reply Rostery could not read. Run `claude update`, check that \
                 `claude` works in a terminal, then try again."
                    .to_owned(),
            )
        })
}

fn claude_problem(detail: &str) -> Error {
    let lower = detail.to_lowercase();
    if ["not logged in", "/login", "log in"]
        .iter()
        .any(|m| lower.contains(m))
    {
        return Error::Provider(
            "Claude Code is not logged in. Run `claude` in a terminal, log in with /login, then try \
             again."
                .to_owned(),
        );
    }
    let visible: String = detail
        .chars()
        .filter(|&c| !is_hidden(c))
        .take(MAX_MESSAGE_CHARS)
        .collect();
    Error::Provider(format!(
        "Claude Code reported a problem: {}. Check that `claude` works in a terminal, then try again.",
        visible.trim().trim_end_matches('.')
    ))
}

/// The CLI to run. A test points this at a fake one, and a release build never looks.
fn binary(name: &str) -> OsString {
    #[cfg(any(test, debug_assertions))]
    if let Some(fake) = std::env::var_os(format!("ROSTERY_{}_BIN", name.to_uppercase())) {
        return fake;
    }
    name.into()
}

struct Finished {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

static RUNNING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn running() -> MutexGuard<'static, Vec<u32>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Each child is its own process group, so a wrapper script takes what it started with it.
fn kill_group(pid: u32) {
    // SAFETY: killpg takes two integers and touches no memory.
    unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
}

/// For when the app quits, so a CLI does not run on for up to 5 minutes with nobody to read its reply.
pub fn stop_running() {
    running().iter().copied().for_each(kill_group);
}

/// Runs a command to the end or to the timeout, which gives `None`. Its streams are unnamed temp
/// files, so nothing can stall on a full pipe, and a process it left behind holding its stdout open
/// cannot keep this waiting.
// ponytail: a CLI gone wrong can fill the temp file until the timeout. Only a broken CLI does that,
// and a real reply is a few KB.
fn run(cmd: &mut Command, input: Option<&str>, timeout: Duration) -> io::Result<Option<Finished>> {
    let stdin = match input {
        Some(text) => {
            let mut file = tempfile::tempfile()?;
            file.write_all(text.as_bytes())?;
            file.rewind()?;
            Stdio::from(file)
        }
        None => Stdio::null(),
    };
    let (out, err) = (tempfile::tempfile()?, tempfile::tempfile()?);
    cmd.stdin(stdin)
        .stdout(out.try_clone()?)
        .stderr(err.try_clone()?);
    // A session of its own. From a terminal, a login shell in a background group would stop as soon
    // as it reached for the terminal, and with no terminal this is the same as a launch from Finder.
    // SAFETY: setsid is async-signal-safe and touches no memory of this process.
    unsafe {
        cmd.pre_exec(|| match libc::setsid() {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    running().push(pid);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                kill_group(pid);
                let _ = child.wait();
                break Ok(None);
            }
            Ok(Some(status)) => break Ok(Some(status)),
            Err(e) => {
                kill_group(pid);
                break Err(e);
            }
        }
    };
    running().retain(|&p| p != pid);

    let Some(status) = status? else {
        return Ok(None);
    };
    Ok(Some(Finished {
        status,
        stdout: read_capped(&out)?,
        stderr: read_capped(&err)?,
    }))
}

fn read_capped(mut file: &File) -> io::Result<String> {
    let mut bytes = Vec::new();
    file.rewind()?;
    file.take(MAX_OUTPUT_BYTES).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The PATH of the person's login shell, plus any usual install folder that exists. An app launched
/// from Finder or the Dock has only `/usr/bin:/bin:/usr/sbin:/sbin`, and `claude` is not there.
pub fn login_shell_path() -> Option<String> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| {
        let (shell, home) = (std::env::var_os("SHELL"), std::env::var_os("HOME"));
        login_path(
            shell.as_deref(),
            home.as_deref().map(Path::new),
            SHELL_TIMEOUT,
        )
    })
    .clone()
}

/// `login_shell_path` with what it reads from the environment passed in, so a test needs no change
/// to this process's environment.
fn login_path(shell: Option<&OsStr>, home: Option<&Path>, timeout: Duration) -> Option<String> {
    let from_shell = shell
        .and_then(|shell| ask_shell(shell, timeout))
        .unwrap_or_default();
    let in_home = INSTALL_DIRS_IN_HOME
        .iter()
        .filter_map(|dir| home.map(|home| home.join(dir)));
    let installed: Vec<String> = in_home
        .chain(INSTALL_DIRS.iter().map(PathBuf::from))
        .filter(|dir| dir.is_dir())
        .filter_map(|dir| dir.to_str().map(String::from))
        .collect();
    let parts = std::iter::once(from_shell.as_str()).chain(installed.iter().map(String::as_str));
    Some(merge_paths(parts)).filter(|path| !path.is_empty())
}

fn ask_shell(shell: &OsStr, timeout: Duration) -> Option<String> {
    let script = format!("printf '{SENTINEL}%s{SENTINEL}' \"$PATH\"");
    let mut cmd = Command::new(shell);
    cmd.args(["-ilc", &script]).current_dir("/");
    let done = run(&mut cmd, None, timeout).ok()??;
    let (_, rest) = done.stdout.split_once(SENTINEL)?;
    let (path, _) = rest.split_once(SENTINEL)?;
    Some(path.to_owned()).filter(|path| !path.is_empty())
}

fn merge_paths<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut seen = HashSet::new();
    let dirs: Vec<&str> = parts
        .into_iter()
        .flat_map(|part| part.split(':'))
        .filter(|dir| !dir.is_empty() && seen.insert(*dir))
        .collect();
    dirs.join(":")
}

/// What a CLI finds its way with: the login shell's PATH, then whatever this app was given.
fn child_path() -> String {
    let login = login_shell_path().unwrap_or_default();
    let inherited = std::env::var("PATH").unwrap_or_default();
    merge_paths([login.as_str(), inherited.as_str()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::ffi::OsStr;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Instant;
    use tempfile::TempDir;

    const QUICK: Duration = Duration::from_secs(5);

    fn script(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A shell whose rc files talk, and which runs the command it was given the way a login shell does.
    fn talking_shell(dir: &Path, path: &str) -> std::path::PathBuf {
        let shell = dir.join("shell");
        script(
            &shell,
            &format!(
                "echo 'Good morning from your rc file'\n\
                 export PATH='{path}'\n\
                 eval \"$2\"\n\
                 echo 'and goodbye'"
            ),
        );
        shell
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            system: "Be mean".into(),
            user: "<digest>{}</digest>".into(),
            schema: json!({}),
        }
    }

    #[test]
    fn login_shell_path_finds_binary() {
        let dir = TempDir::new().unwrap();
        let bin = dir.path().join("only-here");
        script(
            &bin.join("claude"),
            "echo \"$PATH\" > \"$(dirname \"$0\")/path\"\n\
             printf '{\"type\":\"result\",\"is_error\":false,\"result\":\"found me\"}'",
        );
        let known = format!("{}:/usr/bin:/bin", bin.display());
        let shell = talking_shell(dir.path(), &known);
        let home = dir.path().join("home");

        let path = login_path(Some(shell.as_os_str()), Some(&home), QUICK).unwrap();
        assert!(path.starts_with(&known), "{path}");

        // A spawn uses that PATH, both to find the CLI and for the CLI itself to run with.
        let reply = claude(None, &request(), QUICK, &path).unwrap();
        assert_eq!(reply, "found me");
        let seen = fs::read_to_string(bin.join("path")).unwrap();
        assert_eq!(seen.trim(), path);
    }

    #[test]
    fn noise_around_the_path_stays_out_of_it() {
        let dir = TempDir::new().unwrap();
        let shell = talking_shell(dir.path(), "/known/bin:/usr/bin");
        let path = login_path(Some(shell.as_os_str()), Some(dir.path()), QUICK).unwrap();
        assert!(
            !path.contains("morning") && !path.contains("goodbye"),
            "{path}"
        );
        assert!(!path.contains(SENTINEL), "{path}");
    }

    #[test]
    fn a_shell_that_hangs_costs_its_timeout_and_the_install_dirs_still_count() {
        let dir = TempDir::new().unwrap();
        let shell = dir.path().join("shell");
        script(&shell, "sleep 60");
        let local = dir.path().join(".local/bin");
        fs::create_dir_all(&local).unwrap();

        let started = Instant::now();
        let path = login_path(
            Some(shell.as_os_str()),
            Some(dir.path()),
            Duration::from_secs(1),
        );
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(path.unwrap().contains(local.to_str().unwrap()));
    }

    #[test]
    fn a_shell_that_fails_or_prints_no_path_still_gets_the_install_dirs() {
        let dir = TempDir::new().unwrap();
        let mute = dir.path().join("mute");
        script(&mute, "echo no such option; exit 1");
        let bun = dir.path().join(".bun/bin");
        fs::create_dir_all(&bun).unwrap();
        for shell in [mute.as_os_str(), OsStr::new("/no/such/shell")] {
            let path = login_path(Some(shell), Some(dir.path()), QUICK).unwrap();
            assert!(path.contains(bun.to_str().unwrap()), "{path}");
            assert!(!path.contains("no such option"), "{path}");
        }
    }

    #[test]
    fn install_dirs_are_added_only_when_they_exist_and_only_once() {
        let dir = TempDir::new().unwrap();
        let local = dir.path().join(".local/bin");
        let claude_local = dir.path().join(".claude/local");
        fs::create_dir_all(&local).unwrap();
        fs::create_dir_all(&claude_local).unwrap();
        let on_path = format!("/known/bin:{}", local.display());
        let shell = talking_shell(dir.path(), &on_path);

        let path = login_path(Some(shell.as_os_str()), Some(dir.path()), QUICK).unwrap();
        let dirs: Vec<&str> = path.split(':').collect();
        let local = local.to_str().unwrap();
        assert_eq!(dirs.iter().filter(|d| **d == local).count(), 1);
        assert!(dirs.contains(&claude_local.to_str().unwrap()));
        assert!(dirs.iter().all(|d| !d.ends_with(".volta/bin")), "{path}");
    }

    #[test]
    fn a_timeout_is_said_in_words() {
        for (secs, words) in [
            (1, "1 second"),
            (5, "5 seconds"),
            (60, "1 minute"),
            (300, "5 minutes"),
        ] {
            assert_eq!(how_long(Duration::from_secs(secs)), words);
        }
    }

    #[test]
    fn paths_merge_in_order_with_no_repeats_or_blanks() {
        assert_eq!(merge_paths(["/a:/b", "", "/b:/c::/a"]), "/a:/b:/c");
    }

    #[test]
    fn codex_and_local_say_they_are_not_available_yet() {
        let providers = [
            Provider::Codex { model: None },
            Provider::Local {
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "llama".into(),
                api_key: None,
            },
        ];
        for provider in providers {
            let message = complete(&provider, &request(), QUICK)
                .unwrap_err()
                .to_string();
            assert!(message.contains("not available yet"), "{message}");
            assert!(message.contains(provider.label()), "{message}");
        }
    }

    #[test]
    fn provider_serializes_in_camel_case() {
        let local = Provider::Local {
            base_url: "u".into(),
            model: "m".into(),
            api_key: None,
        };
        assert_eq!(
            serde_json::to_value(&local).unwrap(),
            json!({"local": {"baseUrl": "u", "model": "m", "apiKey": null}})
        );
        assert_eq!(
            serde_json::to_value(Provider::ClaudeCode { model: None }).unwrap(),
            json!({"claudeCode": {"model": null}})
        );
    }
}
