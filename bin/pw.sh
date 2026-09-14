#!/usr/bin/env bash
#
# Run playwright-cli from the shared workspace, self-healing the agent session.
#
#   bin/pw.sh -s=<agent> goto https://example.com
#   bin/pw.sh -s=<agent> open https://example.com   # 'open' is normalized to 'goto'
#
# Every command transparently ensures the agent's browser exists and the CLI is
# attached; if the broker restarted, the stale session is replaced automatically.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

require_cli

# Locate the command token: the first argument that is not an option. This CLI
# takes options as `-s=name`, `--foo=bar`, or bare flags.
ARGS=("$@")
cmd_idx=-1
for i in "${!ARGS[@]}"; do
  case "${ARGS[$i]}" in
    -s=*|--session=*|--*=*|--*) ;;
    -*) ;;
    *) cmd_idx="$i"; break ;;
  esac
done
cmd=""
[ "${cmd_idx}" -ge 0 ] && cmd="${ARGS[$cmd_idx]}"

case "${cmd}" in
  # Session/state management: never auto-provision a browser.
  ""|list|show|close|close-all|kill-all|detach|install|install-browser|delete-data|help|--help|--version)
    pw "$@"
    ;;
  *)
    session="$(parse_session "$@")"
    ensure_attached "${session}" \
      || die "could not ensure browser session '${session}' (broker up? bin/status.sh)"
    # The attached browser is already open; navigate with `goto`, not `open`
    # (open launches a new local browser and fails).
    if [ "${cmd}" = "open" ]; then
      ARGS[cmd_idx]="goto"
    fi
    pw "${ARGS[@]}"
    ;;
esac
