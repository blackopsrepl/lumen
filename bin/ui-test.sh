#!/usr/bin/env bash
#
# Run the browser E2E suite against a disposable Lumen service.
#
# Every run gets its own free port and its own data directory, and never reuses
# a server it did not start. That keeps the suite honest (it can only exercise
# this worktree) and keeps two runs from colliding: a service still draining
# from the previous run holds a different port and a different profile root.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

port="$("${PROJECT_DIR}/bin/free-port.sh")"
run_dir="$(mktemp -d /tmp/lumen-e2e-XXXXXX)"
trap 'rm -rf "${run_dir}"' EXIT

config="${run_dir}/lumen.toml"
sed \
  -e "s|/tmp/lumen-e2e-agents|${run_dir}/agents|" \
  -e "s|/tmp/lumen-e2e-feedback.db|${run_dir}/feedback.db|" \
  "${PROJECT_DIR}/tests/e2e/lumen.toml" > "${config}"

chromium="$(node -e 'console.log(require("playwright").chromium.executablePath())')"
log "viewer E2E on a disposable service, port ${port}"
LUMEN_PORT="${port}" \
  LUMEN_TEST_CONFIG="${config}" \
  LUMEN_PROFILES_DIR="${run_dir}/agents/run" \
  LUMEN_CHROME="${chromium}" \
  npm run test:e2e --silent
