#!/usr/bin/env bash
#
# End-to-end smoke test for Lumen.
#
# Proves the browser path: the service is healthy and serving its viewer; two
# agents get isolated browsers; navigation, screenshots, and feedback work.
# Quickshell runtime coverage belongs to the disposable Playwright E2E suite,
# which runs where Sway and Quickshell are installed.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

base="http://127.0.0.1:${LUMEN_PORT}"
name="smoke-$$"
name2="smoke2-$$"
shot="$(mktemp -t lumen-smoke-XXXXXX.png)"

pass() { printf '  ok: %s\n' "$*"; }
fail() { printf '  FAIL: %s\n' "$*" >&2; exit 1; }
cleanup() {
  for n in "${name}" "${name2}"; do
    lumen stop "${n}" >/dev/null 2>&1 || true
  done
  rm -f "${shot}"
}
trap cleanup EXIT

curl -fsS --max-time 5 "${base}/healthz" | grep -q '"ok"' || fail "healthcheck"
pass "service healthy"

curl -fsS --max-time 5 "${base}/" | grep -qi lumen || fail "viewer not served"
pass "viewer served on ${LUMEN_PORT}"

ep="$(lumen ensure "${name}")" || fail "could not ensure ${name}"
ep2="$(lumen ensure "${name2}")" || fail "could not ensure ${name2}"
[ -n "${ep}" ] || fail "empty CDP endpoint"
[ "${ep}" != "${ep2}" ] || fail "agents share a browser (${ep})"
pass "isolated browsers: ${name}=${ep}  ${name2}=${ep2}"

curl -fsS --max-time 20 -X POST "${base}/v1/sessions/${name}/navigate" \
  -H 'content-type: application/json' \
  -d '{"url":"data:text/html,<h1>lumen-smoke</h1>"}' -o /dev/null || fail "navigate"
pass "drove a real page"

curl -fsS --max-time 20 -X POST "${base}/v1/sessions/${name}/screenshot?full=true" \
  -o "${shot}" || fail "screenshot"
[ "$(stat -c%s "${shot}")" -gt 1000 ] || fail "screenshot too small"
pass "captured a screenshot ($(stat -c%s "${shot}") bytes)"

curl -fsS --max-time 5 -X POST "${base}/v1/sessions/${name}/feedback" \
  -H 'content-type: application/json' -d '{"comment":"smoke"}' -o /dev/null || fail "add feedback"
curl -fsS --max-time 5 "${base}/v1/sessions/${name}/feedback?pending=true" \
  | grep -q smoke || fail "feedback not listed"
pass "feedback round-trip"

curl -fsS --max-time 5 -X POST "${base}/v1/sessions/${name}/feedback/ack-all" -o /dev/null || fail "ack"
[ "$(curl -fsS --max-time 5 "${base}/v1/sessions/${name}/feedback?pending=true")" = "[]" ] \
  || fail "feedback not consumed"
pass "feedback consumed"

printf 'SMOKE TEST PASSED\n'
