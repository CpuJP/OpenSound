#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Remove everything install.sh put in place for the current user.
set -e
UUID=opensound@cpujp.github.io

gnome-extensions disable "$UUID" 2>/dev/null || true
systemctl --user disable --now opensound.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/opensound.service"
systemctl --user daemon-reload
rm -rf "$HOME/.local/share/opensound" "$HOME/.local/share/gnome-shell/extensions/$UUID"
rm -f "$HOME/.local/bin/opensound" "$HOME/.local/share/applications/io.github.cpujp.OpenSound.desktop"
echo "OpenSound uninstalled."
