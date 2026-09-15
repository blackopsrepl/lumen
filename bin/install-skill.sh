#!/usr/bin/env bash
#
# Install the lumen skill for opencode, with this checkout's path baked in.
# Safe to re-run.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

skill_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/opencode/skill/lumen"
mkdir -p "${skill_dir}"
sed \
  -e "s|__PW_BIN__|$(sed_escape "${PROJECT_DIR}/bin")|g" \
  "${PROJECT_DIR}/skills/lumen/SKILL.md" \
  > "${skill_dir}/SKILL.md"
log "installed opencode skill -> ${skill_dir}/SKILL.md"
