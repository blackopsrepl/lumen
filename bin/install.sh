#!/usr/bin/env bash
# Install the Lumen daemon as a systemd user service.
#
# The daemon owns the compositor and every session, and must outlive any viewer
# window, so it is a real user service rather than a child of the GUI. This
# installs the binaries and the unit, then enables it. Nothing here needs root:
# it all lives under the invoking user's home.
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
UNITDIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$SOURCE_DIR/build}"

say() { printf '%s\n' "$*"; }

if [[ ! -x "$BUILD_DIR/lumen-daemon" ]]; then
    say "lumen: no build at $BUILD_DIR — run 'cmake -B build && cmake --build build' first"
    exit 1
fi

install -d "$BINDIR" "$UNITDIR"

for binary in lumen-daemon lumen lumen-cli; do
    install -m 0755 "$BUILD_DIR/$binary" "$BINDIR/$binary"
    say "installed $BINDIR/$binary"
done

# A running instance holds the compositor socket and the profile tree; replacing
# the binary underneath it would leave the old process serving stale code.
if systemctl --user is-active --quiet lumen.service; then
    say "stopping the running daemon before replacing it"
    systemctl --user stop lumen.service
fi

# The daemon owns the compositor, and a session an agent is driving must not die
# because the human logged out. `enable-linger` is what makes the user manager
# outlive the last session; without it the service is stopped at logout no
# matter how it is enabled, so a fresh install would quietly not do what the
# README promises.
if [[ "$(loginctl show-user "$(id -un)" -p Linger --value 2>/dev/null)" != "yes" ]]; then
    if loginctl enable-linger "$(id -un)" 2>/dev/null; then
        say "enabled linger: the daemon now survives logout"
    else
        say "WARNING: could not enable linger — the daemon will stop at logout."
        say "         Run 'loginctl enable-linger $(id -un)' to fix it."
    fi
fi

install -m 0644 "$SOURCE_DIR/packaging/lumen.service" "$UNITDIR/lumen.service"
say "installed $UNITDIR/lumen.service"

systemctl --user daemon-reload
systemctl --user enable lumen.service
systemctl --user restart lumen.service

say "daemon status: $(systemctl --user is-active lumen.service)"
say "sockets: $(ls /run/user/"$(id -u)"/lumen-agent.sock 2>/dev/null || echo 'not yet listening')"
