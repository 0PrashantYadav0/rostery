//! The real `llm::complete` and friends against `mockito`, the way a local server would answer.

use mockito::{Matcher, Server};
use rostery_core::llm::{self, CompletionRequest, Disclosure, LocalServer, Provider, Settings};
use rostery_core::store::Store;
use rostery_core::{demo, digest, roast};
use serde_json::{json, Value};
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

fn disclosure(provider: Provider) -> Disclosure {
    Settings {
        provider,
        ..Settings::default()
    }
    .disclosure()
}

fn with_model(model: &str) -> Provider {
    Provider::Local {
        server: LocalServer::Ollama,
        base_url: "http://127.0.0.1:11434/v1".into(),
        model: model.into(),
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

/// What a person reads when the server's own words are of no use: nothing sent, a web page from
/// whatever is really at that address, or bytes that are not text. Each still has to be a sentence.
#[test]
fn a_failure_with_nothing_to_quote_still_reads_as_a_sentence() {
    let page = "<!DOCTYPE html>\n<html><body><h1>502 Bad Gateway</h1></body></html>\n";
    let bodies: [(usize, &[u8]); 5] = [
        (500, b""),
        (500, b"   \n"),
        (502, page.as_bytes()),
        (500, b"\xff\xfe\xfd\xfc"),
        (503, b"<p>Service Unavailable</p>"),
    ];
    for (status, body) in bodies {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(status)
            .with_body(body)
            .create();
        let url = base(&server);
        let message = llm::complete(&local(&url), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains(&url), "{message}");
        assert!(message.contains(&status.to_string()), "{message}");
        assert!(message.contains("gave no reason"), "{message}");
        assert!(message.ends_with("try again."), "{message}");
        for broken in [": .", "<", "\u{fffd}", "\n"] {
            assert!(!message.contains(broken), "{broken:?} in {message}");
        }
    }
}

#[test]
fn a_redirect_says_it_is_a_redirect_and_why_it_was_not_followed() {
    for status in [301, 302, 307, 308] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(status)
            .with_header("location", "https://elsewhere.example/v1/chat/completions")
            .with_body("<html>Moved</html>")
            .create();
        let url = base(&server);
        let message = llm::complete(&local(&url), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains(&url), "{message}");
        assert!(message.contains(&status.to_string()), "{message}");
        assert!(message.contains("redirect"), "{message}");
        assert!(message.contains("does not follow"), "{message}");
        assert!(message.ends_with("try again."), "{message}");
        assert!(!message.contains("elsewhere.example"), "{message}");
        assert!(!message.contains('<'), "{message}");
    }
}

/// It answered, so it can be reached, whatever it answered with.
#[test]
fn a_success_that_is_not_text_or_not_json_is_a_reply_that_could_not_be_read() {
    let bodies: [&[u8]; 3] = [b"\xff\xfe\xfd\xfc", b"<html>a web page</html>", b""];
    for body in bodies {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_body(body)
            .create();
        let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains("could not read"), "{message}");
        assert!(!message.contains("could not reach"), "{message}");
        assert!(message.ends_with("try again."), "{message}");
    }
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

/// The demo Digest with the default voice is 19,900 characters, and the longest there can be today
/// with that voice is 72,500.
fn request_of(chars: usize) -> CompletionRequest {
    CompletionRequest {
        system: "x".repeat(chars / 2),
        user: "y".repeat(chars / 2),
        schema: json!({}),
    }
}

fn counted(tokens: Value) -> String {
    json!({
        "choices": [{"message": {"role": "assistant", "content": "a fine looking reply"}}],
        "usage": {"prompt_tokens": tokens},
    })
    .to_string()
}

/// Ollama answers 200 with a reply that can look fine after it drops the start of a prompt its context
/// cannot hold. Today it keeps about half the context, 2051 tokens of 4096. Its older runners kept all
/// of it. Both are from its source.
#[test]
fn a_prompt_the_server_cut_says_so_and_how_to_fix_it() {
    for (chars, tokens) in [
        (19_900, 2051),
        (19_900, 4096),
        (72_500, 4098),
        (72_500, 16_384),
    ] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_body(counted(json!(tokens)))
            .create();
        let url = base(&server);
        let message = llm::complete(&local(&url), &request_of(chars), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains(&url), "{message}");
        assert!(message.contains(&tokens.to_string()), "{message}");
        assert!(message.contains("context length"), "{message}");
        assert!(message.contains("OLLAMA_CONTEXT_LENGTH"), "{message}");
        assert!(message.contains("slider"), "{message}");
    }

    let mut server = Server::new();
    server
        .mock("POST", "/v1/chat/completions")
        .with_body(counted(json!(2051)))
        .create();
    let lm_studio = Provider::Local {
        server: LocalServer::LmStudio,
        base_url: base(&server),
        model: "tiny-model".into(),
        api_key: None,
    };
    let message = llm::complete(&lm_studio, &request_of(19_900), LONG_ENOUGH)
        .unwrap_err()
        .to_string();
    assert!(message.contains("LM Studio"), "{message}");
    assert!(message.contains("context length"), "{message}");
    assert!(!message.contains("OLLAMA"), "{message}");
}

/// A wrong refusal costs someone a Roast that was fine, so these have to pass.
#[test]
fn a_prompt_read_whole_is_not_called_cut() {
    let hindi = CompletionRequest {
        system: "क".repeat(20_000),
        user: "y".repeat(3_000),
        schema: json!({}),
    };
    for (request, tokens) in [
        // About 3.75 and 5 characters to a token, which is JSON and plain prose.
        (request_of(19_900), json!(5300)),
        (request_of(19_900), json!(3980)),
        // More than a token holds in bytes, but not in characters.
        (hindi, json!(9500)),
        // A prompt that holds less than a context, with room to spare.
        (request_of(19_900), json!(32_768)),
        // Nothing to go on.
        (request_of(19_900), json!(0)),
        (request_of(19_900), json!(null)),
    ] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_body(counted(tokens))
            .create();
        let reply = llm::complete(&local(&base(&server)), &request, LONG_ENOUGH);
        assert_eq!(reply.unwrap(), "a fine looking reply");
    }
}

/// In the words of llama-server, seen from a real one, and of Ollama, from its source.
#[test]
fn a_prompt_the_server_refused_as_too_long_says_how_to_fix_it() {
    for body in [
        r#"{"error":{"code":400,"message":"request (4867 tokens) exceeds the available context size (512 tokens), try increasing it","type":"exceed_context_size_error","n_prompt_tokens":4867,"n_ctx":512}}"#,
        r#"{"error":{"message":"the prompt is longer than the context length currently available to the model; shorten the prompt, adjust the context length in settings, or use a model with a longer context length"}}"#,
    ] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(400)
            .with_body(body)
            .create();
        let message = llm::complete(&local(&base(&server)), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains("400"), "{message}");
        assert!(message.contains("OLLAMA_CONTEXT_LENGTH"), "{message}");
        assert!(!message.contains("Check the model"), "{message}");
    }
}

/// Settings says this about such a server, and a Roast has to say the same and not send someone to
/// check a model that is fine.
#[test]
fn a_server_that_wants_a_key_says_so_when_a_roast_asks() {
    for code in [401, 403] {
        let mut server = Server::new();
        server
            .mock("POST", "/v1/chat/completions")
            .with_status(code)
            .with_body(r#"{"error":{"message":"invalid api key"}}"#)
            .create();
        let url = base(&server);
        let message = llm::complete(&local(&url), &request(), LONG_ENOUGH)
            .unwrap_err()
            .to_string();
        assert!(message.contains(&url), "{message}");
        assert!(message.contains(&code.to_string()), "{message}");
        assert!(message.contains("Add the key under"), "{message}");
        assert!(message.contains("then try again"), "{message}");
        assert!(!message.contains("Check the model"), "{message}");
    }
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
    let ollama = |url: &str| llm::local_status(LocalServer::Ollama, url, "", None);

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

/// An older Ollama with nothing pulled yet says `null` where a newer one says `[]`.
#[test]
fn an_empty_list_written_as_null_is_a_server_with_no_models() {
    for body in [r#"{"object":"list","data":null}"#, r#"{"data":null}"#] {
        let mut server = Server::new();
        server.mock("GET", "/v1/models").with_body(body).create();
        assert_eq!(
            llm::list_models(&base(&server), None, LONG_ENOUGH).unwrap(),
            [] as [&str; 0],
            "{body}"
        );
        let status = llm::local_status(LocalServer::Ollama, &base(&server), "", None);
        assert!(!status.available, "{status:?}");
        assert!(status.detail.contains("no models yet"), "{status:?}");
        assert!(status.detail.contains("ollama pull"), "{status:?}");
    }

    // With no `data` at all, this is some other server.
    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"status":"ok"}"#)
        .create();
    let other = llm::local_status(LocalServer::Ollama, &base(&server), "", None);
    assert!(other.detail.starts_with("Not running at"), "{other:?}");
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
    let status = llm::local_status(LocalServer::Ollama, &url, "", None);
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
    let far = disclosure(local("http://192.168.1.5:11434/v1"));
    assert!(far.flagged, "{far:?}");
    assert!(far.sends_to.contains("192.168.1.5"), "{far:?}");
    assert!(far.sends_to.contains("leaves this computer"), "{far:?}");
    assert!(far.sends_to.contains("titles"), "{far:?}");

    let near = disclosure(local("http://127.0.0.1:11434/v1"));
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
        assert_eq!(!disclosure(local(url)).flagged, stays, "{url}");
    }
    let named = disclosure(local("http://127.0.0.1@evil.example:80/v1"));
    assert!(named.sends_to.contains("evil.example"), "{named:?}");
}

/// Ollama serves these through the same local port and runs them on ollama.com. Its own rule, in
/// `internal/modelref`, ignores case. A name that only has the word in it stays here.
#[test]
fn a_cloud_model_on_a_loopback_address_leaves_this_computer() {
    for model in [
        "gpt-oss:20b-cloud",
        "gemma4:cloud",
        "GEMMA4:CLOUD",
        " gemma4:cloud ",
        "kimi-k2-cloud",
    ] {
        let said = disclosure(with_model(model));
        assert!(said.flagged, "{model}: {said:?}");
        assert!(said.sends_to.contains(model.trim()), "{said:?}");
        assert!(said.sends_to.contains("leaves this computer"), "{said:?}");
        assert!(said.sends_to.contains("ollama.com"), "{said:?}");
        assert!(said.sends_to.contains("titles"), "{said:?}");
        assert!(!said.sends_to.contains("stays on"), "{said:?}");
    }
    for model in ["", "llama3:8b", "cloudy:7b", "cloud", "cloud-9:latest"] {
        let said = disclosure(with_model(model));
        assert!(!said.flagged, "{model}: {said:?}");
        assert!(said.sends_to.contains("stays on this computer"), "{said:?}");
    }
}

/// The screen fills in the first model offered, so a cloud one must never be offered.
#[test]
fn local_status_offers_no_cloud_model_and_says_where_a_saved_one_goes() {
    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"gpt-oss:20b-cloud"},{"id":"tiny"},{"id":"gemma4:cloud"}]}"#)
        .create();
    let url = base(&server);

    let nothing_saved = llm::local_status(LocalServer::Ollama, &url, "", None);
    assert_eq!(nothing_saved.models, ["tiny"]);
    assert!(nothing_saved.available, "{nothing_saved:?}");
    assert!(
        nothing_saved.detail.contains("1 model."),
        "{nothing_saved:?}"
    );
    assert!(
        nothing_saved.detail.contains("ollama.com"),
        "{nothing_saved:?}"
    );
    assert!(
        nothing_saved.sends_to.contains("stays on"),
        "{nothing_saved:?}"
    );

    let saved = llm::local_status(LocalServer::Ollama, &url, "gpt-oss:20b-cloud", None);
    assert!(saved.sends_to.starts_with("Warning"), "{saved:?}");
    assert!(saved.sends_to.contains("ollama.com"), "{saved:?}");
    assert!(!saved.sends_to.contains("stays on"), "{saved:?}");

    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"gemma4:cloud"}]}"#)
        .create();
    let only_cloud = llm::local_status(LocalServer::Ollama, &base(&server), "", None);
    assert!(only_cloud.models.is_empty(), "{only_cloud:?}");
    assert!(!only_cloud.available, "{only_cloud:?}");
    assert!(only_cloud.detail.contains("ollama pull"), "{only_cloud:?}");
    assert!(only_cloud.detail.contains("ollama.com"), "{only_cloud:?}");

    // With no cloud model to speak of, the words say nothing about them.
    let mut server = Server::new();
    server
        .mock("GET", "/v1/models")
        .with_body(r#"{"data":[{"id":"tiny"}]}"#)
        .create();
    let none = llm::local_status(LocalServer::Ollama, &base(&server), "", None);
    assert!(!none.detail.contains("ollama.com"), "{none:?}");
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

/// Its own clock, so that a stuck tag step cannot use up the wait before a Roast.
#[test]
fn the_tag_step_gets_less_time_than_a_roast() {
    assert_eq!(
        local("http://127.0.0.1:1/v1").tag_timeout(),
        Duration::from_secs(300)
    );
    assert_eq!(
        Provider::Codex { model: None }.tag_timeout(),
        Duration::from_secs(120)
    );
    assert_eq!(
        Provider::ClaudeCode { model: None }.tag_timeout(),
        Duration::from_secs(120)
    );
}

/// Names go to the Provider as well as the Digest, wherever that is, so every sentence has to say so.
#[test]
fn every_disclosure_says_creator_names_go_too() {
    for provider in [
        Provider::ClaudeCode { model: None },
        Provider::Codex { model: None },
        local("http://127.0.0.1:11434/v1"),
        local("http://192.168.1.5:11434/v1"),
        with_model("gemma4:cloud"),
    ] {
        let said = disclosure(provider).sends_to;
        assert!(said.contains("80 Creator names"), "{said}");
        assert!(said.contains("Source"), "{said}");
    }
}

/// Rostery cannot stop this one, so the person has to be told. ADR 0003 has what was tried.
#[test]
fn only_the_codex_disclosure_mentions_its_agents_file() {
    let said = disclosure(Provider::Codex { model: None }).sends_to;
    assert!(said.contains("AGENTS.md"), "{said}");
    let said = disclosure(Provider::ClaudeCode { model: None }).sends_to;
    assert!(!said.contains("AGENTS.md"), "{said}");
}

/// The model a live test asks the real Ollama on this computer for, which must already be pulled.
fn live_ollama() -> Provider {
    Provider::Local {
        server: LocalServer::Ollama,
        base_url: LocalServer::Ollama.default_url().into(),
        model: std::env::var("ROSTERY_OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:1.5b".into()),
        api_key: None,
    }
}

fn live_ollama_roast_of_demo() -> rostery_core::Result<roast::Roast> {
    const NOW: i64 = 1_800_000_000;
    let mut store = Store::open_in_memory().unwrap();
    demo::load(&mut store, NOW).unwrap();
    let digest = digest::from_store(&store, NOW).unwrap();
    roast::generate(
        &live_ollama(),
        &digest,
        roast::default_system_prompt(),
        llm::DEFAULT_TIMEOUT,
    )
}

/// Needs `OLLAMA_CONTEXT_LENGTH=32768 ollama serve`, the way the error below tells a person to start it.
#[test]
#[ignore = "talks to the real Ollama, started with a long context"]
fn live_ollama_roast() {
    let started = Instant::now();
    let roast = live_ollama_roast_of_demo().unwrap();
    println!("took {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&roast).unwrap());
    assert!(!roast.burns.is_empty() && !roast.headline.is_empty());
}

/// Needs a plain `ollama serve`, whose default context is too short for a Roast.
#[test]
#[ignore = "talks to the real Ollama, started with its default context"]
fn live_ollama_short_context_says_how_to_raise_it() {
    let said = live_ollama_roast_of_demo().unwrap_err().to_string();
    println!("{said}");
    assert!(said.contains("OLLAMA_CONTEXT_LENGTH=32768"), "{said}");
}
