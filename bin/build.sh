#!/usr/bin/env bash
#
# Build the playwright-browser image.
#
#   bin/build.sh [--no-cache] [--pull]
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

NO_CACHE=()
PULL="newer"
for arg in "$@"; do
  case "${arg}" in
    --no-cache) NO_CACHE=(--no-cache) ;;
    --pull)     PULL="always" ;;
    -h|--help)  grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: ${arg}" ;;
  esac
done

log "building ${IMAGE} (playwright ${PLAYWRIGHT_IMAGE_VERSION})"
podman build \
  --pull="${PULL}" \
  "${NO_CACHE[@]}" \
  --build-arg "PLAYWRIGHT_IMAGE_VERSION=${PLAYWRIGHT_IMAGE_VERSION}" \
  -t "${IMAGE}" \
  -f "${PROJECT_DIR}/Containerfile" \
  "${PROJECT_DIR}"

log "built ${IMAGE}"
podman image inspect "${IMAGE}" --format '  size: {{.Size}} bytes  created: {{.Created}}'
