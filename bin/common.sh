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

# Keep the workspace absolute so the artifact paths pw.sh reports are
# resolvable from wherever the agent runs, not only from the checkout's parent.
case "${PW_WORKSPACE}" in
  /*) ;;
  *) PW_WORKSPACE="${PROJECT_DIR}/${PW_WORKSPACE}" ;;
esac

# The image is stamped with this revision so a deploy can tell which checkout
# the running container was built from. Image ids are not usable for that:
# every rebuild produces a new one because the layers carry file mtimes.
lumen_revision() {
  local revision dirty=""
  revision="$(git -C "${PROJECT_DIR}" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
  git -C "${PROJECT_DIR}" diff --quiet 2>/dev/null \
    && git -C "${PROJECT_DIR}" diff --cached --quiet 2>/dev/null \
    || dirty="-dirty"
  printf '%s%s' "${revision}" "${dirty}"
}
LUMEN_REVISION="${LUMEN_REVISION:-$(lumen_revision)}"

export PROJECT_DIR LUMEN_VERSION PLAYWRIGHT_IMAGE_VERSION IMAGE CONTAINER LUMEN_PORT PW_WORKSPACE PW_CLI LUMEN_REVISION

log() { printf '[%s] %s\n' "${0##*/}" "$*" >&2; }
die() { printf '[%s] error: %s\n' "${0##*/}" "$*" >&2; exit 1; }

# A session name becomes a URL segment, a log field, and a directory component
# under PW_WORKSPACE. This mirrors Supervisor::ensure's rule (src/supervisor.rs)
# so a bad name can never reach the filesystem, where '..' would expand to the
# workspace root and 'rm -rf' would delete it.
valid_session_name() {
  local name="$1"
  [ -n "${name}" ] && [ "${name}" != "." ] && [ "${name}" != ".." ] \
    && [ "${#name}" -le 32 ] \
    && [[ "${name}" =~ ^[A-Za-z0-9._-]+$ ]]
}

# Escape a string for use as a sed replacement with '|' as the delimiter, so a
# checkout path containing '&', '\', or '|' cannot corrupt a generated file.
sed_escape() { printf '%s' "$1" | sed -e 's/[\\&|]/\\&/g'; }

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

# Root of the CLI workspace. pw.sh owns everything under it: one directory per
# session (`sessions/<name>/`) holding that session's artifacts and endpoint
# binding. Keeping one marker at the root keeps every session in a single CLI
# workspace; without it the CLI falls back to a global default it shares with
# unrelated projects.
pw_workspace_init() {
  mkdir -p "${PW_WORKSPACE}/.playwright"
}

pw() {
  pw_workspace_init
  ( cd "${PW_WORKSPACE}" && exec "${PW_CLI}" "$@" )
}
