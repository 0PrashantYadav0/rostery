//! The real `llm::complete` and friends against `mockito`, the way a local server would answer.

use mockito::{Matcher, Server};
use rostery_core::llm::{self, CompletionRequest, LocalServer, Provider};
use serde_json::json;
use std::io::Read;
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

const LONG_ENOUGH: Duration = Duration::from_secs(20);

fn request() -> CompletionRequest {
    CompletionRequest {
        system: "Be mean".into(),
        user: "<digest>{}</digest>".into(),
        schema: json!({"type": "object"}),
    }
}

fn local(base_url: &str) -> Provider {
    Provider::Local {
        server: LocalServer::Ollama,
        base_url: base_url.into(),
        model: "tiny-model".into(),
        api_key: None,
    }
}

fn base(server: &Server) -> String {
    format!("{}/v1", server.url())
}

fn answer(text: &str) -> String {
    json!({"choices": [{"message": {"role": "assistant", "content": text}}]}).to_string()
}

#[test]
fn local_posts_chat_completion() {
    let mut server = Server::new();
    // The whole body, so `response_format` cannot be there: LM Studio rejects `json_object`.
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_header("content-type", "application/json")
        .match_header("authorization", Matcher::Missing)
        .match_body(Matcher::Json(json!({
            "model": "tiny-model",
            "messages": [
                {"role": "system", "content": "Be mean"},
                {"role": "user", "content": "<digest>{}</digest>"},
            ],
            "stream": false,
        })))
        .with_body(answer("hello"))
        .create();

    let reply = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH).unwrap();

    assert_eq!(reply, "hello");
    mock.assert();
}

#[test]
fn a_slash_at_the_end_of_the_address_changes_nothing() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_body(answer("hello"))
        .create();
    let url = format!("{}/", base(&server));
    assert_eq!(
        llm::complete(&local(&url), &request(), LONG_ENOUGH).unwrap(),
        "hello"
    );
    mock.assert();
}

#[test]
fn local_http_error_is_readable() {
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(500)
        .with_body(r#"{"error":{"message":"the model runner crashed"}}"#)
        .create();
    let url = base(&server);
    let message = llm::complete(&local(&url), &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains(&url), "{message}");
    assert!(message.contains("the model runner crashed"), "{message}");
    assert!(message.contains("500"), "{message}");
    assert!(message.contains("try again"), "{message}");

    // Nothing listens on port 1.
    let url = "http://127.0.0.1:1/v1";
    let message = llm::complete(&local(url), &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains(url), "{message}");
    assert!(message.contains("Ollama"), "{message}");
    assert!(message.contains("running"), "{message}");
}

#[test]
fn an_error_body_of_any_shape_is_cut_and_cleaned() {
    for (body, expected) in [
        (r#"{"error":"model not found"}"#, "model not found"),
        ("plain words from a proxy", "plain words from a proxy"),
    ] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(404)
            .with_body(body)
            .create();
        let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains(expected), "{message}");
    }

    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(500)
        .with_body(format!("\u{1b}[31m{}", "x".repeat(50_000)))
        .create();
    let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.len() < 1000, "{} bytes", message.len());
    assert!(!message.contains('\u{1b}'));
}

#[test]
fn local_api_key_sent_when_present() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer sk-from-the-keychain")
        .with_body(answer("hello"))
        .create();
    let provider = Provider::Local {
        server: LocalServer::Ollama,
        base_url: base(&server),
        model: "tiny-model".into(),
        api_key: Some("sk-from-the-keychain".into()),
    };
    llm::complete(&provider, &request(), LONG_ENOUGH).unwrap();
    mock.assert();
}

#[test]
fn a_reply_with_no_text_says_what_to_check() {
    for body in [
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":null}}]}"#,
        r#"{"choices":[{"message":{"content":"  "}}]}"#,
        "<html>a web page, not an API</html>",
    ] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_body(body)
            .create();
        let url = base(&server);
        let message = llm::complete(&local(&url), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains("could not read"), "{message}");
        assert!(message.contains(&url), "{message}");
    }
}

#[test]
fn a_flood_from_a_server_is_not_read_to_the_end() {
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_body("x".repeat(5 << 20))
        .create();
    let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains("far longer"), "{message}");
}

#[test]
fn a_local_provider_with_no_model_says_to_choose_one_and_sends_nothing() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .with_body(answer("hello"))
        .create();
    let provider = Provider::Local {
        server: LocalServer::LmStudio,
        base_url: base(&server),
        model: "  ".into(),
        api_key: None,
    };
    let message = llm::complete(&provider, &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains("LM Studio"), "{message}");
    assert!(message.contains("Settings"), "{message}");
    mock.assert();
}

/// A redirect could take the Digest off a server that was promised to keep it on this computer.
#[test]
fn a_redirect_is_not_followed() {
    let mut elsewhere = Server::new();
    let never = elsewhere.mock("POST", Matcher::Any).expect(0).create();
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(307)
        .with_header("location", &format!("{}/stolen", elsewhere.url()))
        .create();
    let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains("307"), "{message}");
    never.assert();
}

/// A server that takes the request and never answers, as a stuck model does.
#[test]
fn a_server_that_never_answers_costs_only_the_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        for stream in listener.incoming().flatten() {
            let mut buffer = [0; 1024];
            let mut stream = stream;
            let _ = stream.read(&mut buffer);
            kept.push(stream);
        }
    });
    let started = Instant::now();
    let message = llm::complete(&local(&url), &request(), Duration::from_secs(1))
        .unwrap_err()
        .to_string();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(message.contains("did not finish"), "{message}");
    assert!(message.contains(&url), "{message}");
}

#[test]
fn local_lists_models() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/v1/models")
        .with_body(r#"{"object":"list","data":[{"id":"small\u0000"},{"id":"big"},{"no":"id"}]}"#)
        .create();
    let models = llm::list_models(&base(&server), None, LONG_ENOUGH).unwrap();
    assert_eq!(models, ["small", "big"]);
    mock.assert();
}

#[test]
fn local_status_says_what_the_server_said() {
    let ollama = |url: &str| llm::local_status(LocalServer::Ollama, url, None);

    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"tiny"},{"id":"big"}]}"#)
        .create();
    let up = ollama(&base(&server));
    assert_eq!(up.id, "ollama");
    assert!(up.available && up.signed_in, "{up:?}");
    assert_eq!(up.models, ["tiny", "big"]);
    assert!(up.detail.contains("2 models"), "{up:?}");

    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[]}"#)
        .create();
    let empty = ollama(&base(&server));
    assert!(!empty.available, "{empty:?}");
    assert!(empty.detail.contains("ollama pull"), "{empty:?}");

    let down = ollama("http://127.0.0.1:1/v1");
    assert!(!down.available, "{down:?}");
    assert!(down.detail.contains("http://127.0.0.1:1/v1"), "{down:?}");
    assert!(down.detail.contains("ollama serve"), "{down:?}");
    assert!(down.detail.contains("check again"), "{down:?}");

    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body("<html>some other web page</html>")
        .create();
    let other = ollama(&base(&server));
    assert!(!other.available, "{other:?}");
    assert!(other.detail.starts_with("Not running at"), "{other:?}");

    let mut server = Server::new();
    server.mock("GET", "/v1/models").with_status(401).create();
    let locked = ollama(&base(&server));
    assert!(!locked.signed_in, "{locked:?}");
    assert!(locked.detail.contains("API key"), "{locked:?}");
}

#[test]
fn a_status_check_that_hangs_costs_two_seconds() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        for stream in listener.incoming().flatten() {
            kept.push(stream);
        }
    });
    let started = Instant::now();
    let status = llm::local_status(LocalServer::Ollama, &url, None);
    assert!(
        (Duration::from_millis(1800)..Duration::from_secs(4)).contains(&started.elapsed()),
        "took {:?}",
        started.elapsed()
    );
    assert!(!status.available, "{status:?}");
    assert!(status.detail.contains(&url), "{status:?}");
}

#[test]
fn non_loopback_is_flagged() {
    let far = llm::disclosure(&local("http://192.168.1.5:11434/v1"));
    assert!(far.flagged, "{far:?}");
    assert!(far.sends_to.contains("192.168.1.5"), "{far:?}");
    assert!(far.sends_to.contains("leaves this computer"), "{far:?}");
    assert!(far.sends_to.contains("titles"), "{far:?}");

    let near = llm::disclosure(&local("http://127.0.0.1:11434/v1"));
    assert!(!near.flagged, "{near:?}");
    assert!(near.sends_to.contains("stays on this computer"), "{near:?}");
    assert!(!near.sends_to.contains("leaves"), "{near:?}");
    assert!(near.sends_to.contains("titles"), "{near:?}");
}

/// Each row is an address, and whether the Digest would stay on this computer.
#[test]
fn only_a_real_loopback_address_counts_as_this_computer() {
    for (url, stays) in [
        ("http://127.0.0.1:11434/v1", true),
        ("http://127.5.5.5:1234/v1", true),
        ("http://localhost:1234/v1", true),
        ("HTTP://LOCALHOST:1234/v1", true),
        ("http://[::1]:11434/v1", true),
        ("http://192.168.1.5:11434/v1", false),
        ("http://0.0.0.0:11434/v1", false),
        ("http://127.0.0.1.evil.example/v1", false),
        ("http://localhost.evil.example/v1", false),
        ("http://127.0.0.1@evil.example/v1", false),
        ("https://evil.example/v1", false),
        ("not an address at all", false),
    ] {
        assert_eq!(!llm::disclosure(&local(url)).flagged, stays, "{url}");
    }
    let named = llm::disclosure(&local("http://127.0.0.1@evil.example:80/v1"));
    assert!(named.sends_to.contains("evil.example"), "{named:?}");
}

/// A proxy on a loopback call would take the Digest somewhere the screen says it does not go. This
/// sets the proxy variables, so it runs in a process of its own.
#[test]
fn a_proxy_in_the_environment_is_not_used() {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "with_a_proxy_set", "--ignored", "--nocapture"])
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("http_proxy", "http://127.0.0.1:1")
        .env("ROSTERY_TEST_PROXY", "1")
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&child.stdout);
    assert!(
        child.status.success() && out.contains("1 passed"),
        "{out}\n{}",
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
#[ignore = "run by a_proxy_in_the_environment_is_not_used"]
fn with_a_proxy_set() {
    if std::env::var_os("ROSTERY_TEST_PROXY").is_none() {
        return;
    }
    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_body(answer("hello"))
        .create();
    let reply = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH).unwrap();
    assert_eq!(reply, "hello");
}

#[test]
fn local_gets_more_time_than_a_cli() {
    assert_eq!(
        local("http://127.0.0.1:1/v1").timeout(),
        Duration::from_secs(600)
    );
    assert_eq!(
        Provider::Codex { model: None }.timeout(),
        Duration::from_secs(300)
    );
    assert_eq!(
        Provider::ClaudeCode { model: None }.timeout(),
        Duration::from_secs(300)
    );
}
