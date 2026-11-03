//! The loopback OAuth flow, with each test's `open_url` acting as the browser.

use mockito::{Matcher, Server};
use rostery_core::oauth::{self, OAuthConfig};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

fn config(port: u16, token_url: &str) -> OAuthConfig {
    OAuthConfig {
        host: "Spotify".into(),
        auth_url: "https://accounts.example/authorize".into(),
        token_url: token_url.into(),
        client_id: "client-123".into(),
        client_secret: None,
        scopes: vec!["user-read-recently-played".into(), "user-top-read".into()],
        extra_auth_params: vec![],
        port,
    }
}

fn query(url: &str, key: &str) -> String {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .unwrap()
}

/// What a browser does after sign-in: one GET to the loopback listener. Gives the body it got back.
fn visit(port: u16, target: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    out
}

/// Runs `visits` from another thread once the sign-in page has been opened.
fn browser(
    port: u16,
    visits: impl FnOnce(&str) -> Vec<String> + Send + 'static,
) -> impl FnOnce(&str) -> rostery_core::Result<()> {
    move |url: &str| {
        let url = url.to_owned();
        std::thread::spawn(move || {
            for target in visits(&url) {
                visit(port, &target);
            }
        });
        Ok(())
    }
}

#[test]
fn pkce_challenge_matches_rfc7636() {
    assert_eq!(
        oauth::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let (verifier, challenge) = oauth::pkce_pair();
    assert_eq!(verifier.len(), 43);
    assert_eq!(oauth::challenge(&verifier), challenge);
    assert_ne!(oauth::pkce_pair().0, verifier);
}

#[test]
fn full_flow_exchanges_code() {
    let port = 43211;
    let mut server = Server::new();
    let token = server
        .mock("POST", "/token")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
            Matcher::UrlEncoded("code".into(), "the-code".into()),
            Matcher::UrlEncoded(
                "redirect_uri".into(),
                format!("http://127.0.0.1:{port}/callback"),
            ),
            Matcher::UrlEncoded("client_id".into(), "client-123".into()),
            Matcher::Regex("code_verifier=[A-Za-z0-9_-]{43}".into()),
        ]))
        .with_body(r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
        .create();
    let cfg = config(port, &format!("{}/token", server.url()));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen_in = seen.clone();
    let tokens = oauth::authorize(
        &cfg,
        browser(port, move |url| {
            *seen_in.lock().unwrap() = url.to_owned();
            // A browser asks for an icon first, which must not end the wait.
            vec![
                "/favicon.ico".into(),
                format!("/callback?code=the-code&state={}", query(url, "state")),
            ]
        }),
        Duration::from_secs(10),
    )
    .unwrap();
    token.assert();
    assert_eq!(tokens.access_token, "at");
    assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
    let url = seen.lock().unwrap().clone();
    assert_eq!(query(&url, "code_challenge_method"), "S256");
    assert_eq!(
        query(&url, "scope"),
        "user-read-recently-played user-top-read"
    );
    assert_eq!(
        query(&url, "redirect_uri"),
        format!("http://127.0.0.1:{port}/callback")
    );
}

#[test]
fn wrong_state_is_rejected() {
    let port = 43212;
    let mut server = Server::new();
    let token = server.mock("POST", "/token").expect(0).create();
    let cfg = config(port, &format!("{}/token", server.url()));
    let message = oauth::authorize(
        &cfg,
        browser(port, |_| vec!["/callback?code=c&state=forged".into()]),
        Duration::from_secs(10),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("did not match"), "{message}");
    token.assert();
}

#[test]
fn provider_error_is_reported() {
    let port = 43213;
    let cfg = config(port, "http://127.0.0.1:9/token");
    let message = oauth::authorize(
        &cfg,
        browser(port, |url| {
            vec![format!(
                "/callback?error=access_denied&state={}",
                query(url, "state")
            )]
        }),
        Duration::from_secs(10),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("access_denied"), "{message}");
}

#[test]
fn timeout_returns_error() {
    let cfg = config(43214, "http://127.0.0.1:9/token");
    let message = oauth::authorize(&cfg, |_| Ok(()), Duration::from_millis(200))
        .unwrap_err()
        .to_string();
    assert!(message.contains("in time"), "{message}");
}

#[test]
fn busy_port_says_so() {
    let port = 43215;
    let _taken = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let cfg = config(port, "http://127.0.0.1:9/token");
    let message = oauth::authorize(
        &cfg,
        |_| panic!("must not open the browser"),
        Duration::from_secs(1),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("port 43215"), "{message}");
}

/// The machine's own address on its network, with nothing sent anywhere.
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[test]
fn only_binds_loopback() {
    let port = 43216;
    let Some(lan) = lan_address() else {
        eprintln!("no network address on this machine, so there is nothing to check");
        return;
    };
    let reached = std::sync::Arc::new(std::sync::Mutex::new(None));
    let reached_in = reached.clone();
    let cfg = config(port, "http://127.0.0.1:9/token");
    let _ = oauth::authorize(
        &cfg,
        move |_| {
            *reached_in.lock().unwrap() = Some(
                TcpStream::connect_timeout(&(lan, port).into(), Duration::from_secs(2)).is_ok(),
            );
            Ok(())
        },
        Duration::from_millis(300),
    );
    assert_eq!(*reached.lock().unwrap(), Some(false), "reachable at {lan}");
}

#[test]
fn refresh_keeps_old_refresh_token_when_absent() {
    let mut server = Server::new();
    let _token = server
        .mock("POST", "/token")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("grant_type".into(), "refresh_token".into()),
            Matcher::UrlEncoded("refresh_token".into(), "old".into()),
        ]))
        .with_body(r#"{"access_token":"new-at","expires_in":3600}"#)
        .create();
    let cfg = config(43217, &format!("{}/token", server.url()));
    let tokens = oauth::refresh(&cfg, "old").unwrap();
    assert_eq!(tokens.access_token, "new-at");
    assert_eq!(tokens.refresh_token.as_deref(), Some("old"));
}

#[test]
fn invalid_grant_says_reconnect() {
    let mut server = Server::new();
    let _token = server
        .mock("POST", "/token")
        .with_status(400)
        .with_body(r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#)
        .create();
    let cfg = config(43218, &format!("{}/token", server.url()));
    let message = oauth::refresh(&cfg, "old").unwrap_err().to_string();
    assert!(message.contains("connect again"), "{message}");
}
