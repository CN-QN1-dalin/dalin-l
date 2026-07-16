# ============================================================
# Dalin L Runtime — minimal image for task execution only
# (Compiler + LSP NOT included — just runtime workers)
# ============================================================

FROM debian:bookworm-slim AS builder

ARG DALIN_LRS_DIR=/workspace/dalin-l
WORKDIR ${DALIN_LRS_DIR}

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates curl git && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY compiler/ compiler/
COPY cli/ cli/
COPY control-plane/ control-plane/
COPY stdlib/ stdlib/

# Build only dalib (runtime binary), skip LSP for faster build
RUN cargo build --release --bin dalib 2>&1 && \
    echo "=== dalib runtime built ===" && \
    ls -lh target/release/dalib

# ── Stage 2: Distroless runtime ──
FROM gcr.io/distroless/cc-debian12

LABEL org.opencontainers.image.source="https://github.com/CN-QN1-dalin/dalin-l"
LABEL io.k8s.description="Dalin L 2.0 Task Runner — execute compiled .dal agent workloads"

USER 65534:65534
WORKDIR /opt/dalan

COPY --from=builder /workspace/dalin-l/target/release/dalib /usr/local/bin/dalib
COPY --from=builder /workspace/dalin-l/stdlib /opt/dalan/stdlib

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD ["/usr/local/bin/dalib", "version"]

ENTRYPOINT ["/usr/local/bin/dalib"]
CMD ["cpd", "--mode", "runner", "--scheduler-addr", "http://dalin-scheduler:50051"]
