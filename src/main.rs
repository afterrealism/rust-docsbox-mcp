//! rust-docsbox-mcp, streamable-HTTP MCP server for Rust documentation,
//! clippy, rustfmt, crates.io lookups and Rust Playground integration.
//!
//! Bind defaults to 127.0.0.1:7801. In a container, set RUST_DOCSBOX_BIND
//! to 0.0.0.0:7801, the public surface is fronted by Cloudflare and the
//! origin Cloudflare Container is the only thing that talks to it.

mod corpus;
mod server;
mod tools;
mod web;

use std::net::SocketAddr;

use anyhow::Context;
use axum::{routing::get, Router};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::server::DocsBox;

const DEFAULT_BIND: &str = "127.0.0.1:7801";
/// Cap any single MCP request body at 1 MiB. Clippy/rustfmt snippets are
/// always small; rejecting big payloads here avoids spending CPU on abuse.
const MAX_BODY: usize = 1024 * 1024;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let bind: SocketAddr = std::env::var("RUST_DOCSBOX_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()
        .context("RUST_DOCSBOX_BIND is not a valid socket address")?;

    let corpus = corpus::Corpus::load_default()?;
    let state = server::AppState::new(corpus);

    // MCP streamable-HTTP service, mounted at /mcp.
    let mcp_state = state.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(DocsBox::new(mcp_state.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let app = Router::new()
        .route("/", get(web::landing))
        .route("/health", get(web::health))
        .route("/tools", get(web::tools_index))
        .route("/robots.txt", get(web::robots))
        .route("/sitemap.xml", get(web::sitemap))
        .route("/llms.txt", get(web::llms_txt))
        .route("/llms-full.txt", get(web::llms_full_txt))
        .nest_service("/mcp", mcp_service)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(RequestBodyLimitLayer::new(MAX_BODY))
        .with_state(state);

    tracing::info!(%bind, "rust-docsbox-mcp listening");
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind {bind}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum::serve failed")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rmcp=info,tower_http=warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sig) = signal(SignalKind::terminate()) {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutdown signal received");
}
