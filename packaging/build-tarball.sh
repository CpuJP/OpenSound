#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds a generic Linux release tarball (prebuilt binaries + install.sh) into dist/.
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
NAME="opensound-$VERSION-$(uname -m)-linux"
cargo build --release --locked
rm -rf "dist/$NAME" && mkdir -p "dist/$NAME/bin"
cp target/release/opensound target/release/opensound-daemon "dist/$NAME/bin/"
cp -r data gnome-extension install.sh uninstall.sh LICENSE README.md README.es.md DISCLAIMER.md CHANGELOG.md "dist/$NAME/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "dist/$NAME.tar.gz"
