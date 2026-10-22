# SPDX-License-Identifier: GPL-3.0-or-later
"""OpenSound GTK4/libadwaita window. Talks to the daemon over D-Bus."""
import json
import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gio, GLib, Gtk  # noqa: E402

from . import protocol  # noqa: E402
from .daemon import BUS_NAME, OBJ_PATH  # noqa: E402

NC_MODES = ("nc", "ambient", "off")


class Window(Adw.ApplicationWindow):
    def __init__(self, app):
        super().__init__(application=app, title="OpenSound", default_width=480, default_height=820)
        self.state = {}
        self.syncing = False  # true while widgets are being updated from device state
        self.timers = {}  # debounce per setting key while sliders are dragged
        self.alert_dialog = None
        self.groups = []
        self.proxy = Gio.DBusProxy.new_for_bus_sync(Gio.BusType.SESSION, Gio.DBusProxyFlags.NONE, None,
                                                    BUS_NAME, OBJ_PATH, BUS_NAME, None)
        self.proxy.connect("g-signal", self._on_signal)

        self.toast = Adw.ToastOverlay()
        view = Adw.ToolbarView()
        view.add_top_bar(Adw.HeaderBar())
        view.set_content(self.toast)
        self.set_content(view)

        page = Adw.PreferencesPage()
        self.toast.set_child(page)

        # Live monitor
        g = self._group(page, "Estado")
        self.status_row = Adw.ActionRow(title="Desconectado", subtitle="Esperando auriculares…")
        self.status_row.add_prefix(Gtk.Image(icon_name="audio-headphones-symbolic", pixel_size=32))
        g.add(self.status_row)
        self.battery_row = Adw.ActionRow(title="Batería")
        self.battery_bar = Gtk.LevelBar(min_value=0, max_value=100, valign=Gtk.Align.CENTER, width_request=120)
        self.battery_row.add_suffix(self.battery_bar)
        g.add(self.battery_row)
        self.codec_row = Adw.ActionRow(title="Códec")
        g.add(self.codec_row)

        # Ambient sound control
        g = self._group(page, "Control de sonido ambiental")
        self.nc_toggle = Adw.ToggleGroup(valign=Gtk.Align.CENTER)
        for name, label in zip(NC_MODES, ("Supresión de ruido", "Sonido ambiente", "Desactivado")):
            self.nc_toggle.add(Adw.Toggle(name=name, label=label))
        self.nc_toggle.connect("notify::active-name", self._on_nc_mode)
        row = Adw.PreferencesRow()
        row.set_child(self.nc_toggle)
        self.nc_toggle.set_margin_top(8)
        self.nc_toggle.set_margin_bottom(8)
        g.add(row)
        self.amb_level = self._scale_row(g, "Nivel de sonido ambiente", 1, 20, "ambient_level")
        self.amb_voice = self._switch_row(g, "Enfocar en la voz", "ambient_voice")

        # Sound quality
        g = self._group(page, "Calidad de sonido / volumen", "Ecualizador (−10 a +10)")
        self.eq_scales = [None] * len(protocol.EQ_BANDS)
        for i in (1, 2, 3, 4, 5, 0):  # band 0 is Clear Bass; the official app shows it last
            band = protocol.EQ_BANDS[i]
            self.eq_scales[i] = self._scale_row(g, band if i == 0 else f"{band} Hz", -10, 10, None)
            self.eq_scales[i].connect("value-changed", self._on_eq)

        # Connection
        g = self._group(page, "Conexión")
        self.multipoint = self._switch_row(g, "Conectar a 2 dispositivos simultáneamente", "multipoint")
        self.conn_mode = self._combo_row(g, "Calidad de conexión Bluetooth", protocol.CONNECTION_MODE, "connection_mode")

        # Controls
        g = self._group(page, "Controles")
        self.nc_button = self._combo_row(g, "Ajustes de operación [NC/AMB]", protocol.NC_AMB_BUTTON, "nc_amb_button")
        self.touch = self._switch_row(g, "Panel de control con sensor táctil", "touch_panel")
        self.pause = self._switch_row(g, "Se detiene al quitarse los auriculares", "pause_on_removal")

        # Power
        g = self._group(page, "Energía / Batería")
        self.apo = self._combo_row(g, "Apagado automático", protocol.AUTO_POWER_OFF, "auto_power_off")

        # System
        g = self._group(page, "Sistema")
        self.fw_row = Adw.ActionRow(title="Versión")
        g.add(self.fw_row)
        self.voice = self._switch_row(g, "Notificación y guía de voz", "voice_guidance")

        try:
            self._apply(json.loads(self.proxy.call_sync("GetState", None, 0, -1, None).unpack()[0]))
        except GLib.Error as e:
            self._apply({"connected": False})
            self.status_row.set_subtitle(f"Servicio no disponible: {e.message}")

    # --- widget helpers ---
    def _group(self, page, title, desc=None):
        g = Adw.PreferencesGroup(title=title, description=desc)
        page.add(g)
        if title != "Estado":
            self.groups.append(g)  # disabled while disconnected
        return g

    def _switch_row(self, group, title, key):
        row = Adw.SwitchRow(title=title)
        row.connect("notify::active", lambda r, _p: self._set(key, r.get_active()))
        group.add(row)
        return row

    def _combo_row(self, group, title, options, key):
        row = Adw.ComboRow(title=title)
        row.values = list(options)
        row.set_model(Gtk.StringList.new(list(options.values())))
        row.connect("notify::selected", lambda r, _p: self._set(key, r.values[r.get_selected()])
                    if r.get_selected() < len(r.values) else None)
        group.add(row)
        return row

    def _scale_row(self, group, title, lo, hi, key):
        row = Adw.ActionRow(title=title)
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, lo, hi, 1)
        scale.set_hexpand(True)
        scale.set_draw_value(True)
        scale.set_digits(0)
        scale.set_size_request(220, -1)
        if lo < 0:
            scale.add_mark(0, Gtk.PositionType.BOTTOM, None)
        if key:
            scale.connect("value-changed", lambda s: self._set_later(key, lambda: int(s.get_value())))
        row.add_suffix(scale)
        group.add(row)
        return scale

    # --- outgoing ---
    def _set(self, key, value):
        if self.syncing:
            return
        self.proxy.call("Set", GLib.Variant("(ss)", (key, json.dumps(value))), 0, -1, None, self._on_set_done)

    def _on_set_done(self, proxy, res):
        try:
            proxy.call_finish(res)
        except GLib.Error as e:
            self.toast.add_toast(Adw.Toast(title=Gio.DBusError.strip_remote_error(e) or e.message))

    def _on_nc_mode(self, toggle, _p):
        self._set("nc_mode", toggle.get_active_name())
        self.amb_level.get_ancestor(Adw.ActionRow).set_sensitive(toggle.get_active_name() == "ambient")
        self.amb_voice.set_sensitive(toggle.get_active_name() == "ambient")

    def _on_eq(self, _scale):
        self._set_later("eq", lambda: [int(s.get_value()) for s in self.eq_scales])

    def _set_later(self, key, value_fn):
        if self.syncing:
            return
        if key in self.timers:
            GLib.source_remove(self.timers[key])

        def fire():
            del self.timers[key]
            self._set(key, value_fn())
            return False
        self.timers[key] = GLib.timeout_add(250, fire)  # debounce slider drags

    def _show_alert(self, alert):
        if self.alert_dialog:
            return
        d = Adw.AlertDialog(heading="Confirmar cambio", body=alert["text"])
        if not alert["confirm_only"]:
            d.add_response("cancel", "Cancelar")
        d.add_response("ok", "Aceptar")
        d.set_response_appearance("ok", Adw.ResponseAppearance.SUGGESTED)
        d.set_default_response("ok")
        d.set_close_response("ok" if alert["confirm_only"] else "cancel")

        def done(_d, response):
            self.alert_dialog = None
            self._set("alert_response", response == "ok")
        d.connect("response", done)
        self.alert_dialog = d
        d.present(self)

    # --- incoming ---
    def _on_signal(self, proxy, sender, signal, params):
        if signal == "StateChanged":
            self._apply(json.loads(params.unpack()[0]))

    def _apply(self, s):
        self.state = s
        self.syncing = True
        try:
            connected = s.get("connected", False)
            for g in self.groups:
                g.set_sensitive(connected)
            if not connected:
                self.status_row.set_title("Desconectado")
                self.status_row.set_subtitle("Esperando auriculares…")
                return
            if "alert" in s:
                self._show_alert(s["alert"])
            self.status_row.set_title(s.get("model") or s.get("name", "Auriculares"))
            self.status_row.set_subtitle(s.get("address", ""))
            if "battery" in s:
                self.battery_row.set_subtitle(f"{s['battery']} %" + (" · cargando" if s.get("charging") else ""))
                self.battery_bar.set_value(s["battery"])
            self.codec_row.set_subtitle(s.get("codec", "—"))
            if "nc_mode" in s:
                self.nc_toggle.set_active_name(s["nc_mode"])
                ambient = s["nc_mode"] == "ambient"
                self.amb_level.get_ancestor(Adw.ActionRow).set_sensitive(ambient)
                self.amb_voice.set_sensitive(ambient)
            if "ambient_level" in s:
                self.amb_level.set_value(s["ambient_level"])
            self.amb_voice.set_active(s.get("ambient_voice", False))
            for scale, v in zip(self.eq_scales, s.get("eq", [])):
                scale.set_value(v)
            for row, key in ((self.multipoint, "multipoint"), (self.touch, "touch_panel"),
                             (self.pause, "pause_on_removal"), (self.voice, "voice_guidance")):
                if key in s:
                    row.set_active(s[key])
            if "voice_language" in s:
                self.voice.set_subtitle(f"Idioma: {s['voice_language']}")
            if "auto_power_off_options" in s:
                opts = s["auto_power_off_options"]
                if opts != self.apo.values:
                    self.apo.values = opts
                    self.apo.set_model(Gtk.StringList.new([protocol.AUTO_POWER_OFF.get(o, hex(o)) for o in opts]))
            for row, key in ((self.conn_mode, "connection_mode"), (self.nc_button, "nc_amb_button"),
                             (self.apo, "auto_power_off")):
                if s.get(key) in row.values:
                    row.set_selected(row.values.index(s[key]))
            self.fw_row.set_title(f"{s.get('model', 'Auriculares')} Versión")
            self.fw_row.set_subtitle(s.get("firmware", "—"))
        finally:
            self.syncing = False


class App(Adw.Application):
    def __init__(self):
        super().__init__(application_id="io.github.cpujp.OpenSound", flags=Gio.ApplicationFlags.DEFAULT_FLAGS)

    def do_activate(self):
        win = self.get_active_window() or Window(self)
        win.present()


def main():
    return App().run(sys.argv)


if __name__ == "__main__":
    sys.exit(main())
