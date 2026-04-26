//! `run_locally`, never executes anything itself, just emits a structured
//! plan of shell commands the calling LLM agent (Claude Code, OpenCode,
//! Cursor, …) is expected to run with its own bash tool on the *user's*
//! machine. This is the safest sandbox: the user's local OS, the user's
//! permissions, the user's cargo cache. No code we receive ever touches
//! this server's filesystem.

use serde::Serialize;

use crate::server::RunLocallyArgs;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Plan {
    pub overview: String,
    pub commands: Vec<Step>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Step {
    pub command: String,
    pub purpose: String,
}

pub fn plan(args: RunLocallyArgs) -> Plan {
    let task = args.task.trim().to_lowercase();

    let (overview, commands) = if task.contains("bench") {
        bench_plan()
    } else if task.contains("test") {
        test_plan()
    } else if task.contains("fuzz") {
        fuzz_plan()
    } else if task.contains("miri") {
        miri_plan()
    } else if task.contains("expand") || task.contains("macro") {
        expand_plan()
    } else if task.contains("audit") || task.contains("vuln") {
        audit_plan()
    } else {
        check_plan()
    };

    Plan {
        overview,
        commands,
        notes: vec![
            "Run from the cargo workspace root.".into(),
            "All commands honour your local toolchain, pin via rust-toolchain.toml if you need stability.".into(),
            "These commands are emitted by the rust-docsbox MCP server. The server itself never executes them; your agent must run them on the user's machine.".into(),
        ],
    }
}

fn check_plan() -> (String, Vec<Step>) {
    (
        "Compile and lint locally without producing a binary.".into(),
        vec![
            step("cargo fmt --all -- --check", "verify formatting"),
            step(
                "cargo clippy --all-targets --all-features -- -D warnings",
                "lint with errors-on-warnings",
            ),
            step(
                "cargo check --all-targets --all-features",
                "type-check everything",
            ),
        ],
    )
}

fn test_plan() -> (String, Vec<Step>) {
    (
        "Run the full test suite.".into(),
        vec![
            step("cargo build --all-targets --all-features", "compile tests"),
            step("cargo test --all-features --workspace", "execute tests"),
            step(
                "cargo test --all-features --workspace --doc",
                "execute doctests (separate cargo invocation)",
            ),
        ],
    )
}

fn bench_plan() -> (String, Vec<Step>) {
    (
        "Run criterion / nightly benches. Falls back to release-mode test bench if criterion is absent.".into(),
        vec![
            step(
                "cargo install cargo-criterion --locked || true",
                "install criterion driver if missing",
            ),
            step("cargo criterion --workspace", "run criterion benches"),
            step(
                "cargo bench --all-features",
                "fallback for nightly benches",
            ),
        ],
    )
}

fn fuzz_plan() -> (String, Vec<Step>) {
    (
        "Run libfuzzer via cargo-fuzz. Requires nightly toolchain.".into(),
        vec![
            step("rustup toolchain install nightly", "ensure nightly"),
            step(
                "cargo install cargo-fuzz --locked || true",
                "install cargo-fuzz",
            ),
            step("cargo +nightly fuzz list", "list registered targets"),
            step(
                "cargo +nightly fuzz run <TARGET> -- -max_total_time=60",
                "run a target for 60s",
            ),
        ],
    )
}

fn miri_plan() -> (String, Vec<Step>) {
    (
        "Run miri (UB detection).".into(),
        vec![
            step(
                "rustup +nightly component add miri",
                "install miri component",
            ),
            step("cargo +nightly miri test", "run tests under miri"),
        ],
    )
}

fn expand_plan() -> (String, Vec<Step>) {
    (
        "Inspect macro expansion to debug proc macros.".into(),
        vec![
            step(
                "cargo install cargo-expand --locked || true",
                "install cargo-expand",
            ),
            step("cargo expand --bin <BIN>", "expand a binary target"),
            step("cargo expand --lib", "expand the library crate"),
        ],
    )
}

fn audit_plan() -> (String, Vec<Step>) {
    (
        "Audit dependencies for known vulnerabilities and license issues.".into(),
        vec![
            step(
                "cargo install cargo-audit --locked || true",
                "install cargo-audit",
            ),
            step(
                "cargo audit",
                "check Cargo.lock against the RustSec advisory db",
            ),
            step(
                "cargo install cargo-deny --locked || true",
                "install cargo-deny",
            ),
            step("cargo deny check", "license/advisory/duplicate review"),
        ],
    )
}

fn step(command: &str, purpose: &str) -> Step {
    Step {
        command: command.into(),
        purpose: purpose.into(),
    }
}
