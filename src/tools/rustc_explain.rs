//! Wraps `rustc --explain <CODE>`.

use std::time::Duration;

use rmcp::ErrorData;
use serde::Serialize;
use tokio::process::Command;

use super::util::{invalid, run_with_timeout};
use crate::server::RustcExplainArgs;

const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExplainResult {
    pub code: String,
    pub explanation: String,
}

pub async fn explain(args: RustcExplainArgs) -> Result<ExplainResult, ErrorData> {
    let code = args.code.trim().to_uppercase();
    if !code.starts_with('E') || code.len() < 2 || !code[1..].chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid(format!(
            "code must look like E0382 (got `{}`)",
            args.code
        )));
    }

    let mut cmd = Command::new("rustc");
    cmd.arg("--explain").arg(&code);
    let (stdout, stderr, status) = run_with_timeout(cmd, TIMEOUT).await?;
    if status != 0 {
        return Err(invalid(format!(
            "rustc --explain {code} failed: {}",
            stderr.trim()
        )));
    }
    Ok(ExplainResult {
        code,
        explanation: stdout,
    })
}
