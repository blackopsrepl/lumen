#!/usr/bin/env bash
#
# Follow Lumen's container logs.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

podman logs -f "${CONTAINER}"
