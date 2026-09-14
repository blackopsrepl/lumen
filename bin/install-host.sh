#!/usr/bin/env bash
#
# Install the Playwright CLI on the host. The CLI (and its dashboard) runs where
# the agent runs; the browser runs in the container.
#
#   bin/install-host.sh
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

command -v npm >/dev/null 2>&1 || die "npm not found on PATH"

version="${CLI_VERSION:-latest}"
prefix="${NPM_PREFIX:-${HOME}/.local}"
log "installing @playwright/cli@${version} into ${prefix}"
npm install -g --prefix "${prefix}" "@playwright/cli@${version}"

command -v "${PW_CLI}" >/dev/null 2>&1 || die "${PW_CLI} not on PATH; is ${prefix}/bin on your PATH?"
log "installed: $("${PW_CLI}" --version 2>&1 | head -1)"

# The dashboard's annotation UI runs as a local Chromium app window, so the host
# CLI needs its own matching browser (the container's browser is not used for
# the dashboard UI). Agent-driven sessions still run in the container.
log "installing the CLI's browser (for the dashboard/annotation window)"
"${PW_CLI}" install-browser chromium

log "drive a browser with: bin/session.sh <agent> && bin/pw.sh -s=<agent> goto https://example.com"
