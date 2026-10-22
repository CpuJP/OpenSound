#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Install OpenSound for the current user (no root needed).
set -e
cd "$(dirname "$0")"
LIB="$HOME/.local/share/opensound"
EXT="$HOME/.local/share/gnome-shell/extensions/opensound@cpujp.github.io"

mkdir -p "$LIB" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.config/systemd/user" "$EXT"
rm -rf "$LIB/opensound" && cp -r opensound "$LIB/"
cat > "$HOME/.local/bin/opensound" <<EOS
#!/bin/sh
PYTHONPATH="$LIB" exec /usr/bin/python3 -m opensound "\$@"
EOS
chmod +x "$HOME/.local/bin/opensound"
cp data/io.github.cpujp.OpenSound.desktop "$HOME/.local/share/applications/"
cp data/opensound.service "$HOME/.config/systemd/user/"
cp gnome-extension/opensound@cpujp.github.io/* "$EXT/"

systemctl --user daemon-reload
systemctl --user enable --now opensound.service
systemctl --user restart opensound.service
gnome-extensions enable opensound@cpujp.github.io 2>/dev/null || \
  echo "Log out and back in, then run: gnome-extensions enable opensound@cpujp.github.io"
echo "OpenSound installed. Open 'OpenSound' from the application menu."
