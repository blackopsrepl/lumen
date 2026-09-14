#!/usr/bin/env bash
#
# Provision and attach an agent's isolated browser. Idempotent; self-heals if
# the broker restarted. In practice bin/pw.sh does this automatically, so this
# is an explicit convenience.
#
#   bin/session.sh <agent>
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

require_cli

name="${1:-${AGENT_NAME:-default}}"
[ -n "${name}" ] || die "agent name required"

ensure_attached "${name}" \
  || die "could not provision/attach '${name}' (is the broker up? bin/status.sh)"

log "agent '${name}' browser ready and attached"
log "drive it with: bin/pw.sh -s=${name} goto https://example.com"
