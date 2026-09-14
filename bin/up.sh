#!/usr/bin/env bash
#
# Start the browser broker (detached). Browsers are launched per agent on
# demand; nothing is published except the broker on 127.0.0.1 (host network).
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "starting browser broker (compose)"
compose up -d --build

log "broker: $(broker_url)/healthz"
log "next: bin/session.sh <agent>   then   bin/dashboard.sh"
