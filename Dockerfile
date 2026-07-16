# ============================================================
# Dalin L 2.0 — Production Docker Image (multi-stage)
# Stage 1: Rust compilation (slim toolchain)
# Stage 2: Minimal distroless runtime
# ============================================================

ARG RUST_VERSION=1.95.0
FROM rust:${RUST_VERSION}-slim AS builder

WORKDIR /builder
ENV CARGO_NET_RETRY=3
ENV CARGO_INCREMENTAL=0

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates curl git && \
    rm -rf /var/lib/apt/lists/*

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./

# Copy all crate sources
COPY compiler/ compiler/
COPY cli/ cli/
COPY control-plane/ control-plane/
COPY lsp/ lsp/

# Build all binaries in release
RUN cargo build --release --bin dalib --bin dalin-ls -p dalin-ls && \
    echo "=== Build complete ===" && \
    ls -lh target/release/dalib target/release/dalin-ls

# Stage 2: Distroless runtime
FROM gcr.io/distroless/cc-debian12

LABEL org.opencontainers.image.source="https://github.com/CN-QN1-dalin/dalin-l"
LABEL org.opencontainers.image.description="Dalin L 2.0 Compiler + CLI + Language Server"

WORKDIR /opt/dalan

# Copy built binaries
COPY --from=builder /builder/target/release/dalib /usr/local/bin/dalib
COPY --from=builder /builder/target/release/dalin-ls /usr/local/bin/dalin-ls
# Copy stdlib
COPY stdlib /opt/dalan/stdlib

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/dalib", "version"]

ENTRYPOINT ["/usr/local/bin/dalib"]
CMD ["run"]
