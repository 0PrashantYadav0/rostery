//! OAuth 2 with PKCE for a desktop app: the browser comes back to a one-off listener on the loopback
//! address, and the code is swapped for tokens there. The client ID is the person's own.

use crate::error::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Spotify allows a `127.0.0.1` loopback redirect and refuses `localhost`, and the redirect has to be
/// registered exactly, so the port is fixed.
pub const REDIRECT_PORT: u16 = 43117;

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    /// Who the person is connecting, for messages.
    pub host: String,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub extra_auth_params: Vec<(String, String)>,
    pub port: u16,
}

impl OAuthConfig {
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: u64,
}

fn random_text(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the system has no source of random numbers");
    URL_SAFE_NO_PAD.encode(buf)
}

/// A verifier of 43 characters, and its challenge.
pub fn pkce_pair() -> (String, String) {
    let verifier = random_text(32);
    let challenge = challenge(&verifier);
    (verifier, challenge)
}

/// RFC 7636: base64url of the SHA-256 of the verifier, without padding.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Opens the host's sign-in page with `open_url`, waits up to `timeout` for the browser to come back to
/// the loopback listener, then swaps the code for tokens. Only `127.0.0.1` is bound, so nothing else on
/// the network can reach the listener.
pub fn authorize(
    cfg: &OAuthConfig,
    open_url: impl FnOnce(&str) -> Result<()>,
    timeout: Duration,
) -> Result<TokenSet> {
    let listener = TcpListener::bind(("127.0.0.1", cfg.port)).map_err(|e| {
        Error::Connection(format!(
            "Rostery could not listen for {} on port {}: {e}. Another program may be using it, or \
             another copy of Rostery is connecting. Close it, then try again.",
            cfg.host, cfg.port
        ))
    })?;
    listener.set_nonblocking(true)?;
    let (verifier, challenge) = pkce_pair();
    let state = random_text(16);
    let mut url = url::Url::parse(&cfg.auth_url).map_err(|e| {
        Error::Connection(format!(
            "Rostery has a bad sign-in address for {}: {e}",
            cfg.host
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("client_id", &cfg.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &cfg.redirect_uri())
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge)
        .append_pair("state", &state)
        .append_pair("scope", &cfg.scopes.join(" "))
        .extend_pairs(cfg.extra_auth_params.iter());
    open_url(url.as_str())?;

    let deadline = Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(code) = answer(stream, cfg, &state)? {
                    return exchange(cfg, &code, &verifier);
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err(Error::Connection(format!(
                        "{} did not send the browser back to Rostery in time. Try Connect again, and \
                         finish signing in within a few minutes.",
                        cfg.host
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Reads one request. The callback ends the wait one way or another; anything else, such as a
/// browser's `/favicon.ico`, gets a 404 and the wait goes on.
fn answer(stream: TcpStream, cfg: &OAuthConfig, state: &str) -> Result<Option<String>> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut line = String::new();
    // A request that never says anything is no reason to stop waiting for the real one.
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return Ok(None);
    }
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .unwrap_or_else(|_| url::Url::parse("http://127.0.0.1/").unwrap());
    if url.path() != "/callback" {
        reply(&stream, "404 Not Found", "Not found.");
        return Ok(None);
    }
    let get = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    let host = &cfg.host;
    if let Some(error) = get("error") {
        reply(
            &stream,
            "200 OK",
            &format!("Rostery was not connected to {host}. You can close this tab."),
        );
        return Err(Error::Connection(format!(
            "{host} did not connect Rostery ({error}). If you cancelled, press Connect again when \
             you are ready."
        )));
    }
    if get("state").as_deref() != Some(state) {
        reply(
            &stream,
            "400 Bad Request",
            "Rostery did not start this sign-in. You can close this tab.",
        );
        return Err(Error::Connection(format!(
            "The answer from {host} did not match the sign-in Rostery started, so Rostery ignored it. \
             Press Connect again."
        )));
    }
    let code = get("code").filter(|c| !c.is_empty()).ok_or_else(|| {
        Error::Connection(format!(
            "{host} came back without a code. Press Connect again."
        ))
    })?;
    reply(
        &stream,
        "200 OK",
        &format!("Rostery is connected to {host}. You can close this tab and go back to Rostery."),
    );
    Ok(Some(code))
}

fn reply(mut stream: &TcpStream, status: &str, words: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Rostery</title>\
         <body style=\"font:16px system-ui;padding:40px\"><p>{words}</p>"
    );
    // The browser tab is the only one who reads this, and the result is known already.
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
}

fn token_request(cfg: &OAuthConfig, mut form: Vec<(&str, String)>) -> Result<TokenSet> {
    form.push(("client_id", cfg.client_id.clone()));
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build(),
    );
    let host = &cfg.host;
    let mut response = agent
        .post(&cfg.token_url)
        .send_form(form.iter().map(|(k, v)| (*k, v.as_str())))
        .map_err(|e| {
            Error::Connection(format!(
                "Rostery could not reach {host}: {e}. Check your connection, then try again."
            ))
        })?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    if status == 200 {
        return serde_json::from_str(&text).map_err(|_| {
            Error::Connection(format!(
                "{host} sent tokens Rostery could not read. Press Connect again."
            ))
        });
    }
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if body["error"] == "invalid_grant" || body["error"] == "invalid_client" {
        return Err(Error::Connection(format!(
            "{host} no longer accepts Rostery's connection, which happens when access is removed or \
             the app changes. Disconnect, then connect again."
        )));
    }
    Err(Error::Connection(format!(
        "{host} refused the sign-in ({status}: {}). Press Connect again.",
        body["error_description"]
            .as_str()
            .or(body["error"].as_str())
            .unwrap_or("no reason given")
    )))
}

fn exchange(cfg: &OAuthConfig, code: &str, verifier: &str) -> Result<TokenSet> {
    token_request(
        cfg,
        vec![
            ("grant_type", "authorization_code".into()),
            ("code", code.into()),
            ("redirect_uri", cfg.redirect_uri()),
            ("code_verifier", verifier.into()),
        ],
    )
}

/// A fresh access token. A host that leaves out a new refresh token means the old one still works.
pub fn refresh(cfg: &OAuthConfig, refresh_token: &str) -> Result<TokenSet> {
    let mut tokens = token_request(
        cfg,
        vec![
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token.into()),
        ],
    )?;
    if tokens.refresh_token.is_none() {
        tokens.refresh_token = Some(refresh_token.to_owned());
    }
    Ok(tokens)
}
