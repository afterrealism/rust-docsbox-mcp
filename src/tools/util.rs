//! Shared helpers for tool modules.

use std::path::PathBuf;
use std::time::Duration;

use rmcp::ErrorData;
use tempfile::TempDir;
use tokio::process::Command;

/// Map any boxed error into an MCP `ErrorData::internal_error`.
pub fn internal<E: std::fmt::Display>(e: E) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// Map a validation/user-input error into MCP `invalid_params`.
pub fn invalid<E: std::fmt::Display>(e: E) -> ErrorData {
    ErrorData::invalid_params(e.to_string(), None)
}

/// A temporary cargo project layout, ready for `cargo clippy` / `cargo
/// fmt`. Returned `TempDir` is dropped (and therefore deleted) when it
/// goes out of scope on the caller side.
pub struct ScratchProject {
    #[allow(dead_code)]
    pub dir: TempDir,
    pub manifest: PathBuf,
    pub main: PathBuf,
}

impl ScratchProject {
    pub fn create(code: &str, edition: &str) -> std::io::Result<Self> {
        let dir = tempfile::Builder::new().prefix("rdocsbox-").tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("src"))?;

        let manifest = root.join("Cargo.toml");
        std::fs::write(
            &manifest,
            format!(
                r#"[package]
name = "rdocsbox_scratch"
version = "0.0.0"
edition = "{edition}"
publish = false

[lib]
path = "src/lib.rs"

[[bin]]
name = "rdocsbox_scratch"
path = "src/main.rs"
"#,
            ),
        )?;
        std::fs::write(root.join("src").join("lib.rs"), "")?;
        let main = root.join("src").join("main.rs");
        let body = if code.contains("fn main") {
            code.to_string()
        } else {
            format!("fn main() {{\n{code}\n}}\n")
        };
        std::fs::write(&main, body)?;

        Ok(Self {
            dir,
            manifest,
            main,
        })
    }
}

/// Validate edition token; returns the canonicalised string.
pub fn edition(input: Option<&str>) -> Result<String, ErrorData> {
    match input.unwrap_or("2021") {
        e @ ("2015" | "2018" | "2021" | "2024") => Ok(e.to_string()),
        other => Err(invalid(format!(
            "edition must be one of 2015/2018/2021/2024, got `{other}`"
        ))),
    }
}

/// Run a command with a timeout. Returns (stdout, stderr, exit_code).
pub async fn run_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<(String, String, i32), ErrorData> {
    cmd.kill_on_drop(true);
    let child = cmd.output();
    match tokio::time::timeout(timeout, child).await {
        Ok(Ok(out)) => Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.code().unwrap_or(-1),
        )),
        Ok(Err(e)) => Err(internal(format!("spawn failed: {e}"))),
        Err(_) => Err(internal(format!(
            "command timed out after {}s",
            timeout.as_secs()
        ))),
    }
}
