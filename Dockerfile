# syntax=docker/dockerfile:1.7

# ---------- Stage 1: build the binary ----------
FROM rust:1.86-slim-bookworm AS builder
WORKDIR /src

# Cache deps separately from source for faster rebuilds.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release --locked || cargo build --release && \
    rm -rf src

COPY . .
RUN cargo build --release --locked || cargo build --release && \
    strip target/release/rust-docsbox-mcp || true

# ---------- Stage 2: runtime ----------
# We need the rust toolchain at runtime so clippy_check / rustfmt /
# rustc_explain work. `rust:slim` is the cheapest official option that
# already has cargo + rustc + rustfmt + clippy.
FROM rust:1.86-slim-bookworm AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Confirm clippy + rustfmt are present (rust:slim ships them, but be defensive).
RUN rustup component add clippy rustfmt || true

# Non-root user.
RUN useradd --system --create-home --uid 10001 docsbox
USER docsbox
WORKDIR /home/docsbox/app

COPY --from=builder /src/target/release/rust-docsbox-mcp /usr/local/bin/rust-docsbox-mcp
COPY --chown=docsbox:docsbox corpus ./corpus

ENV RUST_DOCSBOX_BIND=0.0.0.0:7801 \
    RUST_DOCSBOX_CORPUS_DIR=/home/docsbox/app/corpus \
    RUST_DOCSBOX_TARGET_DIR=/home/docsbox/scratch-target \
    RUST_LOG=info

# Pre-create the writable target dir so the first invocation doesn't
# race the volume mount.
RUN mkdir -p /home/docsbox/scratch-target

EXPOSE 7801

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:7801/health || exit 1

ENTRYPOINT ["/usr/local/bin/rust-docsbox-mcp"]
