#!/usr/bin/env bash
#
# Drive an agent's browser through Lumen:
#
#   bin/pw.sh -s=alice goto https://example.com
#
# It asks Lumen to ensure the session, attaches the host CLI to that browser's
# CDP endpoint if needed, then runs the given playwright-cli command. 'open' is
# normalized to 'goto' because the browser is already open.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

require_cli

ARGS=("$@")
cmd_idx=-1
name=""
for i in "${!ARGS[@]}"; do
  case "${ARGS[$i]}" in
    -s=*|--session=*) name="${ARGS[$i]#*=}" ;;
  esac
done
for i in "${!ARGS[@]}"; do
  case "${ARGS[$i]}" in
    -s=*|--session=*|--*=*|--*) ;;
    -*) ;;
    *) cmd_idx="$i"; break ;;
  esac
done
cmd=""
[ "${cmd_idx}" -ge 0 ] && cmd="${ARGS[$cmd_idx]}"
name="${name:-${AGENT_NAME:-default}}"

case "${cmd}" in
  ""|list|show|close|close-all|kill-all|detach|install|install-browser|delete-data|help|--help|--version)
    pw "$@"
    ;;
  *)
    owner_args=()
    [ -n "${AGENT_NAME:-}" ] && owner_args=(--owner "${AGENT_NAME}")
    endpoint="$(lumen ensure "${name}" "${owner_args[@]}")" || die "could not ensure session '${name}' (bin/status.sh)"
    if ! pw list 2>/dev/null | grep -qE "^- ${name}:"; then
      pw attach --cdp="${endpoint}" --session="${name}" >/dev/null
    fi
    if [ "${cmd}" = "open" ]; then
      ARGS[cmd_idx]="goto"
    fi
    pw "${ARGS[@]}"
    ;;
esac
