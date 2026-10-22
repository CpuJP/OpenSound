// SPDX-License-Identifier: GPL-3.0-or-later
// OpenSound: top-bar battery indicator + quick settings toggle for Sony headphones.
// All headset I/O goes through the io.github.cpujp.OpenSound D-Bus service (opensound-daemon).
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as QuickSettings from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE = `
<node>
  <interface name="io.github.cpujp.OpenSound">
    <method name="GetState"><arg type="s" direction="out"/></method>
    <method name="Set"><arg type="s" direction="in"/><arg type="s" direction="in"/></method>
    <signal name="StateChanged"><arg type="s"/></signal>
  </interface>
</node>`;
const Proxy = Gio.DBusProxy.makeProxyWrapper(IFACE);
const MODES = [['nc', 'Supresión de ruido'], ['ambient', 'Sonido ambiente'], ['off', 'Desactivado']];
const ICON = 'audio-headphones-symbolic';

const HeadphonesToggle = GObject.registerClass(
class HeadphonesToggle extends QuickSettings.QuickMenuToggle {
    _init(ext) {
        super._init({title: 'Auriculares', iconName: ICON, toggleMode: false});
        this._ext = ext;
        this._lastOn = 'nc';
        this.menu.setHeader(ICON, 'OpenSound');

        this._modeItems = {};
        for (const [mode, label] of MODES) {
            const item = new PopupMenu.PopupMenuItem(label);
            item.connect('activate', () => ext.set('nc_mode', mode));
            this._modeItems[mode] = item;
            this.menu.addMenuItem(item);
        }

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Nivel de sonido ambiente'));
        const sliderItem = new PopupMenu.PopupBaseMenuItem({activate: false});
        this._slider = new Slider(0);
        this._slider.x_expand = true;
        this._slider.connect('drag-begin', () => (this._dragging = true));
        this._slider.connect('drag-end', () => {
            this._dragging = false;
            ext.set('ambient_level', Math.max(1, Math.round(this._slider.value * 20)));
        });
        sliderItem.add_child(this._slider);
        this.menu.addMenuItem(sliderItem);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._info = new PopupMenu.PopupMenuItem('', {reactive: false});
        this.menu.addMenuItem(this._info);
        this.menu.addAction('Abrir OpenSound', () => ext.openApp());

        this.connect('clicked', () => {
            const mode = this._state?.nc_mode;
            ext.set('nc_mode', mode && mode !== 'off' ? 'off' : this._lastOn);
        });
    }

    update(s) {
        this._state = s;
        this.visible = !!s.connected;
        if (!s.connected)
            return;
        this.title = s.model || s.name || 'Auriculares';
        const mode = s.nc_mode;
        if (mode && mode !== 'off')
            this._lastOn = mode;
        this.checked = !!mode && mode !== 'off';
        this.subtitle = (MODES.find(m => m[0] === mode) ?? [null, ''])[1];
        for (const [m, item] of Object.entries(this._modeItems))
            item.setOrnament(m === mode ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
        if (!this._dragging)
            this._slider.value = (s.ambient_level ?? 0) / 20;
        this._slider.reactive = mode === 'ambient';
        const battery = s.battery !== undefined ? `${s.battery}%${s.charging ? ' ⚡' : ''}` : '—';
        this._info.label.text = `Batería ${battery} · Códec ${s.codec ?? '—'}`;
    }
});

const Indicator = GObject.registerClass(
class Indicator extends QuickSettings.SystemIndicator {
    _init(ext) {
        super._init();
        this._icon = this._addIndicator();
        this._icon.icon_name = ICON;
        this._label = new St.Label({y_align: Clutter.ActorAlign.CENTER, style_class: 'opensound-label'});
        this.add_child(this._label);
        this.toggle = new HeadphonesToggle(ext);
        this.quickSettingsItems.push(this.toggle);
    }

    update(s) {
        this.visible = !!s.connected;
        this._icon.visible = !!s.connected;
        this._label.text = s.battery !== undefined ? `${s.battery}%` : '';
        this.toggle.update(s);
    }
});

export default class OpenSoundExtension extends Extension {
    enable() {
        this._indicator = new Indicator(this);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);
        this._indicator.update({connected: false});

        this._proxy = new Proxy(Gio.DBus.session, 'io.github.cpujp.OpenSound', '/io/github/cpujp/OpenSound', (proxy, error) => {
            if (error || !this._proxy)
                return;
            this._signal = proxy.connectSignal('StateChanged', (_p, _s, [json]) => this._update(json));
            this._ownerId = proxy.connect('notify::g-name-owner', () => this._refresh());
            this._refresh();
        });
    }

    disable() {
        if (this._proxy) {
            if (this._signal)
                this._proxy.disconnectSignal(this._signal);
            if (this._ownerId)
                this._proxy.disconnect(this._ownerId);
        }
        this._proxy = null;
        this._signal = this._ownerId = null;
        this._indicator?.quickSettingsItems.forEach(i => i.destroy());
        this._indicator?.destroy();
        this._indicator = null;
    }

    _refresh() {
        // Calling even without an owner lets D-Bus activation start the daemon.
        this._proxy?.GetStateRemote((result, error) => {
            if (error)
                this._indicator?.update({connected: false});
            else
                this._update(result[0]);
        });
    }

    _update(json) {
        try {
            this._indicator?.update(JSON.parse(json));
        } catch (e) {
            logError(e, 'OpenSound');
        }
    }

    set(key, value) {
        this._proxy?.SetRemote(key, JSON.stringify(value), (_r, error) => {
            if (error)
                Main.notifyError('OpenSound', error.message);
        });
    }

    openApp() {
        const app = Gio.DesktopAppInfo.new('io.github.cpujp.OpenSound.App.desktop');
        if (app)
            app.launch([], null);
        else
            GLib.spawn_command_line_async('opensound');
        Main.panel.closeQuickSettings();
    }
}
