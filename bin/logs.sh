#!/usr/bin/env bash
#
# Follow browser broker logs.
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

compose logs -f
