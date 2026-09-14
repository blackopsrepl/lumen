#!/usr/bin/env bash
#
# Shared configuration and helpers for the Lumen stack.
#
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

if [ -f "${PROJECT_DIR}/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "${PROJECT_DIR}/.env"
  set +a
fi

LUMEN_VERSION="${LUMEN_VERSION:-0.1.0}"
PLAYWRIGHT_IMAGE_VERSION="${PLAYWRIGHT_IMAGE_VERSION:-1.63.0}"
IMAGE="${LUMEN_IMAGE:-localhost/lumen:${LUMEN_VERSION}}"
CONTAINER="${LUMEN_CONTAINER:-lumen}"
LUMEN_PORT="${LUMEN_PORT:-8899}"
PW_WORKSPACE="${PW_WORKSPACE:-${PROJECT_DIR}/.workspace}"
PW_CLI="${PW_CLI:-playwright-cli}"

export PROJECT_DIR LUMEN_VERSION PLAYWRIGHT_IMAGE_VERSION IMAGE CONTAINER LUMEN_PORT PW_WORKSPACE PW_CLI

log() { printf '[%s] %s\n' "${0##*/}" "$*" >&2; }
die() { printf '[%s] error: %s\n' "${0##*/}" "$*" >&2; exit 1; }

compose() { ( cd "${PROJECT_DIR}" && exec podman compose -f "${PROJECT_DIR}/compose.yaml" "$@" ); }

require_cli() {
  command -v "${PW_CLI}" >/dev/null 2>&1 \
    || die "${PW_CLI} not found on PATH; run bin/install-host.sh"
}

# Run the Lumen client from inside the container (loopback is shared via host
# networking, so its view of the server is the same as the host's).
lumen() {
  podman exec "${CONTAINER}" lumen --url "http://127.0.0.1:${LUMEN_PORT}" "$@"
}

# Run playwright-cli from the shared workspace so its daemon and every agent
# session agree on one directory.
pw() {
  mkdir -p "${PW_WORKSPACE}"
  ( cd "${PW_WORKSPACE}" && exec "${PW_CLI}" "$@" )
}
