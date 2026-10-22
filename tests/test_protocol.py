# SPDX-License-Identifier: GPL-3.0-or-later
import unittest

from opensound import protocol, transport
from opensound.transport import DT_MDR, DT_MDR2


class TransportTest(unittest.TestCase):
    def test_frame_roundtrip_with_escaping(self):
        payload = bytes((0x3C, 0x3D, 0x3E, 0x00, 0x68))
        raw = transport.encode(DT_MDR, 1, payload)
        self.assertEqual(raw[0], 0x3E)
        self.assertEqual(raw[-1], 0x3C)
        self.assertNotIn(0x3C, raw[1:-1])
        self.assertEqual(transport.decode(raw[1:-1]), (DT_MDR, 1, payload))

    def test_known_frame(self):
        # CONNECT_GET_PROTOCOL_INFO handshake frame
        self.assertEqual(transport.encode(DT_MDR, 0, b"\x00\x00").hex(), "3e0c000000000200000e3c")

    def test_bad_checksum(self):
        raw = bytearray(transport.encode(DT_MDR, 0, b"\x22\x00"))
        raw[-2] ^= 1
        self.assertIsNone(transport.decode(bytes(raw[1:-1])))


class ParseTest(unittest.TestCase):
    """Payloads captured from a WH-1000XM5, firmware 2.5.1."""

    def parse(self, hexstr, dt=DT_MDR, state=None):
        state = {} if state is None else state
        protocol.parse(dt, bytes.fromhex(hexstr), state)
        return state

    def test_device_info(self):
        self.assertEqual(self.parse("05 01 0a 57 48 2d 31 30 30 30 58 4d 35")["model"], "WH-1000XM5")
        self.assertEqual(self.parse("05 02 05 32 2e 35 2e 31")["firmware"], "2.5.1")

    def test_battery_codec(self):
        self.assertEqual(self.parse("23 00 3a 00"), {"battery": 58, "charging": False})
        self.assertEqual(self.parse("13 02 10")["codec"], "LDAC")

    def test_ncasm(self):
        self.assertEqual(self.parse("67 17 01 01 00 00 14"),
                         {"nc_mode": "nc", "ambient_voice": False, "ambient_level": 20})
        self.assertEqual(self.parse("69 17 01 01 01 00 0a")["nc_mode"], "ambient")
        self.assertEqual(self.parse("69 17 01 00 00 00 0a")["nc_mode"], "off")

    def test_eq(self):
        s = self.parse("57 00 a2 06 10 0a 0a 0b 0c 0d")
        self.assertEqual(s["eq"], [6, 0, 0, 1, 2, 3])
        self.assertEqual(s["eq_preset"], 0xA2)

    def test_settings(self):
        s = {}
        for p in ("d7 d1 00 00", "d7 d2 00 01", "f7 01 00", "27 05 10 00", "21 05 02 10 11", "e7 00 00", "67 30 02"):
            self.parse(p, state=s)
        self.parse("47 01 00 04", DT_MDR2, s)
        self.assertEqual(s, {"touch_panel": True, "multipoint": False, "pause_on_removal": True,
                             "auto_power_off": 0x10, "auto_power_off_options": [0x10, 0x11],
                             "connection_mode": 0, "nc_amb_button": 2,
                             "voice_guidance": True, "voice_language": "Español"})


class AlertTest(unittest.TestCase):
    def test_alert_roundtrip(self):
        s = {}
        protocol.parse(DT_MDR, bytes.fromhex("99 00 07 01"), s)  # multipoint change, positive/negative
        self.assertEqual(s["alert"]["id"], 7)
        self.assertFalse(s["alert"]["confirm_only"])
        self.assertEqual(protocol.build("alert_response", False, s)[1], bytes.fromhex("98 00 07 00"))
        self.assertEqual(protocol.build("alert_response", True, s)[1], bytes.fromhex("98 00 07 01"))

    def test_confirm_only_always_positive(self):
        s = {}
        protocol.parse(DT_MDR, bytes.fromhex("99 00 0a 00"), s)
        self.assertEqual(protocol.build("alert_response", False, s)[1], bytes.fromhex("98 00 0a 01"))

    def test_no_pending_alert(self):
        with self.assertRaises(ValueError):
            protocol.build("alert_response", True, {})


class BuildTest(unittest.TestCase):
    def test_ncasm(self):
        st = {"nc_mode": "nc", "ambient_level": 20, "ambient_voice": False}
        self.assertEqual(protocol.build("nc_mode", "ambient", st), (DT_MDR, bytes.fromhex("68 17 01 01 01 00 14")))
        self.assertEqual(protocol.build("nc_mode", "off", st), (DT_MDR, bytes.fromhex("68 17 01 00 00 00 14")))
        st["nc_mode"] = "ambient"
        self.assertEqual(protocol.build("ambient_level", 5, st), (DT_MDR, bytes.fromhex("68 17 01 01 01 00 05")))

    def test_eq(self):
        self.assertEqual(protocol.build("eq", [6, 0, 0, 1, 2, 3], {"eq_preset": 0xA2}),
                         (DT_MDR, bytes.fromhex("58 00 a2 06 10 0a 0a 0b 0c 0d")))
        self.assertEqual(protocol.build("eq", [0] * 6, {"eq_preset": 0x01})[1][2], 0xA0)  # ROCK -> CUSTOM
        with self.assertRaises(ValueError):
            protocol.build("eq", [11, 0, 0, 0, 0, 0], {})

    def test_settings(self):
        self.assertEqual(protocol.build("multipoint", True, {})[1], bytes.fromhex("d8 d2 00 00"))
        self.assertEqual(protocol.build("touch_panel", False, {})[1], bytes.fromhex("d8 d1 00 01"))
        self.assertEqual(protocol.build("pause_on_removal", True, {})[1], bytes.fromhex("f8 01 00"))
        self.assertEqual(protocol.build("auto_power_off", 0x11, {})[1], bytes.fromhex("28 05 11 00"))
        self.assertEqual(protocol.build("voice_guidance", False, {}), (DT_MDR2, bytes.fromhex("48 01 01")))
        with self.assertRaises(ValueError):
            protocol.build("nope", 1, {})


if __name__ == "__main__":
    unittest.main()
