# Smoke test container for schalentier.
#
# Self-building: the binary is compiled inside a builder stage, so no host
# toolchain or pre-built binary is required. Run with: ./tests/smoke/run.sh
#
# We build against glibc rather than static musl simply because a glibc binary
# built on the Debian-based builder runs unmodified on both the Debian and Arch
# test stages below (a musl build is used for release artifacts, see build-musl.sh).

# --- Builder: compile the release binary ---
# All stages share one architecture; run.sh passes --platform matching the host so a
# stale mismatched image in the local cache (e.g. an arm64 debian pull) can't sneak in.
#
# Pinned to a bookworm-based Rust image (glibc 2.36) on purpose: `rust:latest` tracks
# a newer Debian (glibc 2.39) and produces a binary that the debian:bookworm-slim test
# stage can't run ("GLIBC_2.39 not found"). Building against the oldest runtime keeps
# the binary compatible with both the Debian and Arch test stages.
FROM rust:1-bookworm AS builder
WORKDIR /build
# Copy only the build inputs (not tests/, docs, etc.) so that editing the smoke
# script or docs doesn't bust the expensive compile layer. registry/ is included
# because it's embedded into the binary via include_str! at compile time.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY registry ./registry
RUN cargo build --release --bin schalentier
# Sanity-check the binary links and runs in the builder before shipping it.
RUN /build/target/release/schalentier --version

# --- Debian test stage ---
FROM debian:bookworm-slim AS smoke-debian

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    python3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/schalentier /usr/local/bin/schalentier
COPY tests/smoke/run_smoke_tests.sh /run_smoke_tests.sh
COPY tests/smoke/mock_gist_server.py /run_mock_gist_server.py
RUN chmod 755 /run_smoke_tests.sh /usr/local/bin/schalentier

# Create non-root user for testing
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

# Non-interactive master password for secret smoke tests (no keyring/TTY in container).
ENV SCHALENTIER_MASTER_PASSWORD=smoke-test-password-123

ENTRYPOINT ["/bin/bash", "/run_smoke_tests.sh"]

# --- Arch Linux test stage ---
FROM archlinux:latest AS smoke-arch

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm ca-certificates curl git python && \
    pacman -Scc --noconfirm

COPY --from=builder /build/target/release/schalentier /usr/local/bin/schalentier
COPY tests/smoke/run_smoke_tests.sh /run_smoke_tests.sh
COPY tests/smoke/mock_gist_server.py /run_mock_gist_server.py
RUN chmod 755 /run_smoke_tests.sh /usr/local/bin/schalentier

# Create non-root user for testing
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

ENV SCHALENTIER_MASTER_PASSWORD=smoke-test-password-123

ENTRYPOINT ["/bin/bash", "/run_smoke_tests.sh"]
