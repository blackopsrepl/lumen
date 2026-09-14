#!/usr/bin/env bash
#
# Route human annotations from the Playwright dashboard into per-session inboxes.
#
# The dashboard annotate/export is client-side: the browser downloads an
# `annotations-*.zip` (containing annotations-*.png, .yaml and feedback.md) to
# the human's download directory. This watcher moves each new export into
#
#   ${PW_WORKSPACE}/feedback/<session>/
#
# where <session> is parsed from the export's feedback.md header
# ("## screenshot N: <session> / <title> @ <url>"). Agents read their inbox with
# bin/feedback.sh — non-blocking, at whatever point they choose.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

DOWNLOADS_DIR="${DOWNLOADS_DIR:-${HOME}/Downloads}"
FEEDBACK_DIR="${PW_FEEDBACK_DIR:-${PW_WORKSPACE}/feedback}"
POLL_SECONDS="${FEEDBACK_POLL_SECONDS:-3}"

mkdir -p "${DOWNLOADS_DIR}" "${FEEDBACK_DIR}"
log "watching ${DOWNLOADS_DIR}/annotations-*.zip -> ${FEEDBACK_DIR}/<session>/"

session_from_zip() {
  local zip="$1" header session
  header="$(unzip -p "${zip}" feedback.md 2>/dev/null | head -1 || true)"
  session="$(printf '%s' "${header}" | sed -n 's/^##.*: \([^/]*\) \/.*/\1/p' | xargs || true)"
  printf '%s' "${session}"
}

while true; do
  shopt -s nullglob
  for archive in "${DOWNLOADS_DIR}"/annotations-*.zip; do
    [ -f "${archive}" ] || continue
    session="$(session_from_zip "${archive}")"
    [ -n "${session}" ] || session="unknown"
    # Reject anything that would escape the feedback dir.
    session="$(printf '%s' "${session}" | tr -c 'A-Za-z0-9._-' '_')"
    dest="${FEEDBACK_DIR}/${session}"
    mkdir -p "${dest}"
    mv "${archive}" "${dest}/$(date +%s%N)-$(basename "${archive}")"
    log "routed $(basename "${archive}") -> ${dest}/"
  done
  shopt -u nullglob
  sleep "${POLL_SECONDS}"
done
