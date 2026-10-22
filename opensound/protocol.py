# SPDX-License-Identifier: GPL-3.0-or-later
"""WH-1000XM5 MDR v2 commands: request builders and response parser.

Byte layouts documented in docs/PROTOCOL.md and confirmed against a WH-1000XM5 on firmware 2.5.1.
Table 1 frames use data type 0x0C, table 2 frames use 0x0E.
"""
from .transport import DT_MDR, DT_MDR2

CODECS = {0x01: "SBC", 0x02: "AAC", 0x10: "LDAC", 0x20: "aptX", 0x21: "aptX HD", 0x30: "LC3"}
# auto power off options (type 0x05)
AUTO_POWER_OFF = {0x00: "5 min", 0x01: "30 min", 0x02: "60 min", 0x03: "180 min", 0x04: "15 min",
                  0x10: "Al quitarse los auriculares", 0x11: "No apagar"}
# functions the [NC/AMB] button can cycle through (type 0x30)
NC_AMB_BUTTON = {0x01: "NC / Ambiente / Desactivado", 0x02: "NC / Ambiente",
                 0x03: "NC / Desactivado", 0x04: "Ambiente / Desactivado"}
CONNECTION_MODE = {0x00: "Prioridad a la calidad de sonido", 0x01: "Prioridad a la conexión estable"}
LANGUAGES = {0: "Indefinido", 1: "Inglés", 2: "Francés", 3: "Alemán", 4: "Español", 5: "Italiano",
             6: "Portugués", 7: "Neerlandés", 8: "Sueco", 9: "Finlandés", 10: "Ruso", 11: "Japonés",
             13: "Portugués (Brasil)", 15: "Coreano", 16: "Turco", 0xF0: "Chino"}
EQ_BANDS = ("Clear Bass", "400", "1k", "2.5k", "6.3k", "16k")
CUSTOM_PRESETS = range(0xA0, 0xA6)  # editable presets: custom and user 1..5
# alert message IDs: confirmations the headset asks for before disruptive changes
ALERTS = {
    0x00: "Cambiar la calidad de conexión desconectará y reconectará los auriculares.",
    0x06: "Al cambiar la conexión a 2 dispositivos, los auriculares se reconectarán y LDAC no estará disponible.",
    0x07: "Al cambiar la conexión a 2 dispositivos, los auriculares se desconectarán y volverán a conectarse.",
    0x0A: "Si desactivas el panel táctil, no podrás usarlo para la música, las llamadas ni el asistente de voz.",
    0x0B: "Si desactivas el panel táctil, no podrás usarlo para la música, las llamadas ni el asistente de voz. "
          "Los auriculares se reconectarán.",
    0x0C: "Se activará el panel táctil.",
    0x0D: "Se activará el panel táctil y los auriculares se reconectarán.",
    0x1C: "Con prioridad a la calidad de sonido, la conexión puede ser menos estable.",
    0x70: "Con 2 dispositivos conectados y LDAC, el sonido puede cortarse.",
    0x71: "Prioridad a la calidad de sonido con 2 dispositivos conectados puede provocar cortes.",
}

# (data type, payload) queried after connecting
INIT_QUERIES = [
    (DT_MDR, b"\x00\x00"),      # protocol info (handshake)
    (DT_MDR, b"\x94\x00\x00"),  # enable alerts; without it the headset rejects changes that need confirmation
    (DT_MDR, b"\x04\x01"),      # model name
    (DT_MDR, b"\x04\x02"),      # firmware version
    (DT_MDR, b"\x22\x00"),      # battery
    (DT_MDR, b"\x12\x02"),      # codec
    (DT_MDR, b"\x66\x17"),      # NC / ambient sound
    (DT_MDR, b"\x66\x30"),      # NC/AMB button
    (DT_MDR, b"\x56\x00"),      # equalizer
    (DT_MDR, b"\xe6\x00"),      # connection quality
    (DT_MDR, b"\xd6\xd1"),      # touch panel (general setting 1)
    (DT_MDR, b"\xd6\xd2"),      # multipoint (general setting 2)
    (DT_MDR, b"\xf6\x01"),      # pause when headphones are removed
    (DT_MDR, b"\x20\x05"),      # auto power off capability
    (DT_MDR, b"\x26\x05"),      # auto power off
    (DT_MDR2, b"\x46\x01"),     # voice guidance
]


def _onoff(b):
    return b == 0  # protocol booleans: ON=0, OFF=1


def parse(data_type, p, state):
    """Update `state` from one incoming payload. Returns True when something changed."""
    if len(p) < 2:
        return False
    before = dict(state)
    cmd, kind = p[0], p[1]
    if data_type == DT_MDR:
        if cmd == 0x05 and kind in (1, 2):
            state["model" if kind == 1 else "firmware"] = p[3:3 + p[2]].decode(errors="replace")
        elif cmd in (0x23, 0x25) and kind == 0:
            state["battery"] = p[2]
            state["charging"] = p[3] == 1
        elif cmd in (0x13, 0x15) and kind == 2:
            state["codec"] = CODECS.get(p[2], f"0x{p[2]:02x}")
        elif cmd in (0x67, 0x69) and kind == 0x17 and len(p) >= 7:
            state["nc_mode"] = "off" if p[3] == 0 else ("ambient" if p[4] == 1 else "nc")
            state["ambient_voice"] = p[5] == 1
            state["ambient_level"] = p[6]
        elif cmd in (0x67, 0x69) and kind == 0x30:
            state["nc_amb_button"] = p[2]
        elif cmd in (0x57, 0x59) and kind == 0 and len(p) >= 4:
            state["eq_preset"] = p[2]
            state["eq"] = [b - 10 for b in p[4:4 + p[3]]]
        elif cmd in (0xE7, 0xE9) and kind == 0:
            state["connection_mode"] = p[2]
        elif cmd in (0xD7, 0xD9) and kind in (0xD1, 0xD2) and len(p) >= 4:
            state["touch_panel" if kind == 0xD1 else "multipoint"] = _onoff(p[3])
        elif cmd in (0xF7, 0xF9) and kind == 0x01:
            state["pause_on_removal"] = _onoff(p[2])
        elif cmd == 0x21 and kind == 0x05:
            state["auto_power_off_options"] = list(p[3:3 + p[2]])
        elif cmd in (0x27, 0x29) and kind == 0x05:
            state["auto_power_off"] = p[2]
        elif cmd == 0x99 and kind == 0x00 and len(p) >= 4:
            state["alert"] = {"id": p[2], "confirm_only": p[3] == 0,
                              "text": ALERTS.get(p[2], f"Los auriculares piden confirmación (código {p[2]}).")}
    elif data_type == DT_MDR2:
        if cmd in (0x47, 0x49) and kind == 0x01 and len(p) >= 3:
            state["voice_guidance"] = _onoff(p[2])
            if len(p) >= 4:
                state["voice_language"] = LANGUAGES.get(p[3], str(p[3]))
    return state != before


def build(key, value, state):
    """Return (data_type, payload) that sets `key` to `value`. Raises ValueError on bad input."""
    if key == "nc_mode" or key in ("ambient_level", "ambient_voice"):
        mode = value if key == "nc_mode" else state.get("nc_mode", "ambient")
        level = int(value) if key == "ambient_level" else state.get("ambient_level", 20)
        voice = bool(value) if key == "ambient_voice" else state.get("ambient_voice", False)
        if mode not in ("nc", "ambient", "off") or not 0 <= level <= 20:
            raise ValueError("nc_mode must be nc/ambient/off, ambient_level 0..20")
        return DT_MDR, bytes((0x68, 0x17, 0x01, mode != "off", mode == "ambient", voice, level))
    if key == "eq":
        bands = [int(v) for v in value]
        if len(bands) != len(EQ_BANDS) or any(not -10 <= v <= 10 for v in bands):
            raise ValueError("eq needs 6 values in -10..10 (Clear Bass, 400, 1k, 2.5k, 6.3k, 16k)")
        preset = state.get("eq_preset", 0xA0)
        if preset not in CUSTOM_PRESETS:
            preset = 0xA0
        return DT_MDR, bytes((0x58, 0x00, preset, len(bands))) + bytes(v + 10 for v in bands)
    if key == "nc_amb_button":
        if value not in NC_AMB_BUTTON:
            raise ValueError("unknown nc_amb_button value")
        return DT_MDR, bytes((0x68, 0x30, value))
    if key == "connection_mode":
        if value not in CONNECTION_MODE:
            raise ValueError("unknown connection_mode value")
        return DT_MDR, bytes((0xE8, 0x00, value))
    if key in ("touch_panel", "multipoint"):
        return DT_MDR, bytes((0xD8, 0xD1 if key == "touch_panel" else 0xD2, 0x00, 0 if value else 1))
    if key == "pause_on_removal":
        return DT_MDR, bytes((0xF8, 0x01, 0 if value else 1))
    if key == "auto_power_off":
        if value not in AUTO_POWER_OFF:
            raise ValueError("unknown auto_power_off value")
        # second byte is a remembered timer; the WH-1000XM5 has none, so send 0x00
        return DT_MDR, bytes((0x28, 0x05, value, 0x00))
    if key == "alert_response":
        alert = state.get("alert")
        if not alert:
            raise ValueError("no pending alert")
        return DT_MDR, bytes((0x98, 0x00, alert["id"], 1 if value or alert["confirm_only"] else 0))
    if key == "voice_guidance":
        return DT_MDR2, bytes((0x48, 0x01, 0 if value else 1))
    raise ValueError(f"unknown setting {key!r}")
