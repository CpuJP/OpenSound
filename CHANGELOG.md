# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.1] - 2026-10-22

### Changed

- The GNOME Shell extension now has its own top-bar button. It opens a control card with the battery level, the noise control modes, the ambient level and the focus on voice switch.

## [0.2.0] - 2026-10-22

### Changed

- The daemon and the app are rewritten in Rust (`opensound-core`, `opensound-daemon` and the `opensound` GTK 4 / libadwaita app). The D-Bus API is unchanged.
- The app ID is now `io.github.cpujp.OpenSound.App`, so it no longer collides with the daemon's bus name.

### Added

- D-Bus activation: the daemon starts on demand when the app or the GNOME Shell extension needs it.
- Fedora RPM package and a generic Linux tarball, published automatically for each release.
- AppStream metadata.
- 55 unit and integration tests: framing, protocol parsing and building, JSON contract, SDP, the ACK and retransmission logic against a simulated headset, and D-Bus method validation.

### Fixed

- The GNOME Shell extension no longer throws an error when the daemon is not running.

### Removed

- The previous 0.1.x implementation.

## [0.1.0] - 2026-10-21

### Added

- Daemon that owns the RFCOMM link to the headphones and exposes a session D-Bus API.
- GTK 4 / libadwaita app with live status: battery, codec, model and firmware.
- Ambient sound control: noise cancelling, ambient sound or off, ambient level and focus on voice.
- 5-band equalizer and Clear Bass.
- Multipoint and Bluetooth connection quality.
- [NC/AMB] button behaviour, touch panel, and pause when the headphones are taken off.
- Automatic power off and voice guidance.
- Confirmation dialogs for changes that the headphones must approve.
- GNOME Shell extension with a top-bar battery indicator and a Quick Settings toggle.
- Per-user `install.sh` and `uninstall.sh`.
- Project documentation for contributors and AI agents: `AGENTS.md`, `CLAUDE.md`, `llms.txt` and `docs/PROTOCOL.md`.
