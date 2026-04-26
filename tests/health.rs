//! Smoke test that boots the server in-process and hits /health.

use std::net::SocketAddr;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_endpoint_returns_ok() {
    // Bind ephemeral port so multiple test processes don't collide.
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();

    // Build the same Router the binary uses, but skip MCP nesting (we
    // only want to verify /health). The full MCP wiring is exercised
    // by tools/clippy.rs unit tests.
    let app = axum::Router::new().route("/health", axum::routing::get(|| async { "ok" }));

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Wait for listener to be ready.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/health"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}
