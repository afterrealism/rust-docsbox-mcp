//! `rustfmt` tool, pipe a snippet through `rustfmt --emit stdout`.

use std::process::Stdio;
use std::time::Duration;

use rmcp::ErrorData;
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::util::{edition, internal};
use crate::server::CodeArgs;

const FMT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FmtReport {
    pub ok: bool,
    pub formatted: String,
    pub stderr: String,
}

pub async fn rustfmt(args: CodeArgs) -> Result<FmtReport, ErrorData> {
    let edition = edition(args.edition.as_deref())?;

    let mut child = Command::new("rustfmt")
        .arg("--emit")
        .arg("stdout")
        .arg("--edition")
        .arg(&edition)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(internal)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(args.code.as_bytes())
            .await
            .map_err(internal)?;
        stdin.shutdown().await.ok();
    }

    let out = match tokio::time::timeout(FMT_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(internal(e)),
        Err(_) => return Err(internal("rustfmt timed out")),
    };

    let formatted = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    // rustfmt prints `--emit stdout` headers; strip the leading
    // `<stdin>:\n\n` block if present.
    let formatted = formatted
        .strip_prefix("<stdin>:\n\n")
        .unwrap_or(&formatted)
        .to_string();

    Ok(FmtReport {
        ok: out.status.success(),
        formatted,
        stderr,
    })
}
