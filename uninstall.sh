#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Remove everything install.sh put in place for the current user.
UUID=opensound@cpujp.github.io
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"

gnome-extensions disable "$UUID" 2>/dev/null
systemctl --user disable --now opensound.service 2>/dev/null
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/opensound.service"
systemctl --user daemon-reload
# make the session bus pick up the D-Bus activation file
gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.ReloadConfig >/dev/null 2>&1 || true
rm -rf "$DATA/opensound" "$DATA/gnome-shell/extensions/$UUID"
rm -f "$HOME/.local/bin/opensound" "$HOME/.local/bin/opensound-daemon" \
      "$DATA/applications/io.github.cpujp.OpenSound.App.desktop" \
      "$DATA/applications/io.github.cpujp.OpenSound.desktop" \
      "$DATA/metainfo/io.github.cpujp.OpenSound.App.metainfo.xml" \
      "$DATA/dbus-1/services/io.github.cpujp.OpenSound.service"
echo "OpenSound uninstalled."
