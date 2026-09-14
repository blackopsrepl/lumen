#!/usr/bin/env bash
#
# Check for human feedback (dashboard annotations) addressed to a session.
# Non-blocking: prints nothing pending and exits 0 if the human hasn't
# intervened. Safe to call between agent steps.
#
#   bin/feedback.sh <session>            # list pending feedback + notes
#   bin/feedback.sh <session> --consume  # also archive what it printed
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

name=""
consume=0
for arg in "$@"; do
  case "${arg}" in
    --consume) consume=1 ;;
    -s=*|--session=*) name="${arg#*=}" ;;
    -*) ;;
    *) [ -z "${name}" ] && name="${arg}" ;;
  esac
done
[ -n "${name}" ] || name="${PLAYWRIGHT_CLI_SESSION:-${AGENT_NAME:-default}}"

fb_root="${PW_FEEDBACK_DIR:-${PW_WORKSPACE}/feedback}"
dir="${fb_root}/${name}"

shopt -s nullglob
pending=()
for item in "${dir}"/*; do
  [ -f "${item}" ] || continue
  pending+=("${item}")
done
shopt -u nullglob

if [ "${#pending[@]}" -eq 0 ]; then
  printf 'no pending feedback for %s\n' "${name}"
  exit 0
fi

printf 'pending human feedback for %s:\n' "${name}"
for item in "${pending[@]}"; do
  printf '\n== %s ==\n' "${item}"
  case "${item}" in
    *.zip) unzip -p "${item}" feedback.md 2>/dev/null || true
           unzip -l "${item}" 2>/dev/null | sed -n '4,$p' | head -n -2 ;;
    *.png|*.yaml|*.md) printf '(see %s)\n' "${item}" ;;
    *) printf '(see %s)\n' "${item}" ;;
  esac
done

if [ "${consume}" = "1" ]; then
  archive="${fb_root}/.archive/${name}"
  mkdir -p "${archive}"
  mv "${pending[@]}" "${archive}/" 2>/dev/null || true
  printf '\n(archived under %s)\n' "${archive}"
fi
