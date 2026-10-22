#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Per-user install (no root). Uses prebuilt binaries from ./bin when present
# (release tarball), otherwise builds from source with cargo.
set -e
cd "$(dirname "$0")"
UUID=opensound@cpujp.github.io
BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
UNITS="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

if [ -x bin/opensound-daemon ] && [ -x bin/opensound ]; then
    SRC=bin
else
    command -v cargo >/dev/null || { echo "cargo not found: install Rust or use a release tarball" >&2; exit 1; }
    cargo build --release --locked
    SRC=target/release
fi

# Remove the legacy Python version (0.1.x).
rm -rf "$DATA/opensound" "$DATA/applications/io.github.cpujp.OpenSound.desktop"

mkdir -p "$BIN" "$UNITS" "$DATA/applications" "$DATA/metainfo" "$DATA/dbus-1/services" "$DATA/gnome-shell/extensions/$UUID"
install -m755 "$SRC/opensound" "$SRC/opensound-daemon" "$BIN/"
sed "s#/usr/bin/#$BIN/#" data/opensound.service > "$UNITS/opensound.service"
sed "s#/usr/bin/#$BIN/#" data/io.github.cpujp.OpenSound.service > "$DATA/dbus-1/services/io.github.cpujp.OpenSound.service"
install -m644 data/io.github.cpujp.OpenSound.App.desktop "$DATA/applications/"
install -m644 data/io.github.cpujp.OpenSound.App.metainfo.xml "$DATA/metainfo/"
install -m644 gnome-extension/$UUID/* "$DATA/gnome-shell/extensions/$UUID/"

systemctl --user daemon-reload
# make the session bus pick up the D-Bus activation file
gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.ReloadConfig >/dev/null 2>&1 || true
systemctl --user enable opensound.service
systemctl --user restart opensound.service
gnome-extensions enable "$UUID" 2>/dev/null || \
    echo "Log out and back in, then run: gnome-extensions enable $UUID"
echo "OpenSound installed. Open 'OpenSound' from the application menu."
