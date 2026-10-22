#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds the Fedora source and binary RPMs from the current git HEAD into dist/.
# Extra rpmbuild flags can be passed in RPMBUILD_FLAGS (for example --nodeps with a rustup toolchain).
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
SPEC_VERSION=$(sed -n 's/^Version: *//p' packaging/opensound.spec)
[ "$VERSION" = "$SPEC_VERSION" ] || { echo "Cargo.toml ($VERSION) and opensound.spec ($SPEC_VERSION) disagree" >&2; exit 1; }
# rpmbuild helper scripts break on paths with spaces, so build in a temporary directory
TOP=$(mktemp -d)
trap 'rm -rf "$TOP"' EXIT
mkdir -p "$TOP/SOURCES" dist
git archive --format=tar.gz --prefix="OpenSound-$VERSION/" -o "$TOP/SOURCES/OpenSound-$VERSION.tar.gz" HEAD
# shellcheck disable=SC2086
rpmbuild -ba --define "_topdir $TOP" ${RPMBUILD_FLAGS:-} packaging/opensound.spec
cp "$TOP"/RPMS/*/*.rpm "$TOP"/SRPMS/*.rpm dist/
ls -1 dist/*.rpm
