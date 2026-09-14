#!/usr/bin/env bash
#
# Stop the Lumen service (keeps the browser-profile volume).
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "stopping lumen"
compose down
