# Lumen — single-binary browser and desktop session service.
#
# Multi-stage: compile the Rust service against a stub so dependency layers
# cache, then copy only the binary onto the official Playwright image, which
# already carries Chromium and every OS library it needs. Desktop sessions use
# the Ubuntu 25.10 desktop packages because Quickshell requires Qt 6.6+.

ARG PLAYWRIGHT_IMAGE_VERSION=1.63.0
ARG RUST_IMAGE=rust:1.95-slim

FROM ${RUST_IMAGE} AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# The manifest and every workspace member manifest, so the stub build can
# resolve the workspace. The lumen-ratatui crate is a path dependency of the
# service, so its manifest and declared targets (the trex example) must exist
# even for the stub compile; only its lib source is stubbed.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN mkdir -p src ui \
 && echo '' > src/lib.rs \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > ui/index.html \
 && echo '' > crates/lumen-ratatui/src/lib.rs \
 && cargo build --release --locked \
 && rm -rf src ui
COPY src ./src
COPY ui ./ui
# Re-copy the real crate sources over the stub from the cached layer.
COPY crates ./crates
# COPY preserves source mtimes, which can predate the stub build; touch the tree
# so cargo rebuilds against the real sources instead of the cached stub.
RUN find src ui crates -type f -exec touch {} + && cargo build --release --locked

FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_IMAGE_VERSION}-noble

# Stamp the checkout this image was built from, so `bin/up.sh` can recreate the
# container when the running image predates the current revision.
ARG LUMEN_REVISION=unknown
LABEL org.opencontainers.image.revision=$LUMEN_REVISION

RUN export DEBIAN_FRONTEND=noninteractive \
  && apt-get update \
  && apt-get install -y --no-install-recommends curl gnupg sway wtype grim xwayland tmux dbus at-spi2-core \
  && install -d -m 0755 /etc/apt/keyrings \
  && curl -4fsSL 'https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x45FECBE587307AAA3F0A4BE9FC44813D2A7788B7' \
       | gpg --batch --dearmor -o /etc/apt/keyrings/avengemedia-danklinux.gpg \
  && test "$(gpg --show-keys --with-colons /etc/apt/keyrings/avengemedia-danklinux.gpg | awk -F: '$1 == "fpr" { print $10; exit }')" = '45FECBE587307AAA3F0A4BE9FC44813D2A7788B7' \
  && printf '%s\n' \
       'deb [signed-by=/etc/apt/keyrings/avengemedia-danklinux.gpg] https://ppa.launchpadcontent.net/avengemedia/danklinux/ubuntu questing main' \
       'deb http://archive.ubuntu.com/ubuntu/ questing main universe' \
       'deb http://archive.ubuntu.com/ubuntu/ questing-updates main universe' \
       'deb http://security.ubuntu.com/ubuntu questing-security main universe' \
       > /etc/apt/sources.list.d/ubuntu-questing.list \
   && apt-get update \
   && apt-get install -y --no-install-recommends \
        quickshell qml6-module-qtquick-controls qml6-module-qtquick-layouts \
        qml6-module-qtquick-dialogs qml6-module-qtquick-window \
        libqt6concurrent6 \
   && command -v sway \
   && command -v Xwayland \
   && command -v quickshell \
   && command -v wtype \
   && command -v grim \
   && command -v tmux \
   && dbus-daemon --version >/dev/null \
   && find /usr/libexec -name at-spi-bus-launcher -print -quit | grep -q . \
 && rm -rf /var/lib/apt/lists/* \
 && ln -sf "$(ls /ms-playwright/chromium-*/chrome-linux*/chrome | head -1)" /usr/local/bin/chromium \
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
