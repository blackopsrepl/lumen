#!/usr/bin/env bash
#
# Start the Lumen service, converging a running container onto this checkout.
#
# podman-compose leaves an existing container in place when only the image
# changed, which silently keeps an old binary serving. The image is stamped
# with the revision it was built from, so compare that: recreate only when the
# running container predates this checkout, and make `git pull && bin/up.sh` a
# correct one-step upgrade.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

log "building ${IMAGE} (revision ${LUMEN_REVISION})"
compose build

running_revision="$(podman inspect "${CONTAINER}" \
  --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' 2>/dev/null || true)"

if [ -n "${running_revision}" ] && [ "${running_revision}" != "${LUMEN_REVISION}" ]; then
  log "container is from ${running_revision}; recreating for ${LUMEN_REVISION}"
  compose up -d --force-recreate
else
  log "starting ${CONTAINER}"
  compose up -d
fi

log "lumen: http://127.0.0.1:${LUMEN_PORT}/"
