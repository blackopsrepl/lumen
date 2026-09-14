#!/usr/bin/env bash
#
# Shared configuration and helpers for the Playwright browser stack (B).
# Sourced by the other bin/*.sh scripts. Reads ./.env when present.
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

PLAYWRIGHT_IMAGE_VERSION="${PLAYWRIGHT_IMAGE_VERSION:-1.63.0}"
IMAGE="${BROWSER_IMAGE:-localhost/playwright-browser:${PLAYWRIGHT_IMAGE_VERSION}}"
CONTAINER_NAME="${BROKER_CONTAINER_NAME:-browser-broker}"

BROKER_HOST="${BROKER_HOST:-127.0.0.1}"
BROKER_PORT="${BROKER_PORT:-8090}"
DASHBOARD_PORT="${DASHBOARD_PORT:-8899}"
DASHBOARD_HOST="${DASHBOARD_HOST:-127.0.0.1}"

# All playwright-cli invocations share one workspace directory so its daemon
# and `show` dashboard see every agent session. The CLI keys its daemon on cwd.
PW_WORKSPACE="${PW_WORKSPACE:-${PROJECT_DIR}/.workspace}"
PW_CLI="${PW_CLI:-playwright-cli}"

export SCRIPT_DIR PROJECT_DIR PLAYWRIGHT_IMAGE_VERSION IMAGE CONTAINER_NAME
export BROKER_HOST BROKER_PORT DASHBOARD_PORT DASHBOARD_HOST PW_WORKSPACE PW_CLI

log() { printf '[%s] %s\n' "${0##*/}" "$*" >&2; }
die() { printf '[%s] error: %s\n' "${0##*/}" "$*" >&2; exit 1; }

broker_url() { printf 'http://%s:%s' "${BROKER_HOST}" "${BROKER_PORT}"; }

# Run podman compose against this project's compose file, from the project dir
# so it picks up ./.env.
compose() {
  ( cd "${PROJECT_DIR}" && exec podman compose -f "${PROJECT_DIR}/compose.yaml" "$@" )
}

container_exists() { podman container exists "$1" 2>/dev/null; }
image_exists() { podman image exists "${IMAGE}"; }

require_image() {
  image_exists || die "image ${IMAGE} not found; run bin/build.sh first"
}

require_cli() {
  command -v "${PW_CLI}" >/dev/null 2>&1 \
    || die "${PW_CLI} not found on PATH; run bin/install-host.sh"
}

# Run playwright-cli from the shared workspace so sessions/dashboard agree.
pw() {
  mkdir -p "${PW_WORKSPACE}"
  ( cd "${PW_WORKSPACE}" && exec "${PW_CLI}" "$@" )
}

# Ensure an agent's browser exists; echo its CDP URL.
broker_ensure() {
  local name="$1"
  curl -fsS -X POST "$(broker_url)/sessions" \
    -H 'content-type: application/json' \
    -d "{\"name\":\"${name}\"}" \
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["cdpUrl"])'
}

# Resolve the session name from args/env. Precedence: -s=/--session= arg,
# PLAYWRIGHT_CLI_SESSION, AGENT_NAME, "default".
parse_session() {
  local s="${PLAYWRIGHT_CLI_SESSION:-${AGENT_NAME:-default}}"
  local a
  for a in "$@"; do
    case "$a" in
      -s=*|--session=*) s="${a#*=}" ;;
    esac
  done
  printf '%s' "$s"
}

session_in_list() {
  pw list 2>/dev/null | grep -qE "^- $1:"
}

# Make sure <name>'s browser exists in the container and the host CLI is
# attached to it. Self-healing: if the broker restarted (or the browser died),
# the CDP endpoint changes and the stale CLI session is replaced automatically,
# so callers never have to re-run bin/session.sh.
ensure_attached() {
  local name="$1"
  [ -n "${name}" ] || die "empty session name"

  local state_dir="${PW_WORKSPACE}/.sessions"
  local state_file="${state_dir}/${name}"
  mkdir -p "${state_dir}"

  local cdp stored
  if cdp="$(broker_ensure "${name}" 2>/dev/null)"; then
    stored="$(cat "${state_file}" 2>/dev/null || true)"
    if [ "${cdp}" != "${stored}" ] || ! session_in_list "${name}"; then
      # Drop any stale session pointing at a dead endpoint, then re-attach.
      pw -s="${name}" detach >/dev/null 2>&1 || true
      pw -s="${name}" close >/dev/null 2>&1 || true
      pw attach --cdp="${cdp}" --session="${name}" >/dev/null 2>&1 \
        || return 1
      printf '%s' "${cdp}" > "${state_file}"
      # Codex-style live view: name each action and animate a mouse pointer so a
      # human watching the dashboard sees the cursor move. The overlay is
      # viewer-only — it does not appear in the agent's accessibility snapshots
      # (though it can appear in screenshots; set AGENT_ACTION_ANNOTATIONS=0 to
      # disable).
      if [ "${AGENT_ACTION_ANNOTATIONS:-1}" = "1" ]; then
        pw -s="${name}" video-show-actions --cursor pointer \
          --duration "${AGENT_ACTION_ANNOTATION_MS:-1200}" >/dev/null 2>&1 || true
      fi
    fi
    return 0
  fi

  # Broker unreachable: proceed only if an existing session still works.
  session_in_list "${name}"
}

