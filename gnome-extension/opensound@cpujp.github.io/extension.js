// SPDX-License-Identifier: GPL-3.0-or-later
// OpenSound: top-bar battery button with a control card, plus a quick settings toggle, for WH-1000XM5 headphones.
// All headset I/O goes through the io.github.cpujp.OpenSound D-Bus service (opensound-daemon).
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as QuickSettings from 'resource:///org/gnome/shell/ui/quickSettings.js';
import {BarLevel} from 'resource:///org/gnome/shell/ui/barLevel.js';
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
// [mode, label, short label, icon]
const MODES = [
    ['nc', 'Supresión de ruido', 'Supresión', 'audio-volume-muted-symbolic'],
    ['ambient', 'Sonido ambiente', 'Ambiente', 'audio-input-microphone-symbolic'],
    ['off', 'Desactivado', 'Desactivado', 'action-unavailable-symbolic'],
];
const ICON = 'audio-headphones-symbolic';
const AMBIENT_MAX = 20;
const VERTICAL = Clutter.Orientation.VERTICAL;

// Speak-to-chat options: [value, short label], in protocol byte order.
const STC_SENSITIVITY = [[0, 'Auto'], [1, 'Alta'], [2, 'Baja']];
const STC_TIMEOUT = [[0, '5 s'], [1, '15 s'], [2, '30 s'], [3, 'Nunca']];

const modeLabel = mode => (MODES.find(m => m[0] === mode) ?? [null, ''])[1];

function batteryIcon(level, charging) {
    if (charging && level >= 100)
        return 'battery-level-100-charged-symbolic';
    return `battery-level-${Math.round(level / 10) * 10}${charging ? '-charging' : ''}-symbolic`;
}

// Ambient level slider; sends only when the user lets go, so a drag is one write.
function ambientSlider(ext, owner) {
    const slider = new Slider(0);
    slider.x_expand = true;
    slider.connect('drag-begin', () => (owner._dragging = true));
    slider.connect('drag-end', () => {
        owner._dragging = false;
        ext.set('ambient_level', Math.max(1, Math.round(slider.value * AMBIENT_MAX)));
    });
    return slider;
}

// A titled row of small toggle chips; clicking one sends `key` with its value.
function chipRow(ext, title, key, options) {
    const item = staticItem();
    const box = new St.BoxLayout({orientation: VERTICAL, x_expand: true, style_class: 'opensound-chip-box'});
    box.add_child(new St.Label({text: title, style_class: 'opensound-subtitle'}));
    const row = new St.BoxLayout({style_class: 'opensound-chips', x_expand: true});
    const buttons = new Map();
    for (const [value, label] of options) {
        const button = new St.Button({label, style_class: 'opensound-chip', x_expand: true, can_focus: true});
        button.connect('clicked', () => ext.set(key, value));
        buttons.set(value, button);
        row.add_child(button);
    }
    box.add_child(row);
    item.add_child(box);
    return {item, update: current => buttons.forEach((b, v) => (b.checked = v === current))};
}

// Display-only row; drop the dimmed "inactive" style so it reads as content, not a disabled item.
function staticItem() {
    const item = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
    item.remove_style_class_name('popup-inactive-menu-item');
    return item;
}

const PanelButton = GObject.registerClass(
class PanelButton extends PanelMenu.Button {
    _init(ext) {
        super._init(0.5, 'OpenSound');
        const box = new St.BoxLayout({style_class: 'panel-status-menu-box'});
        box.add_child(new St.Icon({icon_name: ICON, style_class: 'system-status-icon'}));
        this._label = new St.Label({y_align: Clutter.ActorAlign.CENTER, style_class: 'opensound-panel-label'});
        box.add_child(this._label);
        this.add_child(box);
        this.menu.box.add_style_class_name('opensound-menu');

        // Header: device, codec and mode, battery
        const header = staticItem();
        header.add_style_class_name('opensound-header');
        header.add_child(new St.Icon({icon_name: ICON, style_class: 'opensound-avatar', y_align: Clutter.ActorAlign.CENTER}));
        const text = new St.BoxLayout({orientation: VERTICAL, x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        this._title = new St.Label({style_class: 'opensound-title'});
        this._subtitle = new St.Label({style_class: 'opensound-subtitle'});
        text.add_child(this._title);
        text.add_child(this._subtitle);
        header.add_child(text);
        const battery = new St.BoxLayout({orientation: VERTICAL, y_align: Clutter.ActorAlign.CENTER});
        this._batteryLabel = new St.Label({style_class: 'opensound-battery', x_align: Clutter.ActorAlign.END});
        this._batteryState = new St.BoxLayout({style_class: 'opensound-battery-state', x_align: Clutter.ActorAlign.END});
        this._batteryIcon = new St.Icon({style_class: 'opensound-battery-icon'});
        this._chargingLabel = new St.Label({style_class: 'opensound-subtitle', y_align: Clutter.ActorAlign.CENTER});
        this._batteryState.add_child(this._batteryIcon);
        this._batteryState.add_child(this._chargingLabel);
        battery.add_child(this._batteryLabel);
        battery.add_child(this._batteryState);
        header.add_child(battery);
        this.menu.addMenuItem(header);

        const barItem = staticItem();
        this._bar = new BarLevel({style_class: 'barlevel opensound-bar', x_expand: true});
        barItem.add_child(this._bar);
        this.menu.addMenuItem(barItem);

        // Noise control: one card button per mode
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Control de ruido'));
        const modesItem = staticItem();
        const row = new St.BoxLayout({style_class: 'opensound-modes', x_expand: true});
        this._modeButtons = {};
        for (const [mode, , short, icon] of MODES) {
            const content = new St.BoxLayout({orientation: VERTICAL, style_class: 'opensound-mode-box'});
            content.add_child(new St.Icon({icon_name: icon, style_class: 'opensound-mode-icon'}));
            content.add_child(new St.Label({text: short, x_align: Clutter.ActorAlign.CENTER}));
            const button = new St.Button({child: content, style_class: 'opensound-mode', x_expand: true, can_focus: true});
            button.connect('clicked', () => ext.set('nc_mode', mode));
            this._modeButtons[mode] = button;
            row.add_child(button);
        }
        modesItem.add_child(row);
        this.menu.addMenuItem(modesItem);

        // Ambient options, shown only in ambient mode
        this._ambientItem = new PopupMenu.PopupBaseMenuItem({activate: false});
        this._ambientItem.add_child(new St.Icon({icon_name: 'audio-input-microphone-symbolic', style_class: 'popup-menu-icon'}));
        this._slider = ambientSlider(ext, this);
        this._ambientItem.add_child(this._slider);
        this._levelLabel = new St.Label({style_class: 'opensound-level', y_align: Clutter.ActorAlign.CENTER});
        this._ambientItem.add_child(this._levelLabel);
        this._slider.connect('notify::value', () =>
            (this._levelLabel.text = `${Math.round(this._slider.value * AMBIENT_MAX)}`));
        this.menu.addMenuItem(this._ambientItem);
        this._voiceItem = new PopupMenu.PopupSwitchMenuItem('Voz enfocada', false);
        this._voiceItem.connect('toggled', (_i, on) => ext.set('ambient_voice', on));
        this.menu.addMenuItem(this._voiceItem);

        // Speak-to-chat: switch plus its options, shown while enabled
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem('Hablar para chatear'));
        this._stcItem = new PopupMenu.PopupSwitchMenuItem('Activar', false);
        this._stcItem.connect('toggled', (_i, on) => ext.set('speak_to_chat', on));
        this.menu.addMenuItem(this._stcItem);
        this._stcSensitivity = chipRow(ext, 'Sensibilidad de detección de voz', 'speak_to_chat_sensitivity', STC_SENSITIVITY);
        this._stcTimeout = chipRow(ext, 'Tiempo hasta que finaliza', 'speak_to_chat_timeout', STC_TIMEOUT);
        this.menu.addMenuItem(this._stcSensitivity.item);
        this.menu.addMenuItem(this._stcTimeout.item);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addAction('Abrir OpenSound', () => ext.openApp(), 'emblem-system-symbolic');

        // Power off asks for a second click instead of a dialog, so the menu stays open.
        const powerItem = staticItem();
        const powerBox = new St.BoxLayout({style_class: 'opensound-power-box'});
        powerBox.add_child(new St.Icon({icon_name: 'system-shutdown-symbolic', style_class: 'popup-menu-icon'}));
        this._powerLabel = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        powerBox.add_child(this._powerLabel);
        this._power = new St.Button({child: powerBox, style_class: 'opensound-power', x_expand: true, can_focus: true});
        this._power.connect('clicked', () => {
            if (this._armed) {
                this._disarm();
                ext.set('power_off', true);
                this.menu.close();
            } else {
                this._arm();
            }
        });
        powerItem.add_child(this._power);
        this.menu.addMenuItem(powerItem);
        this.menu.connect('open-state-changed', (_m, open) => !open && this._disarm());
        this.connect('destroy', () => {
            if (this._armTimeout)
                GLib.source_remove(this._armTimeout);
        });
        this._disarm();
    }

    _arm() {
        this._armed = true;
        this._power.checked = true;
        this._powerLabel.text = 'Pulsa otra vez para apagar';
        this._armTimeout = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 4, () => {
            this._armTimeout = 0;
            this._disarm();
            return GLib.SOURCE_REMOVE;
        });
    }

    _disarm() {
        if (this._armTimeout)
            GLib.source_remove(this._armTimeout);
        this._armTimeout = 0;
        this._armed = false;
        this._power.checked = false;
        this._powerLabel.text = 'Apagar auriculares';
    }

    update(s) {
        this.visible = !!s.connected;
        if (!s.connected) {
            this.menu.close();
            return;
        }
        const level = s.battery;
        const known = level !== undefined;
        this._label.text = known ? `${level}%` : '';
        this._title.text = s.model || s.name || 'Auriculares';
        this._subtitle.text = [s.codec, modeLabel(s.nc_mode)].filter(Boolean).join(' · ');
        this._batteryLabel.text = known ? `${level}%` : '—';
        this._batteryIcon.icon_name = batteryIcon(level ?? 0, s.charging);
        this._chargingLabel.text = s.charging ? 'Cargando' : 'Batería';
        this._bar.value = (level ?? 0) / 100;
        if (known && level <= 20 && !s.charging)
            this._bar.add_style_class_name('low');
        else
            this._bar.remove_style_class_name('low');

        for (const [mode, button] of Object.entries(this._modeButtons))
            button.checked = mode === s.nc_mode;
        const ambient = s.nc_mode === 'ambient';
        this._ambientItem.visible = ambient;
        this._voiceItem.visible = ambient;
        if (!this._dragging)
            this._slider.value = (s.ambient_level ?? 0) / AMBIENT_MAX;
        this._voiceItem.setToggleState(!!s.ambient_voice);

        const stc = s.speak_to_chat !== undefined;
        this._stcItem.visible = stc;
        this._stcItem.setToggleState(!!s.speak_to_chat);
        this._stcSensitivity.item.visible = this._stcTimeout.item.visible = !!s.speak_to_chat;
        this._stcSensitivity.update(s.speak_to_chat_sensitivity);
        this._stcTimeout.update(s.speak_to_chat_timeout);
    }
});

const HeadphonesToggle = GObject.registerClass(
class HeadphonesToggle extends QuickSettings.QuickMenuToggle {
    _init(ext) {
        super._init({title: 'Auriculares', iconName: ICON, toggleMode: false});
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
        this._slider = ambientSlider(ext, this);
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
        this.subtitle = modeLabel(mode);
        for (const [m, item] of Object.entries(this._modeItems))
            item.setOrnament(m === mode ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
        if (!this._dragging)
            this._slider.value = (s.ambient_level ?? 0) / AMBIENT_MAX;
        this._slider.reactive = mode === 'ambient';
        const battery = s.battery !== undefined ? `${s.battery}%${s.charging ? ' ⚡' : ''}` : '—';
        this._info.label.text = `Batería ${battery} · Códec ${s.codec ?? '—'}`;
    }
});

// Holds the quick settings toggle; the top-bar icon lives in PanelButton.
const Indicator = GObject.registerClass(
class Indicator extends QuickSettings.SystemIndicator {
    _init(ext) {
        super._init();
        this.visible = false;
        this.toggle = new HeadphonesToggle(ext);
        this.quickSettingsItems.push(this.toggle);
    }
});

export default class OpenSoundExtension extends Extension {
    enable() {
        this._indicator = new Indicator(this);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);
        this._button = new PanelButton(this);
        Main.panel.addToStatusArea(this.uuid, this._button, 0, 'right');
        this._show({connected: false});

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
        this._button?.destroy();
        this._button = null;
        this._indicator?.quickSettingsItems.forEach(i => i.destroy());
        this._indicator?.destroy();
        this._indicator = null;
    }

    _show(s) {
        this._button?.update(s);
        this._indicator?.toggle.update(s);
    }

    _refresh() {
        // Calling even without an owner lets D-Bus activation start the daemon.
        this._proxy?.GetStateRemote((result, error) => {
            if (error)
                this._show({connected: false});
            else
                this._update(result[0]);
        });
    }

    _update(json) {
        try {
            this._show(JSON.parse(json));
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
        this._button?.menu.close();
    }
}
