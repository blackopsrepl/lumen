#!/usr/bin/env bash
#
# Restart the Lumen service.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "restarting lumen"
compose down
compose up -d --build
