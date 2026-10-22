# SPDX-License-Identifier: GPL-3.0-or-later
# Binaries are built with cargo and the crates.io registry (network needed at build time).
%global debug_package %{nil}
%global uuid opensound@cpujp.github.io

Name:           opensound
Version:        0.3.1
Release:        1%{?dist}
Summary:        Non-profit Linux controller for WH-1000XM5 headphones

License:        GPL-3.0-or-later
URL:            https://github.com/CpuJP/OpenSound
Source0:        %{url}/archive/v%{version}/OpenSound-%{version}.tar.gz

BuildRequires:  cargo >= 1.92
BuildRequires:  rust >= 1.92
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel >= 1.7
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

Requires:       bluez
Requires:       libadwaita >= 1.7
Recommends:     gnome-shell >= 48

%description
OpenSound is a non-profit, community-made application to control and configure
WH-1000XM5 headphones on Linux: noise cancelling and ambient sound, equalizer,
multipoint, connection quality, controls, automatic power off and voice guidance.
It includes a GTK 4 / libadwaita app, a D-Bus service and a GNOME Shell extension
with a top-bar battery indicator.

OpenSound is not affiliated with or endorsed by Sony.

%prep
%autosetup -n OpenSound-%{version}

%build
cargo build --release --locked

%install
install -Dm755 target/release/opensound "%{buildroot}%{_bindir}/opensound"
install -Dm755 target/release/opensound-daemon "%{buildroot}%{_bindir}/opensound-daemon"
install -Dm644 data/opensound.service "%{buildroot}%{_userunitdir}/opensound.service"
install -Dm644 data/io.github.cpujp.OpenSound.service "%{buildroot}%{_datadir}/dbus-1/services/io.github.cpujp.OpenSound.service"
install -Dm644 data/io.github.cpujp.OpenSound.App.desktop "%{buildroot}%{_datadir}/applications/io.github.cpujp.OpenSound.App.desktop"
install -Dm644 data/io.github.cpujp.OpenSound.App.metainfo.xml "%{buildroot}%{_metainfodir}/io.github.cpujp.OpenSound.App.metainfo.xml"
install -d "%{buildroot}%{_datadir}/gnome-shell/extensions/%{uuid}"
install -m644 gnome-extension/%{uuid}/* "%{buildroot}%{_datadir}/gnome-shell/extensions/%{uuid}/"

%check
desktop-file-validate "%{buildroot}%{_datadir}/applications/io.github.cpujp.OpenSound.App.desktop"
appstreamcli validate --no-net "%{buildroot}%{_metainfodir}/io.github.cpujp.OpenSound.App.metainfo.xml"
cargo test --release --locked -p opensound-core -p opensound-daemon

%post
%systemd_user_post opensound.service

%preun
%systemd_user_preun opensound.service

%files
%license LICENSE
%doc README.md README.es.md DISCLAIMER.md CHANGELOG.md docs/PROTOCOL.md
%{_bindir}/opensound
%{_bindir}/opensound-daemon
%{_userunitdir}/opensound.service
%{_datadir}/dbus-1/services/io.github.cpujp.OpenSound.service
%{_datadir}/applications/io.github.cpujp.OpenSound.App.desktop
%{_metainfodir}/io.github.cpujp.OpenSound.App.metainfo.xml
%{_datadir}/gnome-shell/extensions/%{uuid}/

%changelog
* Thu Oct 22 2026 Juan Pablo Giraldo Collazos - 0.3.1-1
- GNOME Shell extension: top-bar control card

* Thu Oct 22 2026 Juan Pablo Giraldo Collazos - 0.2.0-1
- Rewrite in Rust, D-Bus activation, confirmation dialogs, Fedora package
