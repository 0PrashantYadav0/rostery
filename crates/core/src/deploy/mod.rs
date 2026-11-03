//! Publishing a Share Site. Rostery talks to a host's API with the person's own token, and to nothing else.

pub mod github;

use crate::error::{Error, Result};
use crate::export::SiteFile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeployResult {
    pub url: String,
    pub dashboard_url: String,
}

/// What the Share screen shows about publishing. No token ever reaches the webview.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeployStatus {
    /// Whose GitHub account a publish would use.
    pub github: Option<String>,
    /// Connected through a logged-in `gh`, which Rostery can use but not forget.
    pub github_through_gh: bool,
    pub vercel: bool,
}

/// Every file under `dir`, with `/` between folders whatever the system uses.
pub fn read_site(dir: &Path) -> Result<Vec<SiteFile>> {
    let mut files = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let path = entry?.path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let rel = path.strip_prefix(dir).map_err(|_| {
                Error::Deploy("Rostery found a file outside its Share Site folder.".into())
            })?;
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            files.push(SiteFile {
                path: parts.join("/"),
                bytes: std::fs::read(&path)?,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// No redirect, so a token in a header never follows one somewhere else. A status code is read here and
/// not turned into an error by ureq.
pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build(),
    )
}

/// A host's own words about what went wrong, kept short.
pub(crate) fn said(body: &Value) -> String {
    let message = body["message"]
        .as_str()
        .or(body["error"]["message"].as_str())
        .unwrap_or("")
        .trim();
    crate::roast::clean(message, 300)
}
