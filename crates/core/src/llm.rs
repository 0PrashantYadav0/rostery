//! Providers: the AI that writes a Roast. A CLI Provider is the person's own binary, which handles
//! its own login. A Digest holds text written by strangers, so the CLI gets no tools, no settings and
//! an empty folder. ADR 0003 has the reasons. A local Provider is any server that speaks the OpenAI
//! chat API, and it has to say where it is.

use crate::error::{Error, Result};
use crate::roast;
use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, ErrorKind, Read, Seek, Write};
use std::net::IpAddr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use ureq::http::Uri;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// A model on a CPU writes slowly.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(600);
const LOCAL_STATUS_TIMEOUT: Duration = Duration::from_secs(2);
/// A CLI has a runtime to start, which a cold disk can take longer than 2 seconds over.
const CLI_STATUS_TIMEOUT: Duration = Duration::from_secs(5);
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
/// A stray key would move the person from their subscription to metered billing. The last two mark a
/// launch from a Claude Code terminal, which would make `claude` refuse to run inside itself.
const REMOVED_ENV: [&str; 6] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
];
/// Far more than any real reply.
const MAX_OUTPUT_BYTES: u64 = 4 << 20;
const MAX_MESSAGE_CHARS: usize = 400;
const MAX_MODELS: usize = 200;
const MAX_MODEL_NAME: usize = 200;
/// Far more than the default, and still small enough for one argument of a command line.
pub const MAX_SYSTEM_PROMPT_CHARS: usize = 20_000;
const SETTINGS_KEY: &str = "settings";
const DIGEST_HOLDS: &str =
    "It holds song and video titles, Creator names, Interests and dates, never your raw Plays.";

/// Which card a local Provider is. They speak the same API, so either can be pointed at any server
/// that does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LocalServer {
    Ollama,
    LmStudio,
}

impl LocalServer {
    fn label(self) -> &'static str {
        match self {
            LocalServer::Ollama => "Ollama",
            LocalServer::LmStudio => "LM Studio",
        }
    }

    pub fn default_url(self) -> &'static str {
        match self {
            LocalServer::Ollama => "http://127.0.0.1:11434/v1",
            LocalServer::LmStudio => "http://127.0.0.1:1234/v1",
        }
    }

    fn how_to_start(self) -> &'static str {
        match self {
            LocalServer::Ollama => "Install Ollama from ollama.com or start it with ollama serve",
            LocalServer::LmStudio => {
                "Install LM Studio from lmstudio.ai and start its local server"
            }
        }
    }

    fn how_to_get_a_model(self) -> &'static str {
        match self {
            LocalServer::Ollama => "Run ollama pull with the name of a model",
            LocalServer::LmStudio => "Download a model in LM Studio",
        }
    }
}

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
        server: LocalServer,
        base_url: String,
        model: String,
        /// Sent when present, and never written to the settings JSON. A key belongs in the keychain.
        #[serde(skip)]
        api_key: Option<String>,
    },
}

impl Provider {
    /// The name the person knows it by.
    pub fn label(&self) -> &'static str {
        match self {
            Provider::ClaudeCode { .. } => CLAUDE.label,
            Provider::Codex { .. } => CODEX.label,
            Provider::Local { server, .. } => server.label(),
        }
    }

    /// The id of its card on the Settings screen.
    pub fn id(&self) -> &'static str {
        match self {
            Provider::ClaudeCode { .. } => "claude-code",
            Provider::Codex { .. } => "codex",
            Provider::Local {
                server: LocalServer::Ollama,
                ..
            } => "ollama",
            Provider::Local {
                server: LocalServer::LmStudio,
                ..
            } => "lm-studio",
        }
    }

    /// How long a Roast may take.
    pub fn timeout(&self) -> Duration {
        match self {
            Provider::Local { .. } => LOCAL_TIMEOUT,
            _ => DEFAULT_TIMEOUT,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionRequest {
    pub system: String,
    pub user: String,
    /// For a Provider that can enforce it. Codex does. Claude Code cannot with no tools (ADR 0003) and
    /// servers differ, so the prompt carries the contract and the reply gets validated either way.
    pub schema: serde_json::Value,
}

pub fn complete(p: &Provider, req: &CompletionRequest, timeout: Duration) -> Result<String> {
    match p {
        Provider::ClaudeCode { model } => claude(
            model.as_deref(),
            req,
            timeout,
            &child_path(login_shell_path()),
        ),
        Provider::Codex { model } => codex(
            model.as_deref(),
            req,
            timeout,
            &child_path(login_shell_path()),
        ),
        Provider::Local {
            server,
            base_url,
            model,
            api_key,
        } => local(*server, base_url, model, api_key.as_deref(), req, timeout),
    }
}

/// What differs between the two CLIs in what they get told and what the person gets told.
struct Cli {
    binary: &'static str,
    label: &'static str,
    install: &'static str,
    login: &'static str,
    update: &'static str,
}

const CLAUDE: Cli = Cli {
    binary: "claude",
    label: "Claude Code",
    install: "Install it from claude.com/claude-code and log in once in a terminal",
    login: "Run claude in a terminal and log in with /login",
    update: "Run claude update",
};

const CODEX: Cli = Cli {
    binary: "codex",
    label: "Codex",
    install: "Install it from github.com/openai/codex and run codex login once in a terminal",
    login: "Run codex login in a terminal",
    update: "Run codex update, or update it the way you installed it",
};

/// Runs a CLI with what every Provider call gets: the PATH it was given and no stray keys.
fn run_cli(
    cli: &Cli,
    configure: &dyn Fn(&mut Command),
    input: Option<&str>,
    timeout: Duration,
    path: &str,
) -> io::Result<Option<Finished>> {
    // A Command of its own each time, because `run` adds a pre_exec that only works once per Command.
    let attempt = |path: &str| {
        let mut cmd = Command::new(binary(cli.binary));
        configure(&mut cmd);
        cmd.env("PATH", path);
        for key in REMOVED_ENV {
            cmd.env_remove(key);
        }
        run(&mut cmd, input, timeout)
    };
    match attempt(path) {
        // The CLI may be newer than the PATH, or the shell may not have answered in time. Telling
        // someone to install what they have costs far more than asking the shell once more.
        Err(e) if e.kind() == ErrorKind::NotFound => {
            attempt(&child_path(with_install_dirs(ask_login_shell())))
        }
        outcome => outcome,
    }
}

/// What the person hears when a CLI never got as far as an answer.
fn cli_finished(
    cli: &Cli,
    outcome: io::Result<Option<Finished>>,
    timeout: Duration,
) -> Result<Finished> {
    let (label, binary) = (cli.label, cli.binary);
    match outcome {
        Ok(Some(finished)) => Ok(finished),
        Ok(None) => Err(Error::Provider(format!(
            "{label} did not finish within {}, so Rostery stopped it. Check that the {binary} command \
             works in a terminal, then try again.",
            how_long(timeout)
        ))),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(Error::Provider(format!(
            "Rostery could not find {label}. {}, then try again. If it is already installed, try again \
             anyway, because your login shell may have been too slow to say where it is.",
            cli.install
        ))),
        Err(e) => Err(Error::Provider(format!(
            "Rostery could not start {label}: {e}. Check that the {binary} command works in a terminal, \
             then try again."
        ))),
    }
}

fn cli_stopped_or_problem(cli: &Cli, done: &Finished, detail: &str) -> Error {
    match done.status.code() {
        None => Error::Provider(format!(
            "{} was stopped before it finished. If you did not stop it, try again.",
            cli.label
        )),
        Some(code) if detail.trim().is_empty() => cli_problem(cli, &format!("exit code {code}")),
        Some(_) => cli_problem(cli, detail),
    }
}

fn cli_unreadable(cli: &Cli) -> Error {
    Error::Provider(format!(
        "{} gave a reply Rostery could not read. {}, check that the {} command works in a terminal, then \
         try again.",
        cli.label, cli.update, cli.binary
    ))
}

fn says_logged_out(lower: &str) -> bool {
    // The last is how Codex says it, with nothing about logging in anywhere near it.
    [
        "not logged in",
        "/login",
        "please log in",
        "codex login",
        "401 unauthorized",
    ]
    .iter()
    .any(|mark| lower.contains(mark))
}

/// Both CLIs refuse a flag or a command they have never heard of in one of these ways.
fn says_too_old(lower: &str) -> bool {
    [
        "unknown option",
        "unknown command",
        "unexpected argument",
        "unrecognized option",
        "unrecognized argument",
        "unrecognized subcommand",
    ]
    .iter()
    .any(|mark| lower.contains(mark))
}

fn cli_problem(cli: &Cli, detail: &str) -> Error {
    let lower = detail.to_lowercase();
    if says_logged_out(&lower) {
        return Error::Provider(format!(
            "{} is not logged in. {}, then try again.",
            cli.label, cli.login
        ));
    }
    if says_too_old(&lower) {
        return Error::Provider(format!(
            "{} is too old for Rostery. {}, then try again.",
            cli.label, cli.update
        ));
    }
    let visible = roast::clean(detail, MAX_MESSAGE_CHARS);
    Error::Provider(format!(
        "{} reported a problem: {}. Check that the {} command works in a terminal, then try again.",
        cli.label,
        visible.trim_end_matches('.'),
        cli.binary
    ))
}

fn claude(
    model: Option<&str>,
    req: &CompletionRequest,
    timeout: Duration,
    path: &str,
) -> Result<String> {
    let empty = tempfile::tempdir()?;
    let configure = |cmd: &mut Command| {
        cmd.args(["-p", "--output-format", "json", "--system-prompt"])
            .arg(&req.system)
            .args(["--tools", ""])
            .args(["--strict-mcp-config", "--no-session-persistence"])
            .args(["--setting-sources", ""])
            .current_dir(empty.path());
        if let Some(model) = model {
            cmd.args(["--model", model]);
        }
    };
    let outcome = run_cli(&CLAUDE, &configure, Some(&req.user), timeout, path);
    claude_reply(&cli_finished(&CLAUDE, outcome, timeout)?)
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

/// One object, or with `verbose` on in the CLI's global config every event in an array, which
/// `--setting-sources` does not stop. The result is the last of those.
fn parse_claude_reply(stdout: &str) -> Option<ClaudeReply> {
    let result = match serde_json::from_str(stdout).ok()? {
        Value::Array(events) => events.into_iter().rev().find(|e| e["type"] == "result")?,
        single => single,
    };
    serde_json::from_value(result).ok()
}

/// A CLI in trouble can still print its JSON, and the words in that say more than an exit code does.
fn claude_reply(done: &Finished) -> Result<String> {
    let reply = parse_claude_reply(done.stdout.trim());
    if let Some(ClaudeReply {
        is_error: true,
        result,
    }) = &reply
    {
        return Err(cli_problem(
            &CLAUDE,
            result.as_deref().unwrap_or("no details"),
        ));
    }
    if !done.status.success() {
        return Err(cli_stopped_or_problem(&CLAUDE, done, &done.stderr));
    }
    reply
        .and_then(|r| r.result)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| cli_unreadable(&CLAUDE))
}

/// Turned off because a read-only sandbox does not stop them, and each one is a way for a title that
/// says "ignore your instructions" to reach out. Tried one by one against codex-cli 0.164.0. A key an
/// older CLI has never heard of is ignored, since `--strict-config` is not set.
const CODEX_TOOLS_OFF: [&str; 5] = [
    "features.shell_tool=false",
    "web_search=\"disabled\"",
    "features.apps=false",
    "features.image_generation=false",
    "features.view_image=false",
];

/// A TOML basic string. JSON's escapes are all valid in one, apart from DEL, which TOML wants escaped
/// and JSON leaves bare.
fn toml_string(text: &str) -> Result<String> {
    Ok(serde_json::to_string(text)?.replace('\u{7f}', "\\u007f"))
}

fn codex(
    model: Option<&str>,
    req: &CompletionRequest,
    timeout: Duration,
    path: &str,
) -> Result<String> {
    // The folder it works in stays empty, so what Rostery hands it and reads back lives apart from that.
    let (empty, files) = (tempfile::tempdir()?, tempfile::tempdir()?);
    let schema = files.path().join("schema.json");
    let reply = files.path().join("reply.txt");
    std::fs::write(&schema, serde_json::to_vec(&req.schema)?)?;
    let instructions = format!("developer_instructions={}", toml_string(&req.system)?);
    let configure = |cmd: &mut Command| {
        cmd.args(["exec", "--skip-git-repo-check", "--ephemeral"])
            .args(["--sandbox", "read-only"])
            .args(["--ignore-rules", "--ignore-user-config"]);
        for off in CODEX_TOOLS_OFF {
            cmd.args(["-c", off]);
        }
        cmd.arg("--output-schema")
            .arg(&schema)
            .arg("-o")
            .arg(&reply)
            .arg("-C")
            .arg(empty.path())
            .args(["-c", &instructions])
            .current_dir(empty.path());
        if let Some(model) = model {
            cmd.args(["-m", model]);
        }
        // The prompt is on stdin, because argv is visible in `ps`.
        cmd.arg("-");
    };
    let outcome = run_cli(&CODEX, &configure, Some(&req.user), timeout, path);
    let done = cli_finished(&CODEX, outcome, timeout)?;
    if !done.status.success() {
        return Err(cli_stopped_or_problem(
            &CODEX,
            &done,
            codex_error_line(&done.stderr),
        ));
    }
    let text = File::open(&reply)
        .and_then(|file| read_capped(&file))
        .unwrap_or_default();
    Some(text)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| cli_unreadable(&CODEX))
}

/// Its last error, or nothing. `codex exec` starts its stderr with a banner and the whole prompt, so
/// anything else from there could put the Digest in a message on screen.
fn codex_error_line(stderr: &str) -> &str {
    stderr
        .lines()
        .rev()
        .find_map(|line| {
            line.strip_prefix("ERROR: ")
                .or(line.strip_prefix("error: "))
        })
        .unwrap_or("")
}

/// No proxy from the environment and no redirect, because either could take a Digest somewhere the
/// screen says it does not go. A status code gets read here and not turned into an error by ureq.
fn local_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .max_redirects(0)
            .proxy(None)
            .build(),
    )
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{path}", base_url.trim_end_matches('/'))
}

fn read_body(
    response: &mut ureq::http::Response<ureq::Body>,
) -> std::result::Result<String, ureq::Error> {
    response
        .body_mut()
        .with_config()
        .limit(MAX_OUTPUT_BYTES)
        .read_to_string()
}

/// A server's complaint in its own words, from any of the shapes they come in.
fn error_detail(body: &str) -> String {
    let parsed = serde_json::from_str::<Value>(body).unwrap_or_default();
    let said = parsed["error"]["message"]
        .as_str()
        .or(parsed["error"].as_str())
        .unwrap_or(body);
    roast::clean(said, MAX_MESSAGE_CHARS)
}

fn local_unreachable(
    server: LocalServer,
    base_url: &str,
    e: &ureq::Error,
    timeout: Duration,
) -> Error {
    let label = server.label();
    Error::Provider(match e {
        ureq::Error::Timeout(_) => format!(
            "{label} at {base_url} did not finish within {}, so Rostery stopped waiting. A big model on \
             a slow computer can take that long, so try a smaller one, then try again.",
            how_long(timeout)
        ),
        ureq::Error::BodyExceedsLimit(_) => format!(
            "{label} at {base_url} gave a reply far longer than any Roast. Check that the address in \
             Settings is right, then try again."
        ),
        _ => format!(
            "Rostery could not reach {label} at {base_url}. Check that it is running and that the \
             address in Settings is right, then try again."
        ),
    })
}

fn local(
    server: LocalServer,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    req: &CompletionRequest,
    timeout: Duration,
) -> Result<String> {
    let label = server.label();
    if model.trim().is_empty() {
        return Err(Error::Provider(format!(
            "{label} has no model chosen. Choose one in Settings, then try again."
        )));
    }
    // No `response_format`, because some servers reject the kinds others accept. The prompt asks for
    // JSON, and the reply gets validated either way.
    let body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": req.system},
            {"role": "user", "content": req.user},
        ],
        "stream": false,
    });
    let mut request = local_agent(timeout)
        .post(endpoint(base_url, "chat/completions"))
        .header("Content-Type", "application/json");
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let failed = |e: ureq::Error| local_unreachable(server, base_url, &e, timeout);
    let mut response = request.send(body.to_string()).map_err(failed)?;
    let text = read_body(&mut response).map_err(failed)?;
    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "{label} at {base_url} answered with an error ({}): {}. Check the model and the address in \
             Settings, then try again.",
            response.status().as_u16(),
            error_detail(&text).trim_end_matches('.'),
        )));
    }
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|reply| reply["choices"][0]["message"]["content"].as_str().map(String::from))
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| {
            Error::Provider(format!(
                "{label} at {base_url} gave a reply Rostery could not read. Check that the address in \
                 Settings ends in /v1, then try again."
            ))
        })
}

#[derive(Debug, PartialEq)]
pub enum ModelsProblem {
    Status(u16),
    Unreachable,
}

/// What a server says it has, from `GET /models`.
pub fn list_models(
    base_url: &str,
    api_key: Option<&str>,
    timeout: Duration,
) -> std::result::Result<Vec<String>, ModelsProblem> {
    let mut request = local_agent(timeout).get(endpoint(base_url, "models"));
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let mut response = request.call().map_err(|_| ModelsProblem::Unreachable)?;
    if !response.status().is_success() {
        return Err(ModelsProblem::Status(response.status().as_u16()));
    }
    let text = read_body(&mut response).map_err(|_| ModelsProblem::Unreachable)?;
    // Something answers there, and it is not one of these servers if it has no list to give.
    let parsed = serde_json::from_str::<Value>(&text).unwrap_or_default();
    let data = parsed["data"]
        .as_array()
        .ok_or(ModelsProblem::Unreachable)?;
    Ok(data
        .iter()
        .filter_map(|model| model["id"].as_str())
        .map(|id| roast::clean(id, MAX_MODEL_NAME))
        .filter(|id| !id.is_empty())
        .take(MAX_MODELS)
        .collect())
}

/// The host a URL would really connect to, with `http://127.0.0.1@evil.example` taken as the second.
fn host_of(url: &str) -> Option<String> {
    url.trim().parse::<Uri>().ok()?.host().map(str::to_owned)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// What the person reads before anything goes anywhere.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Disclosure {
    pub sends_to: String,
    /// A local Provider on some other computer, so the words above are a warning.
    pub flagged: bool,
}

pub fn disclosure(p: &Provider) -> Disclosure {
    let (sends_to, flagged) = match p {
        Provider::ClaudeCode { .. } => (
            format!("Your Digest goes to Anthropic through Claude Code. {DIGEST_HOLDS}"),
            false,
        ),
        Provider::Codex { .. } => (
            format!("Your Digest goes to OpenAI through Codex. {DIGEST_HOLDS}"),
            false,
        ),
        Provider::Local {
            server, base_url, ..
        } => match host_of(base_url) {
            Some(host) if is_loopback(&host) => (
                format!(
                    "Your Digest stays on this computer. It goes to {} here at {base_url}. {DIGEST_HOLDS}",
                    server.label()
                ),
                false,
            ),
            host => {
                let host = host.as_deref().unwrap_or(base_url);
                (
                    format!(
                        "Warning: {host} is not this computer, so your Digest leaves this computer and \
                         goes there. {DIGEST_HOLDS}"
                    ),
                    true,
                )
            }
        },
    };
    Disclosure { sends_to, flagged }
}

/// A Provider's state right now. `signed_in` is true for a local server unless it wants a key.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub available: bool,
    pub signed_in: bool,
    /// One sentence, with the next step when something is missing.
    pub detail: String,
    pub sends_to: String,
    pub flagged: bool,
    /// What a local server has to offer.
    pub models: Vec<String>,
}

fn status_of(
    p: &Provider,
    available: bool,
    signed_in: bool,
    detail: String,
    models: Vec<String>,
) -> ProviderStatus {
    let Disclosure { sends_to, flagged } = disclosure(p);
    ProviderStatus {
        id: p.id().to_owned(),
        available,
        signed_in,
        detail,
        sends_to,
        flagged,
        models,
    }
}

/// Available, logged in, and what to say about it. `logged_in` reads a run that exited with 0.
fn cli_status(
    cli: &Cli,
    args: &[&str],
    logged_in: fn(&str) -> bool,
    path: &str,
) -> (bool, bool, String) {
    let configure = |cmd: &mut Command| {
        cmd.args(args).current_dir("/");
    };
    let label = cli.label;
    match run_cli(cli, &configure, None, CLI_STATUS_TIMEOUT, path) {
        Err(e) if e.kind() == ErrorKind::NotFound => (
            false,
            false,
            format!("Not installed. {}, then check again.", cli.install),
        ),
        Err(e) => (
            false,
            false,
            format!("Rostery could not start {label}: {e}. Check that it works in a terminal, then check again."),
        ),
        Ok(None) => (
            true,
            false,
            format!(
                "{label} did not answer within {}. Check that it works in a terminal, then check again.",
                how_long(CLI_STATUS_TIMEOUT)
            ),
        ),
        Ok(Some(done)) if done.status.success() && logged_in(&done.stdout) => {
            (true, true, "Logged in.".to_owned())
        }
        Ok(Some(done)) if says_too_old(&done.stderr.to_lowercase()) => (
            true,
            false,
            format!("{label} is too old for Rostery. {}, then check again.", cli.update),
        ),
        Ok(Some(_)) => (
            true,
            false,
            format!("Not logged in. {}, then check again.", cli.login),
        ),
    }
}

/// The status of a local server, which is whether it lists its models.
pub fn local_status(server: LocalServer, base_url: &str, api_key: Option<&str>) -> ProviderStatus {
    let p = Provider::Local {
        server,
        base_url: base_url.to_owned(),
        model: String::new(),
        api_key: None,
    };
    let label = server.label();
    match list_models(base_url, api_key, LOCAL_STATUS_TIMEOUT) {
        Ok(models) if models.is_empty() => status_of(
            &p,
            false,
            true,
            format!(
                "Running at {base_url}, with no models yet. {}, then check again.",
                server.how_to_get_a_model()
            ),
            models,
        ),
        Ok(models) => {
            let count = models.len();
            let unit = if count == 1 { "model" } else { "models" };
            status_of(
                &p,
                true,
                true,
                format!("Running at {base_url}, with {count} {unit}."),
                models,
            )
        }
        Err(ModelsProblem::Status(code @ (401 | 403))) => status_of(
            &p,
            false,
            false,
            format!(
                "The server at {base_url} answered {code}, so it wants an API key, and Rostery cannot \
                 send one yet. Turn that off in {label}, then check again."
            ),
            Vec::new(),
        ),
        Err(ModelsProblem::Status(code)) => status_of(
            &p,
            false,
            true,
            format!(
                "The server at {base_url} answered with an error ({code}). Check that the address ends \
                 in /v1, then check again."
            ),
            Vec::new(),
        ),
        Err(ModelsProblem::Unreachable) => status_of(
            &p,
            false,
            true,
            format!(
                "Not running at {base_url}. {}, then check again.",
                server.how_to_start()
            ),
            Vec::new(),
        ),
    }
}

fn claude_says_logged_in(stdout: &str) -> bool {
    serde_json::from_str::<Value>(stdout).is_ok_and(|reply| reply["loggedIn"] == true)
}

/// Each Provider's state, checked at the same time, so one that hangs costs no more than its timeout.
/// This spawns processes and calls the network, so it only ever runs on a worker thread.
pub fn statuses(settings: &Settings) -> Vec<ProviderStatus> {
    let path = child_path(login_shell_path());
    let path = path.as_str();
    let check = |p: &Provider| match p {
        Provider::ClaudeCode { .. } => {
            let (available, signed_in, detail) =
                cli_status(&CLAUDE, &["auth", "status"], claude_says_logged_in, path);
            status_of(p, available, signed_in, detail, Vec::new())
        }
        Provider::Codex { .. } => {
            let (available, signed_in, detail) =
                cli_status(&CODEX, &["login", "status"], |_| true, path);
            status_of(p, available, signed_in, detail, Vec::new())
        }
        Provider::Local {
            server,
            base_url,
            api_key,
            ..
        } => local_status(*server, base_url, api_key.as_deref()),
    };
    let setups = settings.in_card_order();
    std::thread::scope(|scope| {
        let running: Vec<_> = setups
            .iter()
            .map(|p| scope.spawn(move || check(p)))
            .collect();
        running
            .into_iter()
            .map(|one| {
                one.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    })
}

/// What the person chose on the Settings screen.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The one that writes the next Roast.
    pub provider: Provider,
    /// The voice, when it is not the default. `roast` always adds the fixed part to it.
    pub system_prompt: Option<String>,
    /// How each of the others was set up, so trying one does not lose another's address or model.
    #[serde(default)]
    pub others: Vec<Provider>,
}

fn every_provider() -> [Provider; 4] {
    let local = |server: LocalServer| Provider::Local {
        server,
        base_url: server.default_url().to_owned(),
        model: String::new(),
        api_key: None,
    };
    [
        Provider::ClaudeCode { model: None },
        Provider::Codex { model: None },
        local(LocalServer::Ollama),
        local(LocalServer::LmStudio),
    ]
}

impl Default for Settings {
    fn default() -> Settings {
        let [provider, others @ ..] = every_provider();
        Settings {
            provider,
            system_prompt: None,
            others: others.to_vec(),
        }
    }
}

/// A model name with space around it, or an empty one, is not a model name.
fn tidied_provider(mut p: Provider) -> Provider {
    match &mut p {
        Provider::ClaudeCode { model } | Provider::Codex { model } => {
            *model = model
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(String::from);
        }
        Provider::Local {
            base_url, model, ..
        } => {
            *base_url = base_url.trim().to_owned();
            *model = model.trim().to_owned();
        }
    }
    p
}

impl Settings {
    /// Each Provider exactly once in `others`, with the chosen one left out of it.
    fn tidied(self) -> Settings {
        let provider = tidied_provider(self.provider);
        let mut seen = vec![provider.id()];
        let others = self
            .others
            .into_iter()
            .chain(every_provider())
            .filter(|p| {
                let new = !seen.contains(&p.id());
                seen.push(p.id());
                new
            })
            .map(tidied_provider)
            .collect();
        Settings {
            provider,
            others,
            ..self
        }
    }

    /// `provider` and `others` in the order the cards go in.
    pub fn in_card_order(&self) -> Vec<&Provider> {
        let mut all: Vec<&Provider> = std::iter::once(&self.provider)
            .chain(&self.others)
            .collect();
        let order = every_provider();
        all.sort_by_key(|p| order.iter().position(|o| o.id() == p.id()));
        all
    }

    /// The editable part of the system prompt in use.
    pub fn voice(&self) -> &str {
        self.system_prompt
            .as_deref()
            .unwrap_or(roast::default_system_prompt())
    }

    /// What was saved. Settings that cannot be read are older or damaged, and nothing in them is worth
    /// a Roast that can never start, so they count as never set. The Roast screen says where a Digest
    /// is going before anything goes.
    pub fn load(store: &Store) -> Result<Settings> {
        match store.get_setting::<Settings>(SETTINGS_KEY) {
            Ok(saved) => Ok(saved.unwrap_or_default().tidied()),
            Err(Error::Json(_)) => Ok(Settings::default()),
            Err(e) => Err(e),
        }
    }

    /// Checks these, keeps them and hands back what was kept, which may read a little differently: a
    /// prompt that is the default, or has nothing in it, is the same as none.
    pub fn save(self, store: &Store) -> Result<Settings> {
        let mut settings = self.tidied();
        settings.system_prompt = settings.system_prompt.filter(|prompt| {
            !prompt.trim().is_empty() && prompt.trim() != roast::default_system_prompt().trim()
        });
        if let Some(prompt) = &settings.system_prompt {
            let chars = prompt.chars().count();
            if chars > MAX_SYSTEM_PROMPT_CHARS {
                return Err(Error::Settings(format!(
                    "That system prompt is {chars} characters long and the limit is \
                     {MAX_SYSTEM_PROMPT_CHARS}, so Rostery did not save it. Shorten it, or use Reset to \
                     go back to the default."
                )));
            }
        }
        if let Provider::Local { base_url, .. } = &settings.provider {
            check_address(base_url)?;
        }
        store.set_setting(SETTINGS_KEY, &settings)?;
        Ok(settings)
    }
}

/// Only an address that means one thing to this code and to the person reading it.
fn check_address(base_url: &str) -> Result<()> {
    let refused = || {
        Error::Settings(format!(
            "Rostery cannot use the address {base_url:?}. It should look like \
             http://127.0.0.1:11434/v1, with no name or password in it."
        ))
    };
    let uri = base_url.parse::<Uri>().map_err(|_| refused())?;
    let plain = matches!(uri.scheme_str(), Some("http" | "https"))
        && uri.host().is_some_and(|host| !host.is_empty())
        && !base_url.contains(['?', '#', '@']);
    if plain {
        Ok(())
    } else {
        Err(refused())
    }
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

static SHELL_ANSWER: OnceLock<String> = OnceLock::new();

/// The PATH of the person's login shell, plus any usual install folder that exists right now. An app
/// launched from Finder or the Dock has only `/usr/bin:/bin:/usr/sbin:/sbin`, and `claude` is not there.
pub fn login_shell_path() -> Option<String> {
    with_install_dirs(SHELL_ANSWER.get().cloned().or_else(ask_login_shell))
}

/// Only an answer gets kept. A shell that was slow or failed gets asked again next time.
// ponytail: the first answer stays, so a CLI only a newer answer knows costs one failed spawn and one
// more ask on each Roast. Use a Mutex here if that ever shows.
fn ask_login_shell() -> Option<String> {
    let answer = ask_shell(&login_shell(std::env::var_os("SHELL")), SHELL_TIMEOUT)?;
    let _ = SHELL_ANSWER.set(answer.clone());
    Some(answer)
}

/// An app launched some other way can be missing `SHELL`, and it still has a login shell.
fn login_shell(from_env: Option<OsString>) -> OsString {
    from_env
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else {
                "/bin/sh".into()
            }
        })
}

fn with_install_dirs(from_shell: Option<String>) -> Option<String> {
    let home = std::env::var_os("HOME");
    merged_with_install_dirs(from_shell, home.as_deref().map(Path::new))
}

/// `with_install_dirs` with HOME passed in, so a test needs no change to this process's environment.
fn merged_with_install_dirs(from_shell: Option<String>, home: Option<&Path>) -> Option<String> {
    let from_shell = from_shell.unwrap_or_default();
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
fn child_path(login: Option<String>) -> String {
    let login = login.unwrap_or_default();
    let inherited = std::env::var("PATH").unwrap_or_default();
    merge_paths([login.as_str(), inherited.as_str()])
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Asks this shell, with this HOME, and keeps nothing.
    fn login_path(shell: Option<&OsStr>, home: Option<&Path>, timeout: Duration) -> Option<String> {
        let from_shell = shell.and_then(|shell| ask_shell(shell, timeout));
        merged_with_install_dirs(from_shell, home)
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
    fn a_missing_shell_falls_back_to_the_usual_one() {
        let fallback = if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        };
        assert_eq!(login_shell(None), fallback);
        assert_eq!(login_shell(Some("".into())), fallback);
        assert_eq!(login_shell(Some("/bin/fish".into())), "/bin/fish");
    }

    #[test]
    fn provider_serializes_in_camel_case() {
        let local = Provider::Local {
            server: LocalServer::LmStudio,
            base_url: "u".into(),
            model: "m".into(),
            api_key: None,
        };
        assert_eq!(
            serde_json::to_value(&local).unwrap(),
            json!({"local": {"server": "lmStudio", "baseUrl": "u", "model": "m"}})
        );
        assert_eq!(
            serde_json::to_value(Provider::ClaudeCode { model: None }).unwrap(),
            json!({"claudeCode": {"model": null}})
        );
    }

    fn ollama(base_url: &str) -> Provider {
        Provider::Local {
            server: LocalServer::Ollama,
            base_url: base_url.into(),
            model: "tiny".into(),
            api_key: None,
        }
    }

    #[test]
    fn every_provider_has_its_card_id_label_and_default_url() {
        let ids: Vec<&str> = every_provider().iter().map(Provider::id).collect();
        assert_eq!(ids, ["claude-code", "codex", "ollama", "lm-studio"]);
        let labels: Vec<&str> = every_provider().iter().map(Provider::label).collect();
        assert_eq!(labels, ["Claude Code", "Codex", "Ollama", "LM Studio"]);
        assert_eq!(
            LocalServer::Ollama.default_url(),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            LocalServer::LmStudio.default_url(),
            "http://127.0.0.1:1234/v1"
        );
    }

    #[test]
    fn nothing_saved_means_claude_code_and_the_default_voice() {
        let store = Store::open_in_memory().unwrap();
        let settings = Settings::load(&store).unwrap();
        assert_eq!(settings.provider, Provider::ClaudeCode { model: None });
        assert_eq!(settings.voice(), roast::default_system_prompt());
        assert_eq!(settings.in_card_order().len(), 4);
    }

    #[test]
    fn settings_round_trip_with_every_provider_kept_once() {
        let store = Store::open_in_memory().unwrap();
        let chosen = ollama("http://192.168.1.5:11434/v1");
        let saved = Settings {
            provider: chosen.clone(),
            system_prompt: Some("Be gentle.".into()),
            // Repeats and gaps, as an older or hand-edited save could have.
            others: vec![
                ollama("http://127.0.0.1:9/v1"),
                Provider::Codex {
                    model: Some("  gpt-test ".into()),
                },
                Provider::Codex { model: None },
            ],
        }
        .save(&store)
        .unwrap();

        assert_eq!(Settings::load(&store).unwrap(), saved);
        assert_eq!(saved.provider, chosen);
        assert_eq!(saved.voice(), "Be gentle.");
        let ids: Vec<&str> = saved.in_card_order().iter().map(|p| p.id()).collect();
        assert_eq!(ids, ["claude-code", "codex", "ollama", "lm-studio"]);
        assert!(saved.others.contains(&Provider::Codex {
            model: Some("gpt-test".into())
        }));
    }

    #[test]
    fn an_api_key_never_reaches_the_settings_json() {
        let store = Store::open_in_memory().unwrap();
        let with_key = |server| Provider::Local {
            server,
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: "tiny".into(),
            api_key: Some("sk-very-secret".into()),
        };
        Settings {
            provider: with_key(LocalServer::Ollama),
            system_prompt: None,
            others: vec![with_key(LocalServer::LmStudio)],
        }
        .save(&store)
        .unwrap();
        let raw: Value = store.get_setting("settings").unwrap().unwrap();
        let raw = raw.to_string();
        assert!(!raw.contains("sk-very-secret"), "{raw}");
        assert!(!raw.contains("apiKey"), "{raw}");
    }

    #[test]
    fn settings_that_cannot_be_read_count_as_never_set() {
        let store = Store::open_in_memory().unwrap();
        store
            .set_setting("settings", &json!({"provider": "from the future"}))
            .unwrap();
        assert_eq!(Settings::load(&store).unwrap(), Settings::default());
    }

    #[test]
    fn a_system_prompt_too_long_says_so_and_changes_nothing() {
        let store = Store::open_in_memory().unwrap();
        let fine = Settings {
            system_prompt: Some("é".repeat(MAX_SYSTEM_PROMPT_CHARS)),
            ..Settings::default()
        };
        let kept = fine.save(&store).unwrap();

        let message = Settings {
            system_prompt: Some("é".repeat(MAX_SYSTEM_PROMPT_CHARS + 1)),
            ..Settings::default()
        }
        .save(&store)
        .unwrap_err()
        .to_string();

        assert!(message.contains("20001"), "{message}");
        assert!(message.contains("20000"), "{message}");
        assert!(message.contains("did not save"), "{message}");
        assert_eq!(Settings::load(&store).unwrap(), kept);
    }

    #[test]
    fn a_system_prompt_with_nothing_in_it_or_the_default_is_the_default() {
        let store = Store::open_in_memory().unwrap();
        for prompt in [
            "  \n ".to_owned(),
            format!("{}\n\n", roast::default_system_prompt()),
        ] {
            let saved = Settings {
                system_prompt: Some(prompt),
                ..Settings::default()
            }
            .save(&store)
            .unwrap();
            assert_eq!(saved.system_prompt, None);
        }
    }

    #[test]
    fn only_an_address_that_means_one_thing_is_kept() {
        let store = Store::open_in_memory().unwrap();
        let with = |url: &str| Settings {
            provider: ollama(url),
            ..Settings::default()
        };
        for good in [
            "http://127.0.0.1:11434/v1",
            " http://localhost:1234/v1/ ",
            "https://example.test/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(with(good).save(&store).is_ok(), "{good}");
        }
        for bad in [
            "",
            "127.0.0.1:11434",
            "ftp://127.0.0.1/v1",
            "not an address",
            "http://user:secret@127.0.0.1/v1",
            "http://127.0.0.1@example.test/v1",
            "http://127.0.0.1/v1?x=1",
            "http://127.0.0.1/v1#x",
        ] {
            let message = with(bad).save(&store).unwrap_err().to_string();
            assert!(
                message.contains("http://127.0.0.1:11434/v1"),
                "{bad}: {message}"
            );
        }
    }
}
