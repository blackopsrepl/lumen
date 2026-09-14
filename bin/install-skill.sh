#!/usr/bin/env bash
#
# Install the playwright-browser skill for opencode, with this checkout's path
# baked in. Safe to re-run.
#
#   bin/install-skill.sh
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

skill_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/opencode/skill/playwright-browser"
mkdir -p "${skill_dir}"
sed \
  -e "s|__PW_BIN__|${PROJECT_DIR}/bin|g" \
  "${PROJECT_DIR}/skills/playwright-browser/SKILL.md" \
  > "${skill_dir}/SKILL.md"
log "installed opencode skill -> ${skill_dir}/SKILL.md"
