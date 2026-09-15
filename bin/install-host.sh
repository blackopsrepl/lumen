#!/usr/bin/env bash
#
# Install the Playwright CLI on the host. Agents drive the Lumen-managed
# Chromium by attaching over CDP, so no local browser is required.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

command -v npm >/dev/null 2>&1 || die "npm not found on PATH"

# Pinned: the CLI holds the CDP routing and artifact layout agents depend on,
# so it ships as a tested pair with the service rather than tracking latest.
version="${CLI_VERSION:-0.1.19}"
prefix="${NPM_PREFIX:-${HOME}/.local}"
log "installing @playwright/cli@${version} into ${prefix}"
npm install -g --prefix "${prefix}" "@playwright/cli@${version}"

command -v "${PW_CLI}" >/dev/null 2>&1 || die "${PW_CLI} not on PATH; is ${prefix}/bin on your PATH?"
log "installed: $("${PW_CLI}" --version 2>&1 | head -1)"
