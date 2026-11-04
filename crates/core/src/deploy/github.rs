//! GitHub Pages through the REST API: a repo Rostery created, one commit holding the whole site, and Pages
//! serving that branch. The token is the person's own, with the public_repo scope.

use super::{call, said, DeployResult};
use crate::error::{Error, Result};
use crate::export::SiteFile;
use base64::Engine;
use serde_json::{json, Value};
use std::time::Duration;

/// In the description of every repo Rostery creates, and the only proof it may write there.
pub const MARKER: &str = "Published by Rostery";
const API_VERSION: &str = "2022-11-28";
const BRANCH_TRIES: u32 = 10;

struct Api<'a> {
    base: &'a str,
    token: &'a str,
}

impl Api<'_> {
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value)> {
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        let headers = [
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", API_VERSION),
        ];
        call("GitHub", method, &url, self.token, &headers, body)
    }

    /// A call that must answer one of `ok`, or it says what Rostery was doing when it did not.
    fn expect(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        ok: &[u16],
        doing: &str,
    ) -> Result<Value> {
        let (status, value) = self.call(method, path, body)?;
        if ok.contains(&status) {
            return Ok(value);
        }
        if status == 401 {
            return Err(rejected());
        }
        let reason = said(&value);
        let scope = if status == 403 || status == 404 {
            " Check that the token has the public_repo scope."
        } else {
            ""
        };
        Err(Error::Deploy(format!(
            "GitHub refused to {doing} ({status}{}{reason}).{scope}",
            if reason.is_empty() { "" } else { ": " }
        )))
    }
}

fn rejected() -> Error {
    Error::Deploy(
        "GitHub did not accept your token, so it may have expired or been revoked. Connect GitHub \
         again on the Share screen."
            .into(),
    )
}

/// What GitHub allows in a repo name, checked before anything is sent.
pub fn valid_repo_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && name != "."
        && name != ".."
}

/// Whose token this is, or nothing when GitHub no longer accepts it.
pub fn login(api: &str, token: &str) -> Result<Option<String>> {
    let gh = Api { base: api, token };
    let (status, user) = gh.call("GET", "/user", None)?;
    match status {
        200 => {}
        401 => return Ok(None),
        _ => {
            return Err(Error::Deploy(format!(
                "GitHub answered {status} when Rostery asked whose token this is: {}",
                said(&user)
            )))
        }
    }
    user["login"]
        .as_str()
        .map(|login| Some(login.to_owned()))
        .ok_or_else(|| Error::Deploy("GitHub did not say whose token this is.".into()))
}

pub fn publish(api: &str, token: &str, repo: &str, site: &[SiteFile]) -> Result<DeployResult> {
    publish_waiting(api, token, repo, site, Duration::from_millis(700))
}

fn publish_waiting(
    api: &str,
    token: &str,
    repo: &str,
    site: &[SiteFile],
    wait: Duration,
) -> Result<DeployResult> {
    if !valid_repo_name(repo) {
        return Err(Error::Deploy(format!(
            "\"{repo}\" cannot be a GitHub repo name. Use up to 100 letters, digits, dots, hyphens and \
             underscores."
        )));
    }
    let gh = Api { base: api, token };
    let owner = login(api, token)?.ok_or_else(rejected)?;
    let path = format!("/repos/{owner}/{repo}");
    let url = page_url(&owner, repo);

    let (status, found) = gh.call("GET", &path, None)?;
    let branch = match status {
        401 => return Err(rejected()),
        200 => {
            if !found["description"].as_str().unwrap_or("").contains(MARKER) {
                return Err(Error::Deploy(format!(
                    "{owner}/{repo} already exists and Rostery did not create it, so Rostery will not \
                     publish over it. Choose another name."
                )));
            }
            found["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_owned()
        }
        404 => {
            let created = gh.expect(
                "POST",
                "/user/repos",
                Some(&json!({
                    "name": repo,
                    "description": format!("My Rostery Share Site. {MARKER}."),
                    "homepage": url,
                    "auto_init": true,
                    "has_issues": false,
                    "has_wiki": false,
                    "has_projects": false,
                })),
                &[201],
                &format!("create {owner}/{repo}"),
            )?;
            created["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_owned()
        }
        _ => {
            return Err(Error::Deploy(format!(
                "GitHub answered {status} when Rostery looked for {owner}/{repo}: {}",
                said(&found)
            )))
        }
    };

    // A new repo's first commit can take a moment to appear, and the branch has to exist to be moved.
    let mut head = None;
    for _ in 0..BRANCH_TRIES {
        let (status, found) = gh.call("GET", &format!("{path}/git/ref/heads/{branch}"), None)?;
        if status == 200 {
            head = found["object"]["sha"].as_str().map(str::to_owned);
            break;
        }
        std::thread::sleep(wait);
    }
    head.ok_or_else(|| {
        Error::Deploy(format!(
            "GitHub has not finished creating {owner}/{repo}. Wait a minute, then publish again."
        ))
    })?;

    let mut tree = Vec::new();
    for file in site {
        let blob = gh.expect(
            "POST",
            &format!("{path}/git/blobs"),
            Some(&json!({
                "content": base64::engine::general_purpose::STANDARD.encode(&file.bytes),
                "encoding": "base64",
            })),
            &[201],
            &format!("take {}", file.path),
        )?;
        tree.push(json!({"path": file.path, "mode": "100644", "type": "blob", "sha": blob["sha"]}));
    }
    // No base_tree, so a file left out of this site is gone from the branch too.
    let tree = gh.expect(
        "POST",
        &format!("{path}/git/trees"),
        Some(&json!({ "tree": tree })),
        &[201],
        "take the site's folder",
    )?;
    // A commit with no parent, forced onto the branch, so an earlier site with things since left out does
    // not stay in the branch's history. GitHub can still keep it by its commit ID until the repo is deleted.
    let commit = gh.expect(
        "POST",
        &format!("{path}/git/commits"),
        Some(&json!({
            "message": "Publish from Rostery",
            "tree": tree["sha"],
            "parents": [],
        })),
        &[201],
        "take the commit",
    )?;
    gh.expect(
        "PATCH",
        &format!("{path}/git/refs/heads/{branch}"),
        Some(&json!({ "sha": commit["sha"], "force": true })),
        &[200],
        &format!("move {branch} to the new site"),
    )?;
    // 409 means Pages is on already.
    gh.expect(
        "POST",
        &format!("{path}/pages"),
        Some(&json!({ "source": { "branch": branch, "path": "/" } })),
        &[201, 409],
        "turn on GitHub Pages",
    )?;
    Ok(DeployResult {
        url,
        dashboard_url: format!("https://github.com/{owner}/{repo}"),
    })
}

/// A repo named `<owner>.github.io` is served at the root, and any other under its own name.
pub fn page_url(owner: &str, repo: &str) -> String {
    let host = format!("{}.github.io", owner.to_lowercase());
    if repo.eq_ignore_ascii_case(&host) {
        format!("https://{host}/")
    } else {
        format!("https://{host}/{repo}/")
    }
}
