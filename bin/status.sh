#!/usr/bin/env bash
#
# Show Lumen's container state, health, and active sessions.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

if container_exists "${CONTAINER}"; then
  ctr ps -a --filter "name=^${CONTAINER}$" \
    --format 'container: {{.Names}}  state: {{.Status}}'
else
  log "container ${CONTAINER} is not present"
fi

if HEALTH="$(curl -fsS --max-time 5 "http://127.0.0.1:${LUMEN_PORT}/healthz" 2>/dev/null)"; then
  printf 'lumen: http://127.0.0.1:%s  OK  %s\n' "${LUMEN_PORT}" "${HEALTH}"
else
  printf 'lumen: http://127.0.0.1:%s  UNREACHABLE\n' "${LUMEN_PORT}"
  exit 1
fi

printf 'sessions:\n'
if ! lumen status; then
  printf 'warning: could not list sessions\n' >&2
  exit 1
fi
