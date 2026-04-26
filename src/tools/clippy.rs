//! `clippy_check` and `clippy_fix` tools.
//!
//! These shell out to `cargo clippy` against a per-call temporary cargo
//! project under the OS tmp dir. The CARGO_TARGET_DIR is overridden to a
//! shared cache (default `/tmp/rust-docsbox/target`, override with
//! `RUST_DOCSBOX_TARGET_DIR`) so repeated calls reuse incremental builds.

use std::path::PathBuf;
use std::time::Duration;

use rmcp::ErrorData;
use serde::Serialize;
use tokio::process::Command;

use super::util::{edition, internal, run_with_timeout, ScratchProject};
use crate::server::CodeArgs;

const CLIPPY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ClippyReport {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub stderr: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ClippyFixReport {
    pub ok: bool,
    pub fixed_source: String,
    pub remaining: Vec<Diagnostic>,
    pub stderr: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
    pub code: Option<String>,
    pub line: Option<u64>,
    pub col: Option<u64>,
    pub rendered: Option<String>,
}

pub async fn clippy_check(args: CodeArgs) -> Result<ClippyReport, ErrorData> {
    let edition = edition(args.edition.as_deref())?;
    let project = ScratchProject::create(&args.code, &edition).map_err(internal)?;
    let target = target_dir();
    let _ = std::fs::create_dir_all(&target);

    let mut cmd = Command::new("cargo");
    cmd.arg("clippy")
        .arg("--message-format=json")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&project.manifest)
        .env("CARGO_TARGET_DIR", &target)
        .env("RUSTFLAGS", "--cap-lints=warn")
        .env("CARGO_TERM_COLOR", "never");

    let (stdout, stderr, _code) = run_with_timeout(cmd, CLIPPY_TIMEOUT).await?;
    let diagnostics = parse_diagnostics(&stdout);
    Ok(ClippyReport {
        ok: diagnostics.iter().all(|d| d.level != "error"),
        diagnostics,
        stderr,
    })
}

pub async fn clippy_fix(args: CodeArgs) -> Result<ClippyFixReport, ErrorData> {
    let edition = edition(args.edition.as_deref())?;
    let project = ScratchProject::create(&args.code, &edition).map_err(internal)?;
    let target = target_dir();
    let _ = std::fs::create_dir_all(&target);

    let mut cmd = Command::new("cargo");
    cmd.arg("clippy")
        .arg("--fix")
        .arg("--allow-no-vcs")
        .arg("--manifest-path")
        .arg(&project.manifest)
        .arg("--message-format=json")
        .arg("--quiet")
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_TERM_COLOR", "never");

    let (stdout, stderr, _code) = run_with_timeout(cmd, CLIPPY_TIMEOUT).await?;
    let fixed = std::fs::read_to_string(&project.main).unwrap_or_default();
    let remaining = parse_diagnostics(&stdout);
    Ok(ClippyFixReport {
        ok: remaining.iter().all(|d| d.level != "error"),
        fixed_source: fixed,
        remaining,
        stderr,
    })
}

fn target_dir() -> PathBuf {
    std::env::var_os("RUST_DOCSBOX_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("rust-docsbox").join("target"))
}

fn parse_diagnostics(stdout: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // Cargo wraps rustc diagnostics under reason="compiler-message".
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let level = msg
            .get("level")
            .and_then(|x| x.as_str())
            .unwrap_or("note")
            .to_string();
        let message = msg
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(str::to_string);
        let (line, col) = msg
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|s| {
                        s.get("is_primary")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false)
                    })
                    .or_else(|| a.first())
            })
            .map(|s| {
                (
                    s.get("line_start").and_then(|x| x.as_u64()),
                    s.get("column_start").and_then(|x| x.as_u64()),
                )
            })
            .unwrap_or((None, None));
        let rendered = msg
            .get("rendered")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        out.push(Diagnostic {
            level,
            message,
            code,
            line,
            col,
            rendered,
        });
    }
    out
}
