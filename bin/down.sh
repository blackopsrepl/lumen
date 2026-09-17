#!/usr/bin/env bash
#
# Stop the Lumen service. The /data volume (feedback database) is kept;
# session profiles are ephemeral and are cleared on the next start.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "stopping lumen"
compose down
