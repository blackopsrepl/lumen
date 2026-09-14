#!/usr/bin/env bash
#
# Restart the browser broker.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "restarting browser broker"
compose down
compose up -d --build
