# Smoke test container for schalentier
# Uses pre-built binary - build with: cargo build --release
# Run with: ./tests/smoke/run.sh

FROM debian:bookworm-slim AS smoke-debian

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Static musl binary - works on any Linux
COPY target/x86_64-unknown-linux-musl/release/schalentier /usr/local/bin/schalentier
COPY tests/smoke/run_smoke_tests.sh /run_smoke_tests.sh
RUN chmod 755 /run_smoke_tests.sh /usr/local/bin/schalentier

# Create non-root user for testing
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

ENTRYPOINT ["/bin/bash", "/run_smoke_tests.sh"]

# Arch Linux variant
FROM archlinux:latest AS smoke-arch

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm ca-certificates curl git python && \
    pacman -Scc --noconfirm

COPY target/x86_64-unknown-linux-musl/release/schalentier /usr/local/bin/schalentier
COPY tests/smoke/run_smoke_tests.sh /run_smoke_tests.sh
RUN chmod 755 /run_smoke_tests.sh /usr/local/bin/schalentier

# Create non-root user for testing
RUN useradd -m -s /bin/bash testuser
USER testuser
WORKDIR /home/testuser

ENTRYPOINT ["/bin/bash", "/run_smoke_tests.sh"]
