//! `deploy::github` against a fake GitHub API.

use mockito::{Matcher, Mock, Server, ServerGuard};
use rostery_core::deploy::github::{self, MARKER};
use rostery_core::export::SiteFile;
use serde_json::json;

const TOKEN: &str = "ghp_test";

fn site() -> Vec<SiteFile> {
    vec![
        SiteFile {
            path: "index.html".into(),
            bytes: b"<!doctype html>".to_vec(),
        },
        SiteFile {
            path: "assets/share.js".into(),
            bytes: b"console.log(1)".to_vec(),
        },
    ]
}

fn user(server: &mut ServerGuard) -> Mock {
    server
        .mock("GET", "/user")
        .match_header("authorization", "Bearer ghp_test")
        .match_header("accept", "application/vnd.github+json")
        .match_header("x-github-api-version", "2022-11-28")
        .with_status(200)
        .with_body(r#"{"login":"o"}"#)
        .create()
}

/// Everything after the repo exists: the branch, blobs, tree, commit, ref and Pages.
fn push(server: &mut ServerGuard) -> Vec<Mock> {
    push_with_pages(server, 201)
}

fn push_with_pages(server: &mut ServerGuard, pages: usize) -> Vec<Mock> {
    vec![
        server
            .mock("GET", "/repos/o/r/git/ref/heads/main")
            .with_status(200)
            .with_body(r#"{"object":{"sha":"base"}}"#)
            .create(),
        server
            .mock("POST", "/repos/o/r/git/blobs")
            .match_body(Matcher::PartialJson(json!({"encoding": "base64"})))
            .with_status(201)
            .with_body(r#"{"sha":"blob"}"#)
            .expect(2)
            .create(),
        server
            .mock("POST", "/repos/o/r/git/trees")
            .match_body(Matcher::Json(json!({"tree": [
                {"path": "index.html", "mode": "100644", "type": "blob", "sha": "blob"},
                {"path": "assets/share.js", "mode": "100644", "type": "blob", "sha": "blob"},
            ]})))
            .with_status(201)
            .with_body(r#"{"sha":"tree"}"#)
            .create(),
        server
            .mock("POST", "/repos/o/r/git/commits")
            .match_body(Matcher::PartialJson(json!({"tree": "tree", "parents": []})))
            .with_status(201)
            .with_body(r#"{"sha":"commit"}"#)
            .create(),
        server
            .mock("PATCH", "/repos/o/r/git/refs/heads/main")
            .match_body(Matcher::Json(json!({"sha": "commit", "force": true})))
            .with_status(200)
            .with_body("{}")
            .create(),
        server
            .mock("POST", "/repos/o/r/pages")
            .match_body(Matcher::Json(
                json!({"source": {"branch": "main", "path": "/"}}),
            ))
            .with_status(pages)
            .with_body(r#"{"message":"GitHub Pages is already enabled."}"#)
            .create(),
    ]
}

fn existing(server: &mut ServerGuard, description: &str) -> Mock {
    server
        .mock("GET", "/repos/o/r")
        .with_status(200)
        .with_body(json!({"description": description, "default_branch": "main"}).to_string())
        .create()
}

#[test]
fn creates_repo_when_missing() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let missing = server.mock("GET", "/repos/o/r").with_status(404).create();
    let create = server
        .mock("POST", "/user/repos")
        .match_body(Matcher::PartialJson(
            json!({"name": "r", "auto_init": true}),
        ))
        .match_body(Matcher::Regex(MARKER.into()))
        .with_status(201)
        .with_body(r#"{"default_branch":"main"}"#)
        .create();
    let pushed = push(&mut server);

    let result = github::publish(&server.url(), TOKEN, "r", &site()).unwrap();

    missing.assert();
    create.assert();
    for mock in pushed {
        mock.assert();
    }
    assert_eq!(result.url, "https://o.github.io/r/");
    assert_eq!(result.dashboard_url, "https://github.com/o/r");
}

#[test]
fn pushes_a_commit() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _found = existing(&mut server, &format!("My Rostery Share Site. {MARKER}."));
    let pushed = push(&mut server);
    github::publish(&server.url(), TOKEN, "r", &site()).unwrap();
    for mock in pushed {
        mock.assert();
    }
}

#[test]
fn updates_existing_rostery_repo() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _found = existing(&mut server, &format!("My Rostery Share Site. {MARKER}."));
    let create = server.mock("POST", "/user/repos").expect(0).create();
    let _pushed = push(&mut server);
    github::publish(&server.url(), TOKEN, "r", &site()).unwrap();
    create.assert();
}

#[test]
fn refuses_to_overwrite_foreign_repo() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _found = existing(&mut server, "My thesis. Please do not touch.");
    let writes = [
        server.mock("POST", Matcher::Any).expect(0).create(),
        server.mock("PATCH", Matcher::Any).expect(0).create(),
    ];
    let message = github::publish(&server.url(), TOKEN, "r", &site())
        .unwrap_err()
        .to_string();
    assert!(message.contains("o/r"), "{message}");
    assert!(message.contains("did not create it"), "{message}");
    for mock in writes {
        mock.assert();
    }
}

#[test]
fn enables_pages_once() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _found = existing(&mut server, MARKER);
    // Pages is on already.
    let pushed = push_with_pages(&mut server, 409);
    github::publish(&server.url(), TOKEN, "r", &site()).unwrap();
    for mock in pushed {
        mock.assert();
    }
}

#[test]
fn reports_page_url() {
    assert_eq!(github::page_url("O", "r"), "https://o.github.io/r/");
    assert_eq!(github::page_url("O", "o.github.io"), "https://o.github.io/");
    assert_eq!(github::page_url("o", "O.GitHub.io"), "https://o.github.io/");
}

#[test]
fn bad_token_says_reconnect() {
    let mut server = Server::new();
    let _user = server.mock("GET", "/user").with_status(401).create();
    let message = github::publish(&server.url(), TOKEN, "r", &site())
        .unwrap_err()
        .to_string();
    assert!(message.contains("Connect GitHub again"), "{message}");
}

#[test]
fn invalid_repo_name_rejected() {
    let mut server = Server::new();
    let any = server
        .mock(Matcher::Any.to_string().as_str(), Matcher::Any)
        .expect(0)
        .create();
    for name in ["", "a/b", "../x", "space name", ".", "..", &"x".repeat(101)] {
        assert!(
            github::publish(&server.url(), TOKEN, name, &site()).is_err(),
            "{name}"
        );
    }
    any.assert();
    assert!(github::valid_repo_name("rostery-universe_2.0"));
}

#[test]
fn waits_for_initial_commit() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _missing = server.mock("GET", "/repos/o/r").with_status(404).create();
    let _create = server
        .mock("POST", "/user/repos")
        .with_status(201)
        .with_body(r#"{"default_branch":"main"}"#)
        .create();
    let not_yet = server
        .mock("GET", "/repos/o/r/git/ref/heads/main")
        .with_status(404)
        .expect(1)
        .create();
    let pushed = push(&mut server);
    github::publish(&server.url(), TOKEN, "r", &site()).unwrap();
    not_yet.assert();
    for mock in pushed {
        mock.assert();
    }
}

#[test]
fn a_missing_scope_says_so() {
    let mut server = Server::new();
    let _user = user(&mut server);
    let _missing = server.mock("GET", "/repos/o/r").with_status(404).create();
    let _create = server
        .mock("POST", "/user/repos")
        .with_status(403)
        .with_body(r#"{"message":"Resource not accessible by personal access token"}"#)
        .create();
    let message = github::publish(&server.url(), TOKEN, "r", &site())
        .unwrap_err()
        .to_string();
    assert!(message.contains("public_repo"), "{message}");
    assert!(message.contains("Resource not accessible"), "{message}");
}

#[test]
fn read_site_reads_every_file_with_forward_slashes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("index.html"), "i").unwrap();
    std::fs::write(dir.path().join("assets/share.js"), "j").unwrap();
    std::fs::write(dir.path().join(".nojekyll"), "").unwrap();
    let paths: Vec<String> = rostery_core::deploy::read_site(dir.path())
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(paths, [".nojekyll", "assets/share.js", "index.html"]);
}

#[test]
fn login_names_the_owner_and_a_rejected_token_is_nobody() {
    let mut server = Server::new();
    let _user = user(&mut server);
    assert_eq!(
        github::login(&server.url(), TOKEN).unwrap().as_deref(),
        Some("o")
    );
    let mut server = Server::new();
    let _gone = server.mock("GET", "/user").with_status(401).create();
    assert_eq!(github::login(&server.url(), TOKEN).unwrap(), None);
}
