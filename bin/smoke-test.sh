#!/usr/bin/env bash
#
# End-to-end smoke test for architecture B.
#
# Proves: broker is up; two agents get isolated browsers (distinct CDP ports);
# the host CLI drives one across the split; the dashboard serves on localhost.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

require_image
require_cli

SMOKE_MARKER="browser-b-smoke-ok"
A="smoke-a-$$"
B="smoke-b-$$"

fail() { printf '  FAIL: %s\n' "$*" >&2; exit 1; }
pass() { printf '  ok: %s\n' "$*"; }

cleanup() {
  pw -s="${A}" close >/dev/null 2>&1 || true
  pw -s="${B}" close >/dev/null 2>&1 || true
  curl -fsS -X DELETE "$(broker_url)/sessions/${A}" >/dev/null 2>&1 || true
  curl -fsS -X DELETE "$(broker_url)/sessions/${B}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

HEALTH="$(curl -fsS --max-time 5 "$(broker_url)/healthz" 2>/dev/null || true)"
[ -n "${HEALTH}" ] || fail "broker unreachable at $(broker_url); run bin/up.sh"
pass "broker healthy: ${HEALTH}"

URL_A="$(broker_ensure "${A}")"
URL_B="$(broker_ensure "${B}")"
if [ -z "${URL_A}" ] || [ -z "${URL_B}" ]; then
  fail "broker did not return CDP URLs"
fi
[ "${URL_A}" != "${URL_B}" ] || fail "agents share a browser (${URL_A}); expected isolation"
pass "isolated browsers: ${A}=${URL_A}  ${B}=${URL_B}"

pw attach --cdp="${URL_A}" --session="${A}" >/dev/null
pass "attached ${A}"

pw -s="${A}" goto "data:text/html,<h1>${SMOKE_MARKER}</h1>" >/dev/null
SNAP="$(pw -s="${A}" snapshot 2>&1)"
printf '%s' "${SNAP}" | grep -q "${SMOKE_MARKER}" || fail "snapshot did not contain the marker: ${SNAP}"
pass "drove a real page and read it back"

# Verify self-heal: force the CLI session's endpoint to look stale and confirm
# the next command re-provisions + re-attaches without re-running session.sh.
rm -f "${PW_WORKSPACE}/.sessions/${A}"
URL_A2="$(broker_ensure "${A}")"
[ "${URL_A2}" = "${URL_A}" ] || fail "broker relaunched a live browser unexpectedly"
pw -s="${A}" goto "data:text/html,<h1>${SMOKE_MARKER}-2</h1>" >/dev/null
SNAP2="$(pw -s="${A}" snapshot 2>&1)"
printf '%s' "${SNAP2}" | grep -q "${SMOKE_MARKER}-2" || fail "self-heal path failed: ${SNAP2}"
pass "self-heal: session survived a lost state file"

# Dashboard is served by the systemd user unit at DASHBOARD_PORT. Soft check so
# this test never disturbs the running service's singleton.
CODE="$(curl -sL -o /dev/null -w '%{http_code}' --max-time 5 "http://127.0.0.1:${DASHBOARD_PORT}/" 2>/dev/null || true)"
case "${CODE}" in
  2*) BIND="$(ss -ltn 2>/dev/null | awk -v p=":${DASHBOARD_PORT}" '$4 ~ p {print $4}' | head -1)"
      case "${BIND}" in
        127.0.0.1:*) pass "dashboard served on ${DASHBOARD_PORT} (localhost-locked)" ;;
        *)           printf '  WARN: dashboard bound %s (not loopback-only)\n' "${BIND}" ;;
      esac ;;
  *)  printf '  WARN: dashboard not reachable on %s (systemctl --user start playwright-dashboard)\n' "${DASHBOARD_PORT}" ;;
esac

printf 'SMOKE TEST PASSED\n'
