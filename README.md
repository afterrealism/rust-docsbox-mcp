# rust-docsbox-mcp

A streamable-HTTP [Model Context Protocol](https://modelcontextprotocol.io)
server that gives LLM coding agents typed access to:

- Rust standard library + popular crate documentation (offline corpus, online fallback)
- `cargo clippy` (check + auto-fix) on user-supplied snippets
- `rustfmt` formatting
- The official [Rust Playground](https://play.rust-lang.org) (link & execute)
- crates.io search and crate metadata
- `rustc --explain <code>` lookups
- A `run_locally` planner that emits shell-command plans the calling agent
  is expected to run on the user's machine via its own bash tool

The server itself **never executes user-supplied code**, clippy/rustfmt
run against tempdirs in a strict timeout, and "run this snippet" is
delegated to play.rust-lang.org.

Public deployment: <https://rust-mcp.afterrealism.com/mcp>

## Tools

| Tool | Description |
|---|---|
| `list_sections` | List indexed Rust documentation sections (filterable by `query`/`package`) |
| `get_documentation` | Markdown of a single section path |
| `clippy_check` | Lint a Rust snippet, JSON diagnostics |
| `clippy_fix` | Apply clippy suggestions and return the fixed source |
| `rustfmt` | Format a snippet via `rustfmt --emit stdout` |
| `playground_link` | Build a play.rust-lang.org permalink |
| `playground_run` | Execute a snippet on the official Playground |
| `crate_search` | Search crates.io |
| `crate_info` | Versions, features, deps, repo, docs URL |
| `rustc_explain` | `rustc --explain <code>` |
| `run_locally` | Plan of shell commands the calling agent should run on the user's machine |

## Use it from your editor

### OpenCode (`~/.config/opencode/opencode.json`)

```json
{
  "mcp": {
    "rust-docsbox": {
      "type": "remote",
      "url": "https://rust-mcp.afterrealism.com/mcp",
      "enabled": true
    }
  }
}
```

### Claude Code

```bash
claude mcp add --transport http rust-docsbox https://rust-mcp.afterrealism.com/mcp
```

### Cursor / Continue / any MCP-aware client

Point it at `https://rust-mcp.afterrealism.com/mcp`. Transport is
streamable HTTP per MCP spec 2025-06-18.

## Run it yourself

### Docker

```bash
docker build -t rust-docsbox-mcp .
docker run --rm -p 127.0.0.1:7801:7801 \
  --memory=2g --cpus=2 --pids-limit=512 \
  rust-docsbox-mcp
```

The image is `rust:1.85-slim` based and already contains `cargo`,
`rustc`, `clippy` and `rustfmt`. ~250 MB compressed.

### Cargo

```bash
cargo run --release
# server starts on 127.0.0.1:7801
curl http://127.0.0.1:7801/health
```

### Configuration

| Env | Default | Purpose |
|---|---|---|
| `RUST_DOCSBOX_BIND` | `127.0.0.1:7801` | Listening socket |
| `RUST_DOCSBOX_CORPUS_DIR` | `corpus` | Path to `index.sqlite` + `blobs/` (or `manifest.toml` fallback) |
| `RUST_DOCSBOX_TARGET_DIR` | `$TMPDIR/rust-docsbox/target` | Shared `CARGO_TARGET_DIR` for clippy invocations |
| `RUST_LOG` | `info` | tracing-subscriber filter |

## Endpoints

- `GET /`, landing page (HTML)
- `GET /health`, JSON health probe
- `GET /tools`, JSON tool index
- `POST /mcp`, MCP streamable-HTTP endpoint

## Security model

- Body cap: 1 MiB per request
- All cargo/rustfmt subprocess calls run with timeouts (15–60s)
- Container runs as a non-root user (uid 10001)
- No outbound traffic except crates.io, docs.rs, doc.rust-lang.org and play.rust-lang.org
- Bind defaults to `127.0.0.1`; only flipped to `0.0.0.0` inside the container
- We never trust the snippet to be safe, but we also never link or
  execute it. Clippy / rustfmt only parse + analyse it.

## Building the doc corpus

The repository ships with a small `corpus/manifest.toml` fallback
listing the most useful sections by name. To build the full SQLite
index with rendered markdown blobs, run the corpus builder:

```bash
# (Builder ships in a follow-up commit.)
cargo run --bin build_corpus -- \
  --packages std,tokio,axum,serde,sqlx,reqwest,anyhow \
  --out corpus/
```

This produces `corpus/index.sqlite` + `corpus/blobs/*.md.zst`, which
the runtime opens read-only.

## License

MIT, see `LICENSE`.
