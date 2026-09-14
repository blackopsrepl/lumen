#!/usr/bin/env bash
#
# Install and enable the rootless systemd user services with this checkout's
# real paths baked in. Safe to re-run (idempotent).
#
#   bin/install-systemd.sh
#
set -euo pipefail
# shellcheck source=common.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

command -v systemctl >/dev/null 2>&1 || die "systemctl not found"
podman_compose="$(command -v podman-compose || true)"
[ -n "${podman_compose}" ] || die "podman-compose not found on PATH"

unit_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
mkdir -p "${unit_dir}"

units=(playwright-browser playwright-dashboard playwright-feedback)
for unit in "${units[@]}"; do
  src="${PROJECT_DIR}/systemd/${unit}.service"
  [ -f "${src}" ] || die "missing ${src}"
  sed \
    -e "s|__PROJECT_DIR__|${PROJECT_DIR}|g" \
    -e "s|__HOME__|${HOME}|g" \
    -e "s|__PODMAN_COMPOSE__|${podman_compose}|g" \
    "${src}" > "${unit_dir}/${unit}.service"
  log "installed ${unit_dir}/${unit}.service"
done

systemctl --user daemon-reload
for unit in "${units[@]}"; do
  systemctl --user enable --now "${unit}.service"
done

# Keep user services running without an active login (best effort).
loginctl enable-linger "${USER}" >/dev/null 2>&1 || true

log "enabled: ${units[*]} (linger on for ${USER})"
log "status: systemctl --user status playwright-dashboard.service"
