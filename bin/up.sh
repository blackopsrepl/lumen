#!/usr/bin/env bash
#
# Start the Lumen service (detached).
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "starting lumen (compose)"
compose up -d --build
log "lumen: http://127.0.0.1:${LUMEN_PORT}/"
