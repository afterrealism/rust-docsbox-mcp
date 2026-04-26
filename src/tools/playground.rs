//! Rust Playground integration.
//!
//! `playground_link`, POST a snippet to play.rust-lang.org's `meta/gist`
//! endpoint, return a permalink. Cheap, no execution.
//!
//! `playground_run`, POST to `/execute`. Playground enforces a
//! ~12s wallclock timeout, 256 MiB memory cap, no network. We don't run
//! anything ourselves, we proxy.

use rmcp::ErrorData;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::util::{edition, internal};
use crate::server::{AppState, PlaygroundLinkArgs, PlaygroundRunArgs};

const GIST_URL: &str = "https://play.rust-lang.org/meta/gist";
const EXEC_URL: &str = "https://play.rust-lang.org/execute";

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PlaygroundLink {
    pub gist_id: String,
    pub url: String,
    pub raw_url: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PlaygroundResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub channel: String,
    pub edition: String,
    pub mode: String,
}

#[derive(Debug, Deserialize)]
struct GistResponse {
    id: String,
    url: String,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExecuteResponse {
    success: bool,
    stdout: String,
    stderr: String,
}

pub async fn link(state: &AppState, args: PlaygroundLinkArgs) -> Result<PlaygroundLink, ErrorData> {
    let edition = edition(args.edition.as_deref())?;
    let channel = channel(args.channel.as_deref())?;
    let mode = mode(args.mode.as_deref())?;

    let resp: GistResponse = state
        .http
        .post(GIST_URL)
        .json(&json!({ "code": args.code }))
        .send()
        .await
        .map_err(|e| internal(format!("gist post: {e}")))?
        .error_for_status()
        .map_err(|e| internal(format!("gist non-2xx: {e}")))?
        .json()
        .await
        .map_err(|e| internal(format!("gist json: {e}")))?;

    let permalink = format!(
        "https://play.rust-lang.org/?version={channel}&mode={mode}&edition={edition}&gist={}",
        resp.id
    );
    let raw_url = if resp.code.is_some() {
        resp.url.clone()
    } else {
        format!(
            "https://gist.githubusercontent.com/rust-play/{}/raw",
            resp.id
        )
    };

    Ok(PlaygroundLink {
        gist_id: resp.id,
        url: permalink,
        raw_url,
    })
}

pub async fn run(state: &AppState, args: PlaygroundRunArgs) -> Result<PlaygroundResult, ErrorData> {
    let edition = edition(args.edition.as_deref())?;
    let channel = channel(args.channel.as_deref())?;
    let mode = mode(args.mode.as_deref())?;
    let action = args.action.as_deref().unwrap_or("Run").to_string();

    let body = json!({
        "channel": channel,
        "mode": mode,
        "edition": edition,
        "crateType": "bin",
        "tests": action == "Test",
        "code": args.code,
        "backtrace": false,
    });

    let resp: ExecuteResponse = state
        .http
        .post(EXEC_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| internal(format!("execute post: {e}")))?
        .error_for_status()
        .map_err(|e| internal(format!("execute non-2xx: {e}")))?
        .json()
        .await
        .map_err(|e| internal(format!("execute json: {e}")))?;

    Ok(PlaygroundResult {
        success: resp.success,
        stdout: resp.stdout,
        stderr: resp.stderr,
        channel,
        edition,
        mode,
    })
}

fn channel(input: Option<&str>) -> Result<String, ErrorData> {
    match input.unwrap_or("stable") {
        c @ ("stable" | "beta" | "nightly") => Ok(c.to_string()),
        other => Err(ErrorData::invalid_params(
            format!("channel must be stable/beta/nightly, got `{other}`"),
            None,
        )),
    }
}

fn mode(input: Option<&str>) -> Result<String, ErrorData> {
    match input.unwrap_or("debug") {
        m @ ("debug" | "release") => Ok(m.to_string()),
        other => Err(ErrorData::invalid_params(
            format!("mode must be debug/release, got `{other}`"),
            None,
        )),
    }
}
