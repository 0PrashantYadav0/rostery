//! `deploy::vercel` against a fake Vercel API.

use mockito::{Matcher, Server, ServerGuard};
use rostery_core::deploy::vercel;
use rostery_core::export::SiteFile;
use serde_json::json;
use std::time::Duration;

const TOKEN: &str = "vercel_test";
const FAST: Duration = Duration::from_millis(1);

fn site() -> Vec<SiteFile> {
    let file = |path: &str, text: &str| SiteFile {
        path: path.into(),
        bytes: text.as_bytes().to_vec(),
    };
    vec![
        file("index.html", "<!doctype html>"),
        file("assets/share.js", "x"),
        file(".nojekyll", ""),
    ]
}

fn created(server: &mut ServerGuard, state: &str) -> mockito::Mock {
    server
        .mock("POST", "/v13/deployments")
        .match_query(Matcher::UrlEncoded("skipAutoDetectionConfirmation".into(), "1".into()))
        .match_header("authorization", "Bearer vercel_test")
        .with_status(200)
        .with_body(
            json!({"id": "dpl_1", "url": "r-abc.vercel.app", "readyState": state, "inspectorUrl": "https://vercel.com/o/r/dpl_1"})
                .to_string(),
        )
        .create()
}

#[test]
fn posts_inline_base64_files() {
    let mut server = Server::new();
    let post = server
        .mock("POST", "/v13/deployments")
        .match_query(Matcher::UrlEncoded("skipAutoDetectionConfirmation".into(), "1".into()))
        .match_body(Matcher::Json(json!({
            "name": "my-universe",
            "target": "production",
            "projectSettings": {"framework": null},
            "files": [
                {"file": "index.html", "data": "PCFkb2N0eXBlIGh0bWw+", "encoding": "base64"},
                {"file": "assets/share.js", "data": "eA==", "encoding": "base64"},
            ],
        })))
        .with_status(200)
        .with_body(r#"{"id":"dpl_1","url":"x.vercel.app","readyState":"READY","alias":["my-universe.vercel.app"]}"#)
        .create();
    vercel::publish(&server.url(), TOKEN, "My Universe", &site(), FAST).unwrap();
    post.assert();
}

#[test]
fn polls_until_ready() {
    let mut server = Server::new();
    let _post = created(&mut server, "QUEUED");
    let polls = [
        server
            .mock("GET", "/v13/deployments/dpl_1")
            .with_body(r#"{"readyState":"BUILDING"}"#)
            .expect(1)
            .create(),
        server
            .mock("GET", "/v13/deployments/dpl_1")
            .with_body(
                r#"{"readyState":"READY","url":"r-abc.vercel.app","alias":["r.vercel.app"]}"#,
            )
            .expect(1)
            .create(),
    ];
    let result = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST).unwrap();
    for poll in polls {
        poll.assert();
    }
    assert_eq!(result.url, "https://r.vercel.app");
    assert_eq!(result.dashboard_url, "https://vercel.com/o/r/dpl_1");
}

#[test]
fn error_state_is_reported() {
    let mut server = Server::new();
    let _post = created(&mut server, "QUEUED");
    let _poll = server
        .mock("GET", "/v13/deployments/dpl_1")
        .with_body(r#"{"readyState":"ERROR","errorMessage":"Too many files"}"#)
        .create();
    let message = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST)
        .unwrap_err()
        .to_string();
    assert!(message.contains("Too many files"), "{message}");
    assert!(
        message.contains("https://vercel.com/o/r/dpl_1"),
        "{message}"
    );
}

#[test]
fn poll_gives_up() {
    let mut server = Server::new();
    let _post = created(&mut server, "QUEUED");
    let _poll = server
        .mock("GET", "/v13/deployments/dpl_1")
        .with_body(r#"{"readyState":"BUILDING"}"#)
        .create();
    let message = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST)
        .unwrap_err()
        .to_string();
    assert!(message.contains("stopped waiting"), "{message}");
    assert!(
        message.contains("https://vercel.com/o/r/dpl_1"),
        "{message}"
    );
}

#[test]
fn uses_production_alias() {
    let mut server = Server::new();
    let _post = server
        .mock("POST", "/v13/deployments")
        .match_query(Matcher::Any)
        .with_body(r#"{"id":"d","url":"r-abc.vercel.app","readyState":"READY","alias":["r.vercel.app","r-o.vercel.app"]}"#)
        .create();
    let result = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST).unwrap();
    assert_eq!(result.url, "https://r.vercel.app");

    let mut server = Server::new();
    let _post = server
        .mock("POST", "/v13/deployments")
        .match_query(Matcher::Any)
        .with_body(r#"{"id":"d","url":"r-abc.vercel.app","readyState":"READY"}"#)
        .create();
    let result = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST).unwrap();
    assert_eq!(result.url, "https://r-abc.vercel.app");
    assert_eq!(result.dashboard_url, "https://vercel.com/dashboard");
}

#[test]
fn bad_token_says_reconnect() {
    for status in [401, 403] {
        let mut server = Server::new();
        let _post = server
            .mock("POST", "/v13/deployments")
            .match_query(Matcher::Any)
            .with_status(status)
            .create();
        let message = vercel::publish(&server.url(), TOKEN, "r", &site(), FAST)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("Connect Vercel again"),
            "{status}: {message}"
        );
    }
}

#[test]
fn a_token_is_checked_before_it_is_kept() {
    let mut server = Server::new();
    let _ok = server.mock("GET", "/v2/user").with_body("{}").create();
    assert!(vercel::accepts(&server.url(), TOKEN).unwrap());
    let mut server = Server::new();
    let _no = server.mock("GET", "/v2/user").with_status(403).create();
    assert!(!vercel::accepts(&server.url(), TOKEN).unwrap());
}

#[test]
fn project_name_is_normalised() {
    assert_eq!(
        vercel::project_name("  My Rostery Universe! "),
        "my-rostery-universe"
    );
    assert_eq!(vercel::project_name("a//b__c..d"), "a-b__c..d");
    assert_eq!(vercel::project_name(&"x".repeat(150)).len(), 100);
    assert_eq!(vercel::project_name("!!!"), "rostery-universe");
}

#[test]
fn a_project_rostery_did_not_make_is_refused() {
    let mut server = Server::new();
    let found = server
        .mock("GET", "/v9/projects/portfolio")
        .with_body("{}")
        .create();
    let message = vercel::check_name(&server.url(), TOKEN, "Portfolio", &[])
        .unwrap_err()
        .to_string();
    assert!(message.contains("did not make it"), "{message}");
    found.assert();

    let _missing = server
        .mock("GET", "/v9/projects/fresh")
        .with_status(404)
        .create();
    assert_eq!(
        vercel::check_name(&server.url(), TOKEN, "Fresh", &[]).unwrap(),
        "fresh"
    );

    // One Rostery published to before is not asked about.
    let asked = server.mock("GET", "/v9/projects/mine").expect(0).create();
    assert_eq!(
        vercel::check_name(&server.url(), TOKEN, "mine", &["mine".to_owned()]).unwrap(),
        "mine"
    );
    asked.assert();
}
