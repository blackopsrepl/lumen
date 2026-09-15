#!/usr/bin/env bash
#
# Print a free loopback TCP port for the disposable E2E service.
#
# The suite must always test the server it started. Reusing whatever answers on
# a fixed port is how a run silently exercises a leftover — or the production
# service — instead of the worktree, so instead each run asks for a port
# nothing is listening on, starting from LUMEN_TEST_PORT.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

base="${LUMEN_TEST_PORT:-18899}"
for (( port = base; port < base + 50; port++ )); do
  if ! (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
    printf '%s\n' "${port}"
    exit 0
  fi
done
die "no free port between ${base} and $((base + 49))"
