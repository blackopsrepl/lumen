#!/usr/bin/env bash
#
# One-command setup for a new machine: host CLI + browser, image, broker, and
# the systemd user services + opencode skill. Idempotent. Finish with the smoke
# test.
#
#   bin/bootstrap.sh
#
set -euo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

"${HERE}/install-host.sh"       # @playwright/cli + its browser
"${HERE}/build.sh"              # browser broker image
"${HERE}/up.sh"                 # start the broker
"${HERE}/install-systemd.sh"    # browser + dashboard + feedback services
"${HERE}/install-skill.sh"      # opencode skill
"${HERE}/smoke-test.sh"         # prove it end to end

printf '\nBootstrap complete. Dashboard: http://localhost:8899\n'
printf 'Agents: bin/pw.sh -s=<name> goto https://example.com\n'
