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

LUMEN_VERSION="${LUMEN_VERSION:-0.13.4}"
PLAYWRIGHT_IMAGE_VERSION="${PLAYWRIGHT_IMAGE_VERSION:-1.63.0}"
IMAGE="${LUMEN_IMAGE:-localhost/lumen:${LUMEN_VERSION}}"
CONTAINER="${LUMEN_CONTAINER:-lumen}"
LUMEN_PORT="${LUMEN_PORT:-8899}"
PW_WORKSPACE="${PW_WORKSPACE:-${PROJECT_DIR}/.workspace}"
PW_CLI="${PW_CLI:-playwright-cli}"

log() { printf '[%s] %s\n' "${0##*/}" "$*" >&2; }
die() { printf '[%s] error: %s\n' "${0##*/}" "$*" >&2; exit 1; }

# Container runtime: podman or docker. Override with LUMEN_RUNTIME; otherwise
# prefer podman when present so existing installs keep their behavior, and fall
# back to docker.
_detect_runtime() {
  local override="${LUMEN_RUNTIME:-}"
  if [ -n "${override}" ]; then
    case "${override}" in
      podman|docker) printf '%s' "${override}" ;;
      *) die "unsupported container runtime: ${override} (expected podman or docker)" ;;
    esac
    return
  fi
  if command -v podman >/dev/null 2>&1; then
    printf 'podman'
  elif command -v docker >/dev/null 2>&1; then
    printf 'docker'
  else
    die "no container runtime found; install podman or docker"
  fi
}
# Resolve the runtime only when a caller actually needs container operations.
# Runtime-independent helpers, such as the disposable E2E suite, also source
# this file and must work inside environments without Podman or Docker.
RUNTIME_READY=0
CTR_SUDO=()
ensure_runtime() {
  [ "${RUNTIME_READY}" -eq 1 ] && return

  CONTAINER_RUNTIME="$(_detect_runtime)"
  CTR="${CTR:-${CONTAINER_RUNTIME}}"
  export CONTAINER_RUNTIME CTR

  # Docker elevation: on many distros the user cannot reach the system daemon
  # (/var/run/docker.sock is root:docker and the user is in no docker group).
  # Resolve whether to prefix docker with sudo. LUMEN_DOCKER_SUDO=1 forces it,
  # =0 forbids it, and the default (auto) uses passwordless sudo only when
  # direct access fails, so scripts never surprise you with a password prompt
  # unless you opted in. A reachable DOCKER_HOST (e.g. rootless) just works.
  if [ "${CTR}" = "docker" ]; then
    case "${LUMEN_DOCKER_SUDO:-auto}" in
      1|true|yes) CTR_SUDO=(sudo) ;;
      0|false|no) CTR_SUDO=() ;;
      auto|"")
        if docker info >/dev/null 2>&1; then
          CTR_SUDO=()
        elif sudo -n docker info >/dev/null 2>&1; then
          log "docker needs privilege here; using 'sudo docker' (override with LUMEN_DOCKER_SUDO=0)"
          CTR_SUDO=(sudo)
        else
          die "docker daemon not reachable and no passwordless sudo for docker. Fix one of: add yourself to the docker group ('sudo usermod -aG docker ${USER}' + re-login, or 'newgrp docker'); set LUMEN_DOCKER_SUDO=1 in .env to allow a sudo password prompt; or point DOCKER_HOST at a daemon you can reach (e.g. rootless unix://\${XDG_RUNTIME_DIR}/docker.sock)"
        fi
        ;;
      *) die "unsupported LUMEN_DOCKER_SUDO: ${LUMEN_DOCKER_SUDO} (expected 1, 0, or auto)" ;;
    esac
  fi
  RUNTIME_READY=1
}

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

compose() {
  ensure_runtime
  case "${CTR}" in
    podman)
      # Prefer the `podman compose` plugin; fall back to the standalone
      # `podman-compose` script on hosts that only have that.
      if podman compose version >/dev/null 2>&1; then
        ( cd "${PROJECT_DIR}" && exec podman compose -f "${PROJECT_DIR}/compose.yaml" "$@" )
      else
        ( cd "${PROJECT_DIR}" && exec podman-compose -f "${PROJECT_DIR}/compose.yaml" "$@" )
      fi
      ;;
    docker)
      ( cd "${PROJECT_DIR}" && exec "${CTR_SUDO[@]}" docker compose -f "${PROJECT_DIR}/compose.yaml" "$@" )
      ;;
    *) die "unsupported container runtime: ${CTR}" ;;
  esac
}

# Portable `podman container exists` equivalent: both runtimes exit 0 from
# `inspect` when the container exists.
container_exists() { ctr inspect "$1" >/dev/null 2>&1; }

ctr() {
  ensure_runtime
  "${CTR_SUDO[@]}" "${CTR}" "$@"
}

require_cli() {
  command -v "${PW_CLI}" >/dev/null 2>&1 \
    || die "${PW_CLI} not found on PATH; run bin/install-host.sh"
}

# Run the Lumen client from inside the container (loopback is shared via host
# networking, so its view of the server is the same as the host's).
lumen() {
  ctr exec "${CONTAINER}" lumen --url "http://127.0.0.1:${LUMEN_PORT}" "$@"
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
