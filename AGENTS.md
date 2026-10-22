# AGENTS.md

Guidance for AI coding agents (Claude Code, Codex, Copilot, Cursor, Gemini, …) and for humans working on OpenSound. This file follows the [agents.md](https://agents.md) convention. `CLAUDE.md` imports it.

## Project overview

OpenSound is an unofficial, GPL-3.0-or-later Linux controller for WH-1000XM5 headphones. It is a non-profit project that extends the headphones' connectivity to Linux, where no official option exists. It speaks the MDR v2 control protocol over Bluetooth Classic RFCOMM.

Everything is Rust (Cargo workspace), except the GNOME Shell extension, which must be GJS.

- `crates/opensound-core/src/frame.rs`: frame codec (escaping, checksum, ACK frames) and the streaming `FrameReader`.
- `crates/opensound-core/src/protocol.rs`: pure functions with no I/O. `parse(data_type, payload, &mut State)` updates the state from a device message. `build(key, &Value, &State)` returns the `(data_type, payload)` that changes a setting. `INIT_QUERIES` lists what is read after connecting. `State` serializes to the JSON shared over D-Bus.
- `crates/opensound-daemon/src/bt.rs`: libc Bluetooth sockets (L2CAP for SDP, RFCOMM) and the SDP request/response codec.
- `crates/opensound-daemon/src/link.rs`: one control link: stop-and-wait sends with ACK and retries, and a reader thread.
- `crates/opensound-daemon/src/main.rs`: session D-Bus service `io.github.cpujp.OpenSound` (zbus). A single worker thread owns the link, polls BlueZ for a connected headset and sends queued frames. Methods: `GetState() → s`, `Set(s key, s value_json)`, signal `StateChanged(s)`.
- `crates/opensound-gtk/src/main.rs`: GTK 4 / libadwaita app (`opensound`, app ID `io.github.cpujp.OpenSound.App`), a D-Bus client of the daemon.
- `gnome-extension/opensound@cpujp.github.io/`: GNOME Shell 48–50 extension (ES modules), a D-Bus client of the daemon.
- `data/`: systemd user unit, D-Bus activation file, desktop file and AppStream metainfo.
- `packaging/`: Fedora spec and the RPM / tarball build scripts used by CI and releases.
- `docs/PROTOCOL.md`: byte-level protocol reference. **Keep it in sync with `protocol.rs`.**

Data flow: headset ⇄ `link` ⇄ daemon (uses `protocol`) ⇄ D-Bus ⇄ app / extension. The headset allows **one** control link, so never open RFCOMM from the UIs.

## Setup and commands

Requires Rust ≥ 1.92. The GTK app also needs `gtk4-devel` and `libadwaita-devel` (Fedora) or `libgtk-4-dev` and `libadwaita-1-dev` (Debian/Ubuntu). The core and daemon crates build without GTK.

```sh
cargo test                                   # all tests (no hardware needed)
cargo test -p opensound-core -p opensound-daemon   # tests without GTK installed
cargo fmt --all && cargo clippy --all-targets -- -D warnings
cargo run -p opensound-daemon                # run the daemon in the foreground
cargo run -p opensound-gtk                   # run the app
sh install.sh                                # per-user install (builds with cargo if no ./bin)
sh uninstall.sh
sh packaging/build-rpm.sh                    # Fedora RPM into dist/ (needs a commit: uses git archive)
journalctl --user -u opensound -f            # daemon log when installed
```

To inspect the live state:

```sh
gdbus call --session -d io.github.cpujp.OpenSound -o /io/github/cpujp/OpenSound -m io.github.cpujp.OpenSound.GetState
```

Only one daemon can own the link. Stop the installed one (`systemctl --user stop opensound`) before running another or a raw RFCOMM script.

## Code style

- Rust 2021 edition, formatted with `cargo fmt` (`rustfmt.toml`: 120 columns). Clippy must pass with `-D warnings`.
- Keep `opensound-core` free of I/O and of GTK, so it stays testable everywhere. Add a dependency only when the standard library or an existing crate cannot do the job.
- Follow the surrounding style: small modules, no extra abstraction layers, and short comments that explain *why*.
- Every source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later` (`#` in shell, spec and TOML files).
- The UI strings are in Spanish today. Keep new strings consistent with that until i18n (gettext) is added.
- The GNOME extension must use the ES-module APIs of GNOME 45+ and clean up everything in `disable()`.

## Protocol rules (read before touching `protocol.rs`)

- Frames: `3E type seq len32 payload checksum 3C`. `type` is `0C` (table 1), `0E` (table 2) or `01` (ACK). See `docs/PROTOCOL.md`.
- Booleans in this protocol are **`00` = ON, `01` = OFF**.
- `parse` must accept both the `RET` (reply) and `NTFY` (notification) command bytes for each function, and must never panic on truncated payloads (there is a test that feeds every short payload).
- Changes that disconnect or restart the headphones, such as multipoint and the touch panel, require the **alert handshake**. The handshake (`94 00 00` → `99 00 id kind` → `98 00 id yes/no`) is already wired in. Without it, the device silently rejects the change.
- Every new protocol feature needs a test with a real captured payload, plus a row in `docs/PROTOCOL.md`.
- The JSON keys of `State` are a contract with the GNOME extension and the app. Keep them stable, and update the JSON tests in `protocol.rs` when adding keys.

## Testing on real hardware

- Read before you write. Record the current value, apply the change, verify the notification, and **restore the original value**.
- Some writes restart the headset's Bluetooth. Turning the touch panel off also makes the device unassign the voice assistant, and OpenSound cannot assign it again. Ask the human before testing that write.
- Never send firmware-update (`UPDT_*`) or factory-reset commands.

## Legal and safety boundaries (strict)

- **Never commit third-party proprietary material.** That covers proprietary software or any part of it, code derived from it, firmware, and third-party images, sounds or strings. Describe protocol findings in your own words as byte layouts.
- Do not add manufacturer logos or branding. Do not present the project as official. Keep the disclaimer in `README.md` and `DISCLAIMER.md` intact.
- Do not commit personal data such as Bluetooth addresses, device names or user names in tests, docs or screenshots.

## Commits and pull requests

- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`).
- Run `cargo fmt`, clippy and the tests before committing. CI (`.github/workflows/ci.yml`) checks formatting, runs the tests, runs clippy and builds the RPM on Fedora, and syntax-checks the extension.
- Releases: bump `version` in `Cargo.toml` and `Version` in `packaging/opensound.spec` (plus a `%changelog` entry), add the release to the AppStream metainfo, and move `CHANGELOG.md` entries under the new version. When this reaches `main`, `.github/workflows/release.yml` builds the RPM and the tarball and publishes the GitHub release.
- Update `CHANGELOG.md` under `[Unreleased]` for user-visible changes.
- AI-assisted contributions are welcome. Disclose them in the PR description, and a human must review and test them (see `CONTRIBUTING.md`).
