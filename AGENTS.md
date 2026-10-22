# AGENTS.md

Guidance for AI coding agents (Claude Code, Codex, Copilot, Cursor, Gemini, …) and for humans working on OpenSound. This file follows the [agents.md](https://agents.md) convention. `CLAUDE.md` imports it.

## Project overview

OpenSound is an unofficial, GPL-3.0-or-later Linux controller for WH-1000XM5 headphones. It is a non-profit project that extends the headphones' connectivity to Linux, where no official option exists. It speaks the MDR v2 control protocol over Bluetooth Classic RFCOMM.

- `opensound/transport.py`: SDP channel lookup, RFCOMM socket, frame codec (escaping, checksum), ACK and sequence handling. No protocol semantics.
- `opensound/protocol.py`: pure functions with no I/O. `parse(data_type, payload, state)` updates a state dict from a device message. `build(key, value, state)` returns the `(data_type, payload)` that changes a setting. `INIT_QUERIES` lists what is read after connecting.
- `opensound/daemon.py`: session D-Bus service `io.github.cpujp.OpenSound`. It owns the only RFCOMM link, polls BlueZ for a connected headset, and runs blocking sends on a worker thread. Methods: `GetState() → s`, `Set(s key, s value_json)`, signal `StateChanged(s)`.
- `opensound/app.py`: GTK 4 / libadwaita UI, a D-Bus client of the daemon.
- `gnome-extension/opensound@cpujp.github.io/`: GNOME Shell 48–50 extension (ES modules), a D-Bus client of the daemon.
- `docs/PROTOCOL.md`: byte-level protocol reference. **Keep it in sync with `protocol.py`.**
- `tests/test_protocol.py`: unit tests built from payloads captured on real hardware.

Data flow: headset ⇄ `transport` ⇄ `daemon` (uses `protocol`) ⇄ D-Bus ⇄ app / extension. The headset allows **one** control link, so never open RFCOMM from the UIs.

## Setup and commands

There are no third-party Python dependencies. The project uses the system PyGObject, GTK 4 and libadwaita ≥ 1.7.

```sh
python3 -m unittest discover -s tests -t .   # run all tests (no hardware needed)
python3 -m compileall -q opensound           # syntax check
python3 -m opensound.daemon                  # run the daemon in the foreground
python3 -m opensound                         # run the app from the source tree
sh install.sh                                # per-user install (systemd user unit, app, extension)
sh uninstall.sh
journalctl --user -u opensound -f            # daemon log when installed
```

To inspect the live state:

```sh
gdbus call --session -d io.github.cpujp.OpenSound -o /io/github/cpujp/OpenSound -m io.github.cpujp.OpenSound.GetState
```

Only one daemon can own the link. Stop the installed one (`systemctl --user stop opensound`) before running another or a raw RFCOMM script.

## Code style

- Python ≥ 3.10, standard library and PyGObject only. Do not add pip dependencies.
- Follow the surrounding style: small modules, no extra abstraction layers, and short comments that explain *why*.
- Every source file starts with `# SPDX-License-Identifier: GPL-3.0-or-later` (`//` in JavaScript).
- The UI strings are in Spanish today. Keep new strings consistent with that until i18n (gettext) is added.
- The GNOME extension must use the ES-module APIs of GNOME 45+ and clean up everything in `disable()`.

## Protocol rules (read before touching `protocol.py`)

- Frames: `3E type seq len32 payload checksum 3C`. `type` is `0C` (table 1), `0E` (table 2) or `01` (ACK). See `docs/PROTOCOL.md`.
- Booleans in this protocol are **`00` = ON, `01` = OFF**.
- `parse` must accept both the `RET` (reply) and `NTFY` (notification) command bytes for each function.
- Changes that disconnect or restart the headphones, such as multipoint and the touch panel, require the **alert handshake**. The handshake (`94 00 00` → `99 00 id kind` → `98 00 id yes/no`) is already wired in. Without it, the device silently rejects the change.
- Every new protocol feature needs a test with a real captured payload, plus a row in `docs/PROTOCOL.md`.

## Testing on real hardware

- Read before you write. Record the current value, apply the change, verify the notification, and **restore the original value**.
- Some writes restart the headset's Bluetooth. Turning the touch panel off also makes the device unassign the voice assistant, and OpenSound cannot assign it again. Ask the human before testing that write.
- Never send firmware-update (`UPDT_*`) or factory-reset commands.

## Legal and safety boundaries (strict)

- **Never commit third-party proprietary material.** That covers proprietary software or any part of it, code derived from it, firmware, and third-party images, sounds or strings. Folders matching `*_extracted/` are git-ignored for local research material. Describe protocol findings in your own words as byte layouts.
- Do not add manufacturer logos or branding. Do not present the project as official. Keep the disclaimer in `README.md` and `DISCLAIMER.md` intact.
- Do not commit personal data such as Bluetooth addresses, device names or user names in tests, docs or screenshots.

## Commits and pull requests

- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`).
- Run the tests before committing. CI (`.github/workflows/ci.yml`) runs them on Python 3.10–3.13 and syntax-checks the extension.
- Update `CHANGELOG.md` under `[Unreleased]` for user-visible changes.
- AI-assisted contributions are welcome. Disclose them in the PR description, and a human must review and test them (see `CONTRIBUTING.md`).
