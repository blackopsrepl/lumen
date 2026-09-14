#!/usr/bin/env bash
#
# Open a shell inside the running broker container.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

container_exists "${CONTAINER_NAME}" || die "container ${CONTAINER_NAME} is not running"
exec podman exec -it "${CONTAINER_NAME}" bash
