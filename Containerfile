# Playwright browser stack
#
# Container role (architecture "B"): a *browser server*, not an MCP server.
# It runs one isolated Chromium per agent and exposes each on its own loopback
# CDP port. The agent's `playwright-cli` (running on the host) attaches to that
# endpoint; `playwright-cli show` is the live, annotatable dashboard.
#
# Base image: official Microsoft Playwright image (Ubuntu 24.04 "noble") with
# Chromium, Firefox, WebKit and every OS library they need already present.
# We do not install the MCP server here.
#
# Build arg moves the single version pin:
#   --build-arg PLAYWRIGHT_IMAGE_VERSION=1.63.0

ARG PLAYWRIGHT_IMAGE_VERSION=1.63.0
FROM mcr.microsoft.com/playwright:v${PLAYWRIGHT_IMAGE_VERSION}-noble

ARG PLAYWRIGHT_IMAGE_VERSION

ENV DEBIAN_FRONTEND=noninteractive \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
    PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

USER root

# Stable chromium entrypoint for the broker. The base image path embeds a
# browser revision that changes between releases, so resolve it once at build.
RUN ln -sf "$(ls /ms-playwright/chromium-*/chrome-linux*/chrome | head -1)" /usr/local/bin/chromium \
 && chromium --version

# Writable agent profile root. By default the container runs as root (rootless:
# maps to your host user, so bind-mounted /data stays host-owned).
RUN mkdir -p /data/agents \
 && chown -R ubuntu:ubuntu /data

COPY browser-broker.js /usr/local/bin/browser-broker.js
COPY entrypoint.sh /usr/local/bin/playwright-browser-entrypoint
COPY healthcheck.sh /usr/local/bin/playwright-browser-healthcheck
RUN chmod 0755 \
      /usr/local/bin/browser-broker.js \
      /usr/local/bin/playwright-browser-entrypoint \
      /usr/local/bin/playwright-browser-healthcheck

ENV HOME=/home/ubuntu \
    BROKER_HOST=127.0.0.1 \
    BROKER_PORT=8090 \
    AGENT_DATA_DIR=/data/agents \
    AGENT_HEADLESS=1 \
    AGENT_NO_SANDBOX=1 \
    MAX_AGENTS=8 \
    CHROME_BIN=chromium

VOLUME ["/data"]
EXPOSE 8090

USER root
WORKDIR /home/ubuntu
ENTRYPOINT ["playwright-browser-entrypoint"]
CMD []
