#!/usr/bin/env bash
#
# Show broker status and the per-agent browsers currently registered.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

if container_exists "${CONTAINER_NAME}"; then
  podman ps -a --filter "name=^${CONTAINER_NAME}$" \
    --format 'container: {{.Names}}  state: {{.Status}}'
else
  log "container ${CONTAINER_NAME} is not present"
fi

HEALTH="$(curl -fsS --max-time 5 "$(broker_url)/healthz" 2>/dev/null || true)"
if [ -z "${HEALTH}" ]; then
  printf 'broker: %s  UNREACHABLE\n' "$(broker_url)"
  exit 1
fi
printf 'broker: %s  OK  %s\n' "$(broker_url)" "${HEALTH}"

printf 'agents:\n'
curl -fsS --max-time 5 "$(broker_url)/sessions" \
  | python3 -c 'import sys, json
rows = json.load(sys.stdin)
if not rows:
    print("  (none)")
else:
    for r in rows:
        print("  - {:<20} {}".format(r["name"], r["cdpUrl"]))'
