#!/bin/sh
# Start a session bus and an unlocked Secret Service keyring, then run Lumen.
#
# Sessions inherit this environment, so a program inside a terminal or desktop
# session can use the keyring through `secret-tool` without a prompt, and
# Chromium finds the same service for saved passwords.
set -e

runtime_dir="${XDG_RUNTIME_DIR:-/run/lumen}"
mkdir -p "$runtime_dir"
chmod 700 "$runtime_dir"

# One session bus for the whole container, shared by every session.
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
  DBUS_SESSION_BUS_ADDRESS="$(dbus-daemon --session --fork --print-address)"
  export DBUS_SESSION_BUS_ADDRESS
fi

# Keep keyring material on the data volume, so credentials an agent stores
# inside a session survive a container recreate.
if [ -d /data ] && [ ! -e "${HOME}/.local/share/keyrings" ]; then
  mkdir -p /data/keyrings "${HOME}/.local/share"
  chmod 700 /data/keyrings
  ln -s /data/keyrings "${HOME}/.local/share/keyrings"
fi

# Unlock a login keyring with a throwaway password: the container is disposable
# and no one is present to answer a prompt. A failure here costs the keyring,
# not the service, so it is reported and not fatal.
if [ -z "${GNOME_KEYRING_CONTROL:-}" ]; then
  if ! printf '%s' "${LUMEN_KEYRING_PASSWORD:-lumen}" \
    | gnome-keyring-daemon --unlock --components=secrets >/dev/null; then
    echo "lumen: keyring daemon did not start; secret-tool will be unavailable" >&2
  fi
fi

exec lumen "$@"
