# OpenSound

[![CI](https://github.com/CpuJP/OpenSound/actions/workflows/ci.yml/badge.svg)](https://github.com/CpuJP/OpenSound/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

**OpenSound** is a free and open-source Linux application to control and configure wireless headphones that speak the MDR v2 protocol, such as the WH-1000XM5. It provides a GTK 4 / libadwaita app and a GNOME Shell extension, with no account, no telemetry and no cloud.

> **OpenSound is a non-profit, non-commercial community project.** There is no official way to configure these headphones from Linux, so OpenSound exists only to extend their connectivity and let owners use the hardware they already have on their computer. It is not sold, has no paid features, and is **not affiliated with, endorsed, sponsored or approved by Sony Group Corporation** or any of its subsidiaries. See [Legal notice](#legal-notice).

[Leer en español](README.es.md)

## Features

| Area | Settings |
|---|---|
| Live status | Battery level and charging, active codec (SBC / AAC / LDAC…), model and firmware |
| Ambient sound control | Noise cancelling / Ambient sound / Off, ambient level (1–20), focus on voice |
| Sound | 5-band equalizer (400, 1k, 2.5k, 6.3k, 16k Hz) from −10 to +10, plus Clear Bass |
| Connection | Connect to 2 devices simultaneously (multipoint), Bluetooth connection quality |
| Controls | [NC/AMB] button behaviour, touch sensor control panel, pause when headphones are taken off |
| Power | Automatic power off |
| System | Firmware version, voice guidance on/off (language is shown) |

Changes made on the headphones themselves, for example with the buttons, appear in the UI immediately. When the headphones ask for a confirmation before a disruptive change, such as a reconnect after toggling multipoint, OpenSound shows the same confirmation dialog.

**Tested hardware:** WH-1000XM5, firmware 2.5.1. Other models that use the same protocol may work partially. Reports are welcome.

## Requirements

- Linux with BlueZ, a systemd user session, GTK 4 and libadwaita ≥ 1.7. Fedora 42+, Debian 13, Ubuntu 25.04+ and Arch Linux qualify.
- GNOME Shell 48–50 for the top-bar extension. The app works on any desktop.
- The headphones must be paired and connected through the normal Bluetooth settings.

## Installation

Download the files from the [latest release](https://github.com/CpuJP/OpenSound/releases/latest).

**Fedora (RPM):**

```sh
sudo dnf install ./opensound-*.x86_64.rpm
```

**Other distributions (prebuilt binaries, per user, no root):**

```sh
tar xzf opensound-*-linux.tar.gz
cd opensound-*-linux
sh install.sh
```

**From source** (needs Rust ≥ 1.92 and the GTK 4 / libadwaita development packages):

```sh
git clone https://github.com/CpuJP/OpenSound.git
cd OpenSound
sh install.sh        # builds with cargo and installs under ~/.local
```

Each method installs the same components:

- `opensound-daemon`: the service that owns the connection to the headphones. D-Bus starts it on demand, and the systemd user unit `opensound.service` keeps it running.
- The **OpenSound** app, in your application menu, or run `opensound`.
- The GNOME Shell extension `opensound@cpujp.github.io`. It adds a top-bar button with the battery level that opens a control card (battery, noise control, ambient level and focus on voice), plus a Quick Settings toggle.

On Wayland, log out and back in once. Then run `gnome-extensions enable opensound@cpujp.github.io`.

To uninstall, run `sudo dnf remove opensound` for the RPM, or `sh uninstall.sh` for the other methods.

## Architecture

```
Headphones ──RFCOMM (SPP)── opensound-daemon (Rust) ──session D-Bus──┬── opensound GTK app (Rust)
                                                                     └── GNOME Shell extension (GJS)
```

The headphones accept a single control connection. The daemon is the only process that opens it, and the UIs are D-Bus clients. GNOME Shell only loads extensions written in JavaScript (GJS), so the extension is the only part that is not Rust.

| Crate | Content |
|---|---|
| `crates/opensound-core` | Framing, protocol parser and builders, state model. No I/O |
| `crates/opensound-daemon` | Bluetooth sockets, SDP, ACK handling, D-Bus service |
| `crates/opensound-gtk` | GTK 4 / libadwaita app (`opensound` binary) |

D-Bus API on bus name `io.github.cpujp.OpenSound`, object `/io/github/cpujp/OpenSound`:

| Member | Description |
|---|---|
| `GetState() → s` | Current state as JSON |
| `Set(s key, s value_json)` | Change a setting, for example `Set("nc_mode", "\"ambient\"")` |
| signal `StateChanged(s json)` | Emitted on every change, including changes made on the headphones |

The protocol is documented in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Development

```sh
cargo test                        # all tests, no hardware needed
cargo clippy --all-targets -- -D warnings
cargo run -p opensound-daemon     # run the daemon in the foreground
cargo run -p opensound-gtk        # run the app
sh packaging/build-rpm.sh         # build the Fedora RPM into dist/
sh packaging/build-tarball.sh     # build the generic Linux tarball into dist/
```

See [CONTRIBUTING.md](CONTRIBUTING.md). Do **not** commit third-party proprietary software, firmware or code derived from it. The [`.gitignore`](.gitignore) already excludes them.

## Known limitations

- The UI is currently in Spanish only. Translations are welcome.
- Changing the voice guidance language is not supported, because it requires a firmware package download.
- Firmware updates are not supported and will not be added.
- Turning off the touch panel makes the headphones unassign the voice assistant. OpenSound cannot assign it again.

## AI transparency

OpenSound was developed with the assistance of an AI coding agent (Claude Code), under human direction, review and testing on real hardware. Instructions for AI agents working on this repository are in [AGENTS.md](AGENTS.md), and an index for language models is in [llms.txt](llms.txt).

## Legal notice

- OpenSound is an **unofficial**, independent, **non-profit** community project with no commercial purpose. It is **not** affiliated with, authorized, maintained, sponsored or endorsed by Sony Group Corporation, Sony Corporation, Sony Electronics Inc. or any of their affiliates.
- "Sony", "WH-1000XM5", "LDAC", "DSEE" and all related names, marks and logos are trademarks or registered trademarks of their respective owners. They are used here only to identify the compatible hardware (nominative use) and do not imply any association.
- This repository contains **no** proprietary code, binaries, firmware, images or other assets from Sony or any other vendor. The protocol knowledge was obtained for the sole purpose of interoperability between hardware lawfully owned by the author and the Linux operating system, for which no official software exists.
- The software is provided **"as is", without warranty of any kind**. See sections 15–17 of the [license](LICENSE). Use at your own risk. The authors are not liable for any damage to devices, loss of data, warranty issues or any other consequence of using this software.

See [DISCLAIMER.md](DISCLAIMER.md) for the full text.

## License

Copyright © 2026 Juan Pablo Giraldo Collazos and contributors.

OpenSound is free software: you can redistribute it and/or modify it under the terms of the [GNU General Public License v3.0 or later](LICENSE).
