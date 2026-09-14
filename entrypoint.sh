#!/usr/bin/env bash
#
# Playwright browser container entrypoint.
#
# Modes (BROKER_MODE):
#   broker   run the browser broker (default)
#   shell    interactive bash inside the image
#   <other>  anything else is exec'd verbatim
#
set -euo pipefail

log() { printf '[playwright-browser] %s\n' "$*" >&2; }

# Resolve HOME/XDG_RUNTIME_DIR for the effective uid. Chromium expects both to
# be owned by the running user.
if command -v getent >/dev/null 2>&1; then
  resolved_home="$(getent passwd "$(id -u)" | cut -d: -f6 || true)"
  [ -n "${resolved_home}" ] && export HOME="${resolved_home}"
fi

uid="$(id -u)"
runtime_dir="${XDG_RUNTIME_DIR:-/tmp/runtime-${uid}}"
mkdir -p "${runtime_dir}"
chown "${uid}:$(id -g)" "${runtime_dir}" 2>/dev/null || true
chmod 700 "${runtime_dir}"
export XDG_RUNTIME_DIR="${runtime_dir}"

# Explicit command-line arguments always win, so `podman run IMAGE --help`
# or `podman run IMAGE chromium --version` behave as expected.
if [ "$#" -gt 0 ]; then
  exec "$@"
fi

case "${BROKER_MODE:-broker}" in
  broker)
    log "starting browser broker on ${BROKER_HOST:-127.0.0.1}:${BROKER_PORT:-8090}"
    exec node /usr/local/bin/browser-broker.js
    ;;
  shell)
    exec bash
    ;;
  *)
    exec "${BROKER_MODE}"
    ;;
esac
