#!/usr/bin/env bash
#
# Run the container runtime with Lumen's resolution (runtime auto-detect plus
# docker-via-sudo when the user cannot reach the daemon). Lets make targets
# share the exact same logic as the scripts without duplicating it.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

ctr "$@"
