# SPDX-License-Identifier: GPL-3.0-or-later
"""OpenSound daemon: owns the single RFCOMM link and exposes it on the session bus.

Bus name io.github.cpujp.OpenSound, object /io/github/cpujp/OpenSound.
GUI and GNOME Shell extension are clients; the headset accepts only one MDR link.
"""
import json
import queue
import threading

from gi.repository import Gio, GLib

from . import protocol
from .transport import Transport, UUID_V2

BUS_NAME = "io.github.cpujp.OpenSound"
OBJ_PATH = "/io/github/cpujp/OpenSound"
IFACE_XML = """
<node>
  <interface name="io.github.cpujp.OpenSound">
    <method name="GetState"><arg type="s" direction="out"/></method>
    <method name="Set">
      <arg name="key" type="s" direction="in"/>
      <arg name="value_json" type="s" direction="in"/>
    </method>
    <signal name="StateChanged"><arg type="s"/></signal>
  </interface>
</node>
"""
POLL_SECONDS = 5


def find_headset(bus):
    """First connected BlueZ device that advertises the Sony MDR v2 service."""
    objs = bus.call_sync("org.bluez", "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects",
                         None, None, Gio.DBusCallFlags.NONE, -1, None).unpack()[0]
    for ifaces in objs.values():
        dev = ifaces.get("org.bluez.Device1")
        if dev and dev.get("Connected") and str(UUID_V2).lower() in [u.lower() for u in dev.get("UUIDs", [])]:
            return dev["Address"], dev.get("Alias", "")
    return None


class Daemon:
    def __init__(self):
        self.state = {"connected": False}
        self.transport = None
        self.jobs = queue.Queue()  # sends block on ACKs; keep them off the main loop
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.system_bus = Gio.bus_get_sync(Gio.BusType.SYSTEM)
        node = Gio.DBusNodeInfo.new_for_xml(IFACE_XML)
        self.bus.register_object(OBJ_PATH, node.interfaces[0], self._on_call)
        Gio.bus_own_name_on_connection(self.bus, BUS_NAME, Gio.BusNameOwnerFlags.NONE, None, None)
        threading.Thread(target=self._worker, daemon=True).start()
        GLib.timeout_add_seconds(POLL_SECONDS, self._poll)
        self._poll()

    # --- link management (main loop) ---
    def _poll(self):
        if self.transport is None:
            try:
                found = find_headset(self.system_bus)
            except GLib.Error:
                found = None
            if found:
                self.transport = Transport(found[0], self._on_message, self._on_close)
                self.jobs.put(("connect", found))
        return True

    def _on_close(self):
        GLib.idle_add(self._closed)

    def _closed(self):
        self.transport = None
        self.state = {"connected": False}
        self._emit()

    # --- worker thread ---
    def _worker(self):
        while True:
            job, arg = self.jobs.get()
            t = self.transport
            if t is None:
                continue
            try:
                if job == "connect":
                    t.connect()
                    GLib.idle_add(self._update, {"connected": True, "address": arg[0], "name": arg[1]})
                    for dt, payload in protocol.INIT_QUERIES:
                        t.send(payload, dt)
                else:
                    t.send(arg[1], arg[0])
            except OSError as e:
                print(f"opensound: link error: {e}", flush=True)
                t.close()
                if job == "connect":
                    GLib.idle_add(self._closed)

    def _on_message(self, data_type, payload):
        GLib.idle_add(self._apply, data_type, payload)

    def _apply(self, data_type, payload):
        # emit even when unchanged: a rejected Set comes back as NTFY with the old value and clients must revert
        protocol.parse(data_type, payload, self.state)
        self._emit()

    def _update(self, values):
        self.state.update(values)
        self._emit()

    def _emit(self):
        self.bus.emit_signal(None, OBJ_PATH, BUS_NAME, "StateChanged",
                             GLib.Variant("(s)", (json.dumps(self.state),)))

    # --- D-Bus ---
    def _on_call(self, conn, sender, path, iface, method, params, invocation):
        if method == "GetState":
            invocation.return_value(GLib.Variant("(s)", (json.dumps(self.state),)))
            return
        key, raw = params.unpack()
        try:
            if not self.state.get("connected"):
                raise ValueError("headphones not connected")
            frame = protocol.build(key, json.loads(raw), self.state)
            if key == "alert_response":
                del self.state["alert"]
                self._emit()
        except (ValueError, TypeError) as e:
            invocation.return_dbus_error("io.github.cpujp.OpenSound.Error", str(e))
            return
        self.jobs.put(("send", frame))
        invocation.return_value(None)


def main():
    Daemon()
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
