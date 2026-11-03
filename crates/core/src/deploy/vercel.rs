//! Vercel: one production deployment holding the whole site, with no build, then a wait until it is ready.

use super::{call, said, DeployResult};
use crate::error::{Error, Result};
use crate::export::SiteFile;
use base64::Engine;
use serde_json::{json, Value};
use std::time::Duration;

const MAX_POLLS: u32 = 120;
const DASHBOARD: &str = "https://vercel.com/dashboard";

fn rejected() -> Error {
    Error::Deploy(
        "Vercel did not accept your token, so it may have expired or been revoked. Connect Vercel \
         again on the Share screen."
            .into(),
    )
}

fn api_call(
    api: &str,
    method: &str,
    path: &str,
    token: &str,
    body: Option<&Value>,
) -> Result<(u16, Value)> {
    let url = format!("{}{path}", api.trim_end_matches('/'));
    let (status, value) = call("Vercel", method, &url, token, &[], body)?;
    if status == 401 || status == 403 {
        return Err(rejected());
    }
    Ok((status, value))
}

/// Whether Vercel accepts this token.
pub fn accepts(api: &str, token: &str) -> Result<bool> {
    let url = format!("{}/v2/user", api.trim_end_matches('/'));
    let (status, value) = call("Vercel", "GET", &url, token, &[], None)?;
    match status {
        200 => Ok(true),
        401 | 403 => Ok(false),
        _ => Err(Error::Deploy(format!(
            "Vercel answered {status} when Rostery checked the token: {}",
            said(&value)
        ))),
    }
}

/// Lowercase, with anything Vercel does not allow turned into a hyphen, at most 100 characters.
pub fn project_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_matches('-').chars().take(100).collect();
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "rostery-universe".into()
    } else {
        out
    }
}

pub fn publish(
    api: &str,
    token: &str,
    project: &str,
    site: &[SiteFile],
    poll_every: Duration,
) -> Result<DeployResult> {
    let project = project_name(project);
    // ponytail: every file goes inline in one request, which suits a site of three small files. Upload
    // with POST /v2/files and send their digests if a Share Site ever grows past a few megabytes.
    let files: Vec<Value> = site
        .iter()
        // GitHub Pages needs it, and Vercel would serve it.
        .filter(|f| f.path != ".nojekyll")
        .map(|f| {
            json!({
                "file": f.path,
                "data": base64::engine::general_purpose::STANDARD.encode(&f.bytes),
                "encoding": "base64",
            })
        })
        .collect();
    let (status, created) = api_call(
        api,
        "POST",
        "/v13/deployments?skipAutoDetectionConfirmation=1",
        token,
        Some(&json!({
            "name": project,
            "target": "production",
            "projectSettings": { "framework": null },
            "files": files,
        })),
    )?;
    if status != 200 && status != 201 {
        return Err(Error::Deploy(format!(
            "Vercel refused the deployment ({status}: {}).",
            said(&created)
        )));
    }
    let id = created["id"]
        .as_str()
        .ok_or_else(|| Error::Deploy("Vercel did not say which deployment it made.".into()))?
        .to_owned();
    let dashboard = created["inspectorUrl"]
        .as_str()
        .unwrap_or(DASHBOARD)
        .to_owned();

    let mut latest = created;
    for _ in 0..MAX_POLLS {
        match latest["readyState"].as_str().unwrap_or("") {
            "READY" => return Ok(result(&latest, dashboard)),
            "ERROR" | "CANCELED" => {
                let why = latest["errorMessage"].as_str().unwrap_or("no reason given");
                return Err(Error::Deploy(format!(
                    "Vercel could not finish the deployment: {why}. See {dashboard}"
                )));
            }
            _ => {}
        }
        std::thread::sleep(poll_every);
        let (status, value) = api_call(api, "GET", &format!("/v13/deployments/{id}"), token, None)?;
        if status != 200 {
            return Err(Error::Deploy(format!(
                "Vercel answered {status} when Rostery asked about the deployment: {}",
                said(&value)
            )));
        }
        latest = value;
    }
    Err(Error::Deploy(format!(
        "Vercel is still working on the deployment, so Rostery stopped waiting. It may still finish: \
         see {dashboard}"
    )))
}

/// The production alias when there is one, since the deployment's own address changes every time.
fn result(ready: &Value, dashboard: String) -> DeployResult {
    let host = ready["alias"][0]
        .as_str()
        .or(ready["url"].as_str())
        .unwrap_or("");
    let url = if host.starts_with("https://") {
        host.to_owned()
    } else {
        format!("https://{host}")
    };
    DeployResult {
        url,
        dashboard_url: dashboard,
    }
}
