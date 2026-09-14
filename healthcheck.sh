#!/usr/bin/env bash
#
# Container healthcheck: probe the broker's /healthz endpoint. Exit 0 if it
# answers 200, non-zero otherwise. Node is always present in the image.
#
set -euo pipefail

# shellcheck disable=SC2016
node -e '
const port = process.env.BROKER_PORT || "8090";
fetch(`http://127.0.0.1:${port}/healthz`)
  .then((r) => process.exit(r.ok ? 0 : 1))
  .catch(() => process.exit(1));
'
