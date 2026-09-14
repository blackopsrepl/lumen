# Lumen — single-binary browser service.
#
# Multi-stage: compile the Rust service against a stub so dependency layers
# cache, then copy only the binary onto the official Playwright image, which
# already carries Chromium and every OS library it needs.

ARG PLAYWRIGHT_IMAGE_VERSION=1.63.0
ARG RUST_IMAGE=rust:1.95-slim

FROM ${RUST_IMAGE} AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src ui \
 && echo '' > src/lib.rs \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > ui/index.html \
 && cargo build --release --locked \
 && rm -rf src ui
COPY src ./src
COPY ui ./ui
# COPY preserves source mtimes, which can predate the stub build; touch the tree
# so cargo rebuilds against the real sources instead of the cached stub.
RUN find src ui -type f -exec touch {} + && cargo build --release --locked

FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_IMAGE_VERSION}-noble

RUN ln -sf "$(ls /ms-playwright/chromium-*/chrome-linux*/chrome | head -1)" /usr/local/bin/chromium \
 && chromium --version \
 && mkdir -p /data /etc/lumen \
 && chown -R ubuntu:ubuntu /data

COPY --from=builder /src/target/release/lumen /usr/local/bin/lumen
COPY config/lumen.toml /etc/lumen/lumen.toml

ENV LUMEN_CONFIG=/etc/lumen/lumen.toml \
    HOME=/home/ubuntu \
    RUST_LOG=lumen=info

VOLUME ["/data"]
EXPOSE 8899
ENTRYPOINT ["lumen"]
