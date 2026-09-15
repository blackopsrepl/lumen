#!/usr/bin/env bash
#
# Drive an agent's browser through Lumen:
#
#   bin/pw.sh -s=alice goto https://example.com
#
# It asks Lumen to ensure the session, binds the host CLI to the endpoint the
# service reports, then runs the given playwright-cli command. 'open' is
# normalized to 'goto' because the browser is already open.
#
# The service is the single source of truth for routing: the recorded endpoint
# is only a cache, re-validated against Lumen on every call, so a browser the
# service recreated can never leave the CLI talking to a dead endpoint.
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
valid_session_name "${name}" \
  || die "invalid session name '${name}' (use [A-Za-z0-9._-], 1-32 chars)"

# Each session gets its own directory under the shared root:
#   <root>/sessions/<name>/            the CLI's working directory
#   <root>/sessions/<name>/.playwright-cli/  artifacts, owned by this session
#   <root>/sessions/<name>/endpoint    endpoint this session was bound to
# The `.playwright` marker at the root keeps every session in one CLI
# workspace; without it the CLI falls back to a global default shared with
# every unrelated project on the host.
PW_ROOT="${PW_WORKSPACE}"
session_dir="${PW_ROOT}/sessions/${name}"

pw_in() {
  local dir="$1"
  shift
  ( cd "${dir}" && exec "${PW_CLI}" "$@" )
}

case "${cmd}" in
  ""|list|show|install|install-browser|delete-data|help|--help|--version)
    pw "$@"
    ;;
  close|detach|close-all|kill-all)
    rc=0
    case "${cmd}" in
      close|detach)
        run_dir="${PW_ROOT}"
        [ -d "${session_dir}" ] && run_dir="${session_dir}"
        pw_in "${run_dir}" "$@" || rc=$?
        if [ "${cmd}" = "close" ]; then
          # The service owns the browser and its profile, so ending the session
          # is the service's call; the CLI close only unbinds this client. If
          # that call fails the browser may still be running, so keep the local
          # state and fail loudly instead of reporting a clean stop.
          lumen stop "${name}" >/dev/null \
            || die "could not stop session '${name}' (bin/status.sh)"
          rm -rf "${session_dir}"
          printf '[lumen] session %s stopped\n' "${name}"
          rc=0
        fi
        ;;
      close-all|kill-all)
        pw_in "${PW_ROOT}" "$@" || rc=$?
        # Same rule for the bulk verbs: the CLI unbinds, the service reclaims.
        lumen stop --all >/dev/null || die "could not stop every session (bin/status.sh)"
        rm -rf "${PW_ROOT}/sessions"
        rc=0
        ;;
    esac
    exit "${rc}"
    ;;
  *)
    owner_args=()
    [ -n "${AGENT_NAME:-}" ] && owner_args=(--owner "${AGENT_NAME}")
    endpoint="$(lumen ensure "${name}" "${owner_args[@]}")" || die "could not ensure session '${name}' (bin/status.sh)"

    mkdir -p "${session_dir}" "${PW_ROOT}/.playwright"
    if [ "$(cat "${session_dir}/endpoint" 2>/dev/null)" != "${endpoint}" ]; then
      pw_in "${session_dir}" attach --cdp="${endpoint}" --session="${name}" >/dev/null \
        || die "could not bind session '${name}' to ${endpoint}"
      printf '%s\n' "${endpoint}" > "${session_dir}/endpoint"
    fi

    if [ "${cmd}" = "open" ]; then
      ARGS[cmd_idx]="goto"
    fi
    marker="$(mktemp)"
    rc=0
    pw_in "${session_dir}" "${ARGS[@]}" || rc=$?
    # Artifacts are written with relative names inside the session directory,
    # which no agent can resolve from its own project. Name every file this
    # command produced by session and absolute path, so reading one is a plain
    # Read and never a filesystem search.
    artifact_dir="${session_dir}/.playwright-cli"
    if [ -d "${artifact_dir}" ]; then
      find "${artifact_dir}" -type f -newer "${marker}" 2>/dev/null |
        while IFS= read -r file; do printf '[lumen] artifact %s %s\n' "${name}" "${file}"; done
    fi
    rm -f "${marker}"
    exit "${rc}"
    ;;
esac
