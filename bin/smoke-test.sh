#!/usr/bin/env bash
#
# Compatibility entry point for the retired architecture-B smoke test.
#
set -euo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec "${HERE}/smoke.sh" "$@"
