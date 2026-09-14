#!/usr/bin/env bash
#
# One-command setup for a new machine: host CLI, image, service, skill, smoke.
#
set -euo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

"${HERE}/install-host.sh"
"${HERE}/build.sh"
"${HERE}/up.sh"
"${HERE}/install-systemd.sh"
"${HERE}/install-skill.sh"
"${HERE}/smoke.sh"

printf '\nBootstrap complete. Lumen: http://127.0.0.1:8899/\n'
printf 'Agents: bin/pw.sh -s=<name> goto https://example.com\n'
