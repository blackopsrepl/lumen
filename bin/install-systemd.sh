#!/usr/bin/env bash
#
# Install and enable the Lumen user service with this checkout's real paths.
# Safe to re-run. Also retires any leftover playwright-*.user units.
# Resolves the compose command from the active container runtime
# (LUMEN_RUNTIME/CONTAINER_RUNTIME, or podman-preferred auto-detect).
#
set -euo pipefail
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

command -v systemctl >/dev/null 2>&1 || die "systemctl not found"
ensure_runtime

# Resolve an absolute compose command for the systemd unit. ExecStart splits
# on spaces, so "…/podman compose" and "…/docker compose" work as binary + arg,
# matching how `compose()` in common.sh invokes the same runtime.
case "${CTR}" in
  podman)
    if command -v podman >/dev/null 2>&1 && podman compose version >/dev/null 2>&1; then
      compose_cmd="$(command -v podman) compose"
      unit_wants="Wants=podman-user-wait-network-online.service"
      unit_after="After=podman-user-wait-network-online.service"
    elif command -v podman-compose >/dev/null 2>&1; then
      compose_cmd="$(command -v podman-compose)"
      unit_wants="Wants=podman-user-wait-network-online.service"
      unit_after="After=podman-user-wait-network-online.service"
    else
      die "no podman compose found (need 'podman compose' plugin or podman-compose)"
    fi
    ;;
  docker)
    command -v docker >/dev/null 2>&1 || die "docker not found on PATH"
    ctr compose version >/dev/null 2>&1 || die "'docker compose' plugin not found"
    # A user unit cannot answer a sudo password prompt, so an elevated unit
    # requires passwordless sudo for docker; common.sh already resolved whether
    # elevation is needed into CTR_SUDO.
    if [ "${#CTR_SUDO[@]}" -gt 0 ]; then
      sudo -n docker info >/dev/null 2>&1 \
        || die "docker needs sudo here but passwordless sudo is unavailable; allow it (e.g. a sudoers rule) or join the docker group instead"
      compose_cmd="$(command -v sudo) $(command -v docker) compose"
    else
      compose_cmd="$(command -v docker) compose"
    fi
    unit_wants="Wants=docker.service"
    unit_after="After=docker.service"
    ;;
  *) die "unsupported container runtime: ${CTR}" ;;
esac

unit_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
mkdir -p "${unit_dir}"

sed \
  -e "s|__PROJECT_DIR__|$(sed_escape "${PROJECT_DIR}")|g" \
  -e "s|__HOME__|$(sed_escape "${HOME}")|g" \
  -e "s|__RUNTIME__|$(sed_escape "${CTR}")|g" \
  -e "s|__COMPOSE__|$(sed_escape "${compose_cmd}")|g" \
  -e "s|__UNIT_WANTS__|$(sed_escape "${unit_wants}")|g" \
  -e "s|__UNIT_AFTER__|$(sed_escape "${unit_after}")|g" \
  "${PROJECT_DIR}/systemd/lumen.service" > "${unit_dir}/lumen.service"
log "installed ${unit_dir}/lumen.service (${compose_cmd})"

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
