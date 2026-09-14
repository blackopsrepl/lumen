#!/usr/bin/env bash
#
# Stop the browser broker (agent profile volumes are kept; use
# `podman compose down -v` to delete them).
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "stopping browser broker"
compose down
