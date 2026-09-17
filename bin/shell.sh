#!/usr/bin/env bash
#
# Open a shell inside the Lumen container.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

ctr exec -it "${CONTAINER}" bash
