# SPDX-License-Identifier: GPL-3.0-or-later
"""MDR transport: SDP lookup, RFCOMM socket, frame codec, ACK/sequence handling.

See docs/PROTOCOL.md for the framing, escaping, checksum and ACK rules.
"""
import socket
import threading
import uuid

UUID_V2 = uuid.UUID("956C7B26-D49A-4BA8-B03F-B17D393CB6E2")
UUID_V1 = uuid.UUID("96CC203E-5068-46ad-B32D-E316F5E069BA")

START, END, ESC = 0x3E, 0x3C, 0x3D
DT_ACK, DT_MDR, DT_MDR2 = 0x01, 0x0C, 0x0E


def escape(data):
    out = bytearray()
    for b in data:
        if b in (START, END, ESC):
            out += bytes((ESC, b & 0xEF))
        else:
            out.append(b)
    return bytes(out)


def unescape(data):
    out, i = bytearray(), 0
    while i < len(data):
        if data[i] == ESC:
            i += 1
            out.append(data[i] | 0x10)
        else:
            out.append(data[i])
        i += 1
    return bytes(out)


def encode(data_type, seq, payload):
    body = bytes((data_type, seq)) + len(payload).to_bytes(4, "big") + payload
    body += bytes((sum(body) & 0xFF,))
    return bytes((START,)) + escape(body) + bytes((END,))


def decode(raw):
    """raw = bytes between START and END. Returns (data_type, seq, payload) or None on bad checksum."""
    body = unescape(raw)
    if len(body) < 7 or sum(body[:-1]) & 0xFF != body[-1]:
        return None
    n = int.from_bytes(body[2:6], "big")
    return body[0], body[1], body[6:6 + n]


def sdp_rfcomm_channel(addr, service_uuid=UUID_V2):
    """Minimal SDP ServiceSearchAttributeRequest over L2CAP PSM 1; returns RFCOMM channel or None."""
    s = socket.socket(socket.AF_BLUETOOTH, socket.SOCK_SEQPACKET, socket.BTPROTO_L2CAP)
    s.settimeout(10)
    try:
        s.connect((addr, 1))
        pattern = bytes((0x35, 17, 0x1C)) + service_uuid.bytes
        attrs = bytes((0x35, 3, 0x09, 0x00, 0x04))  # ProtocolDescriptorList
        cont, data, tid = b"\x00", b"", 1
        while True:
            params = pattern + b"\xff\xff" + attrs + cont
            s.send(bytes((0x06,)) + tid.to_bytes(2, "big") + len(params).to_bytes(2, "big") + params)
            resp = s.recv(4096)
            if resp[0] != 0x07:
                return None
            count = int.from_bytes(resp[5:7], "big")
            data += resp[7:7 + count]
            cont = resp[7 + count:]
            tid += 1
            if not cont or cont[0] == 0:
                break
    finally:
        s.close()
    # RFCOMM UUID16 0x0003 followed by uint8 channel
    i = data.find(b"\x19\x00\x03\x08")
    return data[i + 4] if i >= 0 else None


class Transport:
    """One RFCOMM link. on_message(data_type, payload) runs on the reader thread."""

    def __init__(self, addr, on_message, on_close=None):
        self.addr, self.on_message, self.on_close = addr, on_message, on_close
        self.sock = None
        self.seq = 0
        self.last_rx_seq = -1
        self.ack = threading.Condition()
        self.acked = False
        self.lock = threading.Lock()  # one outstanding command at a time, as the device expects

    def connect(self):
        channel = sdp_rfcomm_channel(self.addr) or sdp_rfcomm_channel(self.addr, UUID_V1)
        if channel is None:
            raise OSError("Sony MDR service not found on device")
        self.sock = socket.socket(socket.AF_BLUETOOTH, socket.SOCK_STREAM, socket.BTPROTO_RFCOMM)
        self.sock.settimeout(10)
        self.sock.connect((self.addr, channel))
        self.sock.settimeout(None)
        threading.Thread(target=self._reader, daemon=True).start()

    def close(self):
        if self.sock:
            try:
                self.sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.sock.close()

    def send(self, payload, data_type=DT_MDR, retries=3):
        with self.lock:
            for _ in range(retries):
                with self.ack:
                    self.acked = False
                    self.sock.sendall(encode(data_type, self.seq, payload))
                    if self.ack.wait_for(lambda: self.acked, timeout=1.5):
                        return True
            return False

    def _reader(self):
        buf, inframe = bytearray(), False
        try:
            while True:
                chunk = self.sock.recv(4096)
                if not chunk:
                    break
                for b in chunk:
                    if not inframe:
                        inframe = b == START
                    elif b == END:
                        frame = decode(bytes(buf))
                        buf.clear()
                        inframe = False
                        if frame:
                            self._handle(*frame)
                    else:
                        buf.append(b)
        except OSError:
            pass
        finally:
            if self.on_close:
                self.on_close()

    def _handle(self, data_type, seq, payload):
        if data_type == DT_ACK:
            with self.ack:
                if seq != self.seq:
                    self.seq = seq
                    self.acked = True
                    self.ack.notify_all()
            return
        self.sock.sendall(encode(DT_ACK, 1 - seq, b""))
        if seq == self.last_rx_seq:
            return  # retransmission
        self.last_rx_seq = seq
        self.on_message(data_type, payload)
