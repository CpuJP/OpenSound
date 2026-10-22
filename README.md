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

- Linux with BlueZ, a systemd user session and GNOME 48–50. Developed on Fedora.
- Python ≥ 3.10, PyGObject, GTK 4, libadwaita ≥ 1.7. These are preinstalled on Fedora Workstation.
- The headphones must be paired and connected through the normal Bluetooth settings.

## Installation

```sh
git clone https://github.com/CpuJP/OpenSound.git
cd OpenSound
sh install.sh
```

The installer works per user, needs no root, and installs everything under `~/.local`:

- `opensound.service`: a systemd user service that owns the connection to the headphones.
- The **OpenSound** app, in your application menu, or run `opensound`.
- The GNOME Shell extension `opensound@cpujp.github.io`. It adds a battery indicator to the top bar and a Quick Settings toggle.

On Wayland, log out and back in once. Then run `gnome-extensions enable opensound@cpujp.github.io`.

To remove everything, run `sh uninstall.sh`.

## Architecture

```
Headphones ──RFCOMM (SPP)── opensound daemon ──session D-Bus──┬── GTK app
                                                              └── GNOME Shell extension
```

The headphones accept a single control connection. The daemon is the only process that opens it, and the UIs are D-Bus clients.

D-Bus API on bus name `io.github.cpujp.OpenSound`, object `/io/github/cpujp/OpenSound`:

| Member | Description |
|---|---|
| `GetState() → s` | Current state as JSON |
| `Set(s key, s value_json)` | Change a setting, for example `Set("nc_mode", "\"ambient\"")` |
| signal `StateChanged(s json)` | Emitted on every change, including changes made on the headphones |

The protocol is documented in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Development

```sh
python3 -m unittest discover -s tests -t .   # unit tests, no hardware needed
python3 -m opensound.daemon                  # run the daemon in the foreground
python3 -m opensound                         # run the app from the source tree
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
