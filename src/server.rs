//! MCP `tool_router`, every public tool the LLM can call is registered here.
//!
//! The pattern follows the rmcp 0.8+ macro guide: each tool is a method on
//! `DocsBox`, takes typed `Parameters<T>` for its arguments and returns a
//! type that serialises into MCP `CallToolResult` content. Heavy lifting
//! lives in the `tools::*` modules so that `server.rs` stays a thin wiring
//! layer that's easy to scan.

use std::sync::Arc;

use rmcp::{
    handler::server::{
        tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::Deserialize;

use crate::corpus::Corpus;
use crate::tools::{
    clippy as clippy_tool, crates_io, docs, playground, run_locally, rustc_explain,
    rustfmt as rustfmt_tool,
};

/// Process-wide state. Cheap to clone (everything inside is `Arc`).
#[derive(Clone)]
pub struct AppState {
    pub corpus: Arc<Corpus>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(corpus: Corpus) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "rust-docsbox-mcp/",
                env!("CARGO_PKG_VERSION"),
                " (+https://rust-mcp.afterrealism.com)"
            ))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        Self {
            corpus: Arc::new(corpus),
            http,
        }
    }
}

#[derive(Clone)]
pub struct DocsBox {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

// ---------- argument structs ----------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSectionsArgs {
    /// Optional substring filter on the section path. Case-insensitive.
    #[serde(default)]
    pub query: Option<String>,
    /// Optional package filter, e.g. "tokio", "std", "axum".
    #[serde(default)]
    pub package: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDocumentationArgs {
    /// Section path returned by `list_sections`, e.g. "tokio::sync::Mutex".
    pub section: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeArgs {
    /// Rust source snippet. Wrap in `fn main() { ... }` if it isn't already.
    pub code: String,
    /// Edition: 2015 / 2018 / 2021 / 2024. Defaults to 2021.
    #[serde(default)]
    pub edition: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateSearchArgs {
    pub query: String,
    /// 1..=20, defaults to 10.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrateInfoArgs {
    pub name: String,
    /// Optional exact version, otherwise newest stable.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlaygroundLinkArgs {
    pub code: String,
    #[serde(default)]
    pub edition: Option<String>,
    /// "debug" or "release"
    #[serde(default)]
    pub mode: Option<String>,
    /// "stable" / "beta" / "nightly"
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlaygroundRunArgs {
    pub code: String,
    #[serde(default)]
    pub edition: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    /// "Run" / "Build" / "Test"
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RustcExplainArgs {
    /// e.g. "E0382"
    pub code: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunLocallyArgs {
    /// Free-form description of what the LLM wants to do.
    pub task: String,
}

// ---------- tool router ----------

#[tool_router]
impl DocsBox {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List Rust documentation sections. Returns paths like `std::vec::Vec` or `tokio::sync::Mutex`. Optionally filter by `query` substring or `package` (e.g. `tokio`, `std`)."
    )]
    async fn list_sections(
        &self,
        Parameters(args): Parameters<ListSectionsArgs>,
    ) -> Result<Json<docs::SectionList>, ErrorData> {
        Ok(Json(docs::list_sections(&self.state, args).await?))
    }

    #[tool(
        description = "Fetch the rendered documentation for a single section path returned by `list_sections`. Returns markdown."
    )]
    async fn get_documentation(
        &self,
        Parameters(args): Parameters<GetDocumentationArgs>,
    ) -> Result<Json<docs::SectionDoc>, ErrorData> {
        Ok(Json(docs::get_documentation(&self.state, args).await?))
    }

    #[tool(
        description = "Run `cargo clippy` on a Rust snippet and return diagnostics in JSON. Code is compiled in a tempdir; not a sandbox, only enable on trusted hosts or behind a network firewall."
    )]
    async fn clippy_check(
        &self,
        Parameters(args): Parameters<CodeArgs>,
    ) -> Result<Json<clippy_tool::ClippyReport>, ErrorData> {
        Ok(Json(clippy_tool::clippy_check(args).await?))
    }

    #[tool(
        description = "Run `cargo clippy --fix` on a Rust snippet and return the fixed source plus remaining diagnostics."
    )]
    async fn clippy_fix(
        &self,
        Parameters(args): Parameters<CodeArgs>,
    ) -> Result<Json<clippy_tool::ClippyFixReport>, ErrorData> {
        Ok(Json(clippy_tool::clippy_fix(args).await?))
    }

    #[tool(
        description = "Format a Rust snippet with `rustfmt`. Returns the formatted source. Honours rustfmt defaults, no project rustfmt.toml is loaded."
    )]
    async fn rustfmt(
        &self,
        Parameters(args): Parameters<CodeArgs>,
    ) -> Result<Json<rustfmt_tool::FmtReport>, ErrorData> {
        Ok(Json(rustfmt_tool::rustfmt(args).await?))
    }

    #[tool(
        description = "Build a shareable play.rust-lang.org permalink for a Rust snippet. Does NOT execute code, just returns the URL."
    )]
    async fn playground_link(
        &self,
        Parameters(args): Parameters<PlaygroundLinkArgs>,
    ) -> Result<Json<playground::PlaygroundLink>, ErrorData> {
        Ok(Json(playground::link(&self.state, args).await?))
    }

    #[tool(
        description = "Execute a Rust snippet on play.rust-lang.org and return stdout/stderr. The Playground enforces its own timeouts and resource caps; we never execute Rust on this server."
    )]
    async fn playground_run(
        &self,
        Parameters(args): Parameters<PlaygroundRunArgs>,
    ) -> Result<Json<playground::PlaygroundResult>, ErrorData> {
        Ok(Json(playground::run(&self.state, args).await?))
    }

    #[tool(
        description = "Search crates.io. Returns name, latest stable version, downloads, repo and description."
    )]
    async fn crate_search(
        &self,
        Parameters(args): Parameters<CrateSearchArgs>,
    ) -> Result<Json<crates_io::CrateSearchResult>, ErrorData> {
        Ok(Json(crates_io::search(&self.state, args).await?))
    }

    #[tool(
        description = "Crate metadata from crates.io: versions, features, deps, repo, docs URL."
    )]
    async fn crate_info(
        &self,
        Parameters(args): Parameters<CrateInfoArgs>,
    ) -> Result<Json<crates_io::CrateInfoResult>, ErrorData> {
        Ok(Json(crates_io::info(&self.state, args).await?))
    }

    #[tool(description = "Run `rustc --explain <CODE>` for a compiler error code such as E0382.")]
    async fn rustc_explain(
        &self,
        Parameters(args): Parameters<RustcExplainArgs>,
    ) -> Result<Json<rustc_explain::ExplainResult>, ErrorData> {
        Ok(Json(rustc_explain::explain(args).await?))
    }

    #[tool(
        description = "Returns shell instructions the calling LLM agent can execute on the *user's local machine* via its bash tool to compile, test or fuzz arbitrary Rust code. This server NEVER executes user-supplied code itself."
    )]
    async fn run_locally(
        &self,
        Parameters(args): Parameters<RunLocallyArgs>,
    ) -> Result<Json<run_locally::Plan>, ErrorData> {
        Ok(Json(run_locally::plan(args)))
    }
}

#[tool_handler]
impl ServerHandler for DocsBox {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "rust-docsbox-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Rust DocsBox MCP".into()),
                ..Default::default()
            },
            instructions: Some(INSTRUCTIONS.into()),
        }
    }
}

const INSTRUCTIONS: &str = r#"
You are connected to the Rust DocsBox MCP server.

Use this server when writing or reviewing Rust code. Workflow:

1. Call `list_sections` (optionally with `query` / `package`) to discover
   relevant doc paths. Then `get_documentation(section=...)` for the
   actual reference text. Cheaper than reading docs.rs HTML.
2. Before suggesting a snippet to the user, call `rustfmt` then `clippy_check`.
   If `clippy_check` returns lints, fix them and re-run.
3. To run code, prefer `playground_run`, it executes on
   play.rust-lang.org under their sandbox, not on this server.
4. To compile against a real local project, call `run_locally` and ask
   the user (or your own bash tool) to execute the returned commands.
5. For dependency questions use `crate_search` / `crate_info`.

This server NEVER executes user-supplied code. `clippy_*`, `rustfmt`,
and `rustc_explain` invoke the official toolchain on stdin/tempfiles.
"#;
