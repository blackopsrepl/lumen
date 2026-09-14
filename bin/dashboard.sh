#!/usr/bin/env bash
#
# Open the live, annotatable Playwright dashboard, bound to localhost.
#
#   bin/dashboard.sh
#
# It shows every attached agent session with a live screencast preview; click a
# session to zoom in and take over mouse/keyboard. `--annotate` mode (draw a
# box + comment, returned to the caller) is available via:
#
#   bin/dashboard.sh --annotate
#
# This blocks (it is an HTTP server). Bind address is DASHBOARD_HOST
# (default 127.0.0.1). Verify with: ss -ltn | grep "${DASHBOARD_PORT}"
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

require_cli

log "dashboard: http://${DASHBOARD_HOST}:${DASHBOARD_PORT}/  (host: ${DASHBOARD_HOST})"
pw show --port="${DASHBOARD_PORT}" --host="${DASHBOARD_HOST}" "$@"
