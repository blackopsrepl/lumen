#!/usr/bin/env bash
#
# Build the Lumen image.
#
set -euo pipefail
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
# `docker build` only knows `--pull` (always pull) while podman also accepts
# `--pull=newer`; both map to a refresh here, so either runtime re-pulls base
# layers by default and `--pull` forces it explicitly.
PULL_FLAG=()
case "${CTR}" in
  docker) PULL_FLAG=(--pull) ;;
  *) PULL_FLAG=(--pull="${PULL}") ;;
esac
ctr build \
  "${PULL_FLAG[@]}" \
  "${NO_CACHE[@]}" \
  --build-arg "PLAYWRIGHT_IMAGE_VERSION=${PLAYWRIGHT_IMAGE_VERSION}" \
  --build-arg "LUMEN_REVISION=${LUMEN_REVISION}" \
  -t "${IMAGE}" \
  -f "${PROJECT_DIR}/Containerfile" \
  "${PROJECT_DIR}"

log "built ${IMAGE}"
ctr image inspect "${IMAGE}" --format '  size: {{.Size}} bytes  created: {{.Created}}'
