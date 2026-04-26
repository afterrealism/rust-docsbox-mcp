//! HTTP endpoints that aren't MCP: landing page, /health, /tools.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::server::AppState;

/// Static landing page. The HTML is baked into the binary so we don't
/// need to mount any volume to serve it.
pub async fn landing() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("web/index.html"),
    )
}

/// `robots.txt`, explicit allow-list for major search and AI/LLM crawlers,
/// disallows the MCP transport endpoint to keep crawlers out of the JSON-RPC
/// surface. Baked into the binary at build time.
pub async fn robots() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        include_str!("web/robots.txt"),
    )
}

/// `sitemap.xml`, sitemaps.org 0.9 schema, lists the indexable URLs.
pub async fn sitemap() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        include_str!("web/sitemap.xml"),
    )
}

/// `llms.txt`, llmstxt.org spec index for LLM agents.
pub async fn llms_txt() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        include_str!("web/llms.txt"),
    )
}

/// `llms-full.txt`, full markdown reference dump for LLM agents.
pub async fn llms_full_txt() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        include_str!("web/llms-full.txt"),
    )
}

pub async fn health(State(state): State<AppState>) -> Response {
    let n = state.corpus.list("", None).map(|v| v.len()).unwrap_or(0);
    Json(json!({
        "ok": true,
        "name": "rust-docsbox-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "corpus_sections": n,
    }))
    .into_response()
}

pub async fn tools_index() -> Response {
    Json(json!({
        "tools": [
            {"name": "list_sections", "description": "List indexed Rust documentation sections"},
            {"name": "get_documentation", "description": "Fetch the markdown of a single section"},
            {"name": "clippy_check", "description": "Run cargo clippy on a snippet"},
            {"name": "clippy_fix", "description": "Run cargo clippy --fix on a snippet"},
            {"name": "rustfmt", "description": "Format a snippet via rustfmt --emit stdout"},
            {"name": "playground_link", "description": "Build a play.rust-lang.org permalink"},
            {"name": "playground_run", "description": "Execute a snippet on play.rust-lang.org"},
            {"name": "crate_search", "description": "Search crates.io"},
            {"name": "crate_info", "description": "Crate metadata, deps, features, versions"},
            {"name": "rustc_explain", "description": "Run rustc --explain <code>"},
            {"name": "run_locally", "description": "Emit a plan of shell commands the calling agent should run on the user's machine"}
        ],
        "mcp_endpoint": "/mcp"
    }))
    .into_response()
}
