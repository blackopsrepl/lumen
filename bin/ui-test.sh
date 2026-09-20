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
# The ratatui E2E spec runs a real app binary; build it so the suite can point
# the service at one instead of mocking the protocol.
cargo build --quiet --locked -p lumen-ratatui --example trex
trex="$(cargo metadata --no-deps --format-version 1 \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const m=JSON.parse(s);console.log(m.target_directory+"/debug/examples/trex")})')"

# The Qt E2E drives a real Qt application. Build the fixture when a Qt6
# development toolchain is present; otherwise the spec skips itself.
qt_app="${LUMEN_QT_APP:-}"
if [ -z "${qt_app}" ] && command -v g++ >/dev/null 2>&1 \
    && pkg-config --exists Qt6Quick Qt6Qml 2>/dev/null; then
  qt_bin="${run_dir}/qt-fixture"
  qt_flags="$(pkg-config --cflags --libs Qt6Quick Qt6Qml)"
  # Word splitting is intentional: pkg-config returns a list of flags.
  # shellcheck disable=SC2086
  if g++ -fPIC "${PROJECT_DIR}/tests/fixtures/qt_app.cpp" -o "${qt_bin}" ${qt_flags} 2>/dev/null; then
    qt_app="${qt_bin} ${PROJECT_DIR}/tests/fixtures/qt_app.qml"
    log "Qt E2E fixture built"
  else
    log "Qt E2E fixture did not build; the Qt spec will skip"
  fi
fi

log "viewer E2E on a disposable service, port ${port}"
LUMEN_PORT="${port}" \
  LUMEN_TEST_CONFIG="${config}" \
  LUMEN_PROFILES_DIR="${run_dir}/agents/run" \
  LUMEN_CHROME="${chromium}" \
  LUMEN_TREX="${trex}" \
  LUMEN_QT_APP="${qt_app}" \
  npm run test:e2e --silent
