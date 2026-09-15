#!/usr/bin/env bash
#
# Install and enable the Lumen user service with this checkout's real paths.
# Safe to re-run. Also retires any leftover playwright-*.user units.
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

command -v systemctl >/dev/null 2>&1 || die "systemctl not found"
podman_compose="$(command -v podman-compose || true)"
[ -n "${podman_compose}" ] || die "podman-compose not found on PATH"

unit_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
mkdir -p "${unit_dir}"

sed \
  -e "s|__PROJECT_DIR__|$(sed_escape "${PROJECT_DIR}")|g" \
  -e "s|__HOME__|$(sed_escape "${HOME}")|g" \
  -e "s|__PODMAN_COMPOSE__|$(sed_escape "${podman_compose}")|g" \
  "${PROJECT_DIR}/systemd/lumen.service" > "${unit_dir}/lumen.service"
log "installed ${unit_dir}/lumen.service"

for stale in playwright-browser playwright-dashboard playwright-feedback; do
  if [ -f "${unit_dir}/${stale}.service" ]; then
    systemctl --user disable --now "${stale}.service" >/dev/null 2>&1 || true
    rm -f "${unit_dir}/${stale}.service"
    log "retired ${stale}.service"
  fi
done

systemctl --user daemon-reload
systemctl --user enable --now lumen.service
if loginctl enable-linger "${USER}" >/dev/null 2>&1; then
  log "enabled: lumen.service (linger on for ${USER})"
else
  log "enabled: lumen.service (warning: linger not enabled; the service stops at logout)"
fi
