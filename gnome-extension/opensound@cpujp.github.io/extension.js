// SPDX-License-Identifier: GPL-3.0-or-later
// OpenSound: top-bar battery button with a control card, plus a quick settings toggle, for WH-1000XM5 headphones.
// All headset I/O goes through the io.github.cpujp.OpenSound D-Bus service (opensound-daemon).
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Shell from 'gi://Shell';
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
// Same thresholds as the daemon notifications.
const BATTERY_LOW = 20;
const BATTERY_CRITICAL = 10;

// Equalizer presets in protocol order: [id, name].
const EQ_PRESETS = [
    [0x00, 'Desactivado'], [0x10, 'Alegre'], [0x11, 'Emocionado'], [0x12, 'Suave'],
    [0x13, 'Relajado'], [0x14, 'Canto'], [0x15, 'Incremento de agudos'],
    [0x16, 'Incremento de graves'], [0x17, 'Locución'], [0xA0, 'Manual'],
    [0xA1, 'Personalizado 1'], [0xA2, 'Personalizado 2'],
];
const eqName = id => (EQ_PRESETS.find(p => p[0] === id) ?? [null, '—'])[1];

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
        this._panelIcon = new St.Icon({icon_name: ICON, style_class: 'system-status-icon'});
        this._panelBattery = new St.Icon({style_class: 'system-status-icon'});
        // low battery warning, orange; red when critical
        this._warning = new St.Icon({icon_name: 'dialog-warning-symbolic', style_class: 'system-status-icon opensound-warning'});
        box.add_child(this._warning);
        box.add_child(this._panelIcon);
        box.add_child(this._panelBattery);
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
        this._headerBattery = battery;
        this.menu.addMenuItem(header);

        const barItem = staticItem();
        this._bar = new BarLevel({style_class: 'barlevel opensound-bar', x_expand: true});
        barItem.add_child(this._bar);
        this.menu.addMenuItem(barItem);
        this._barItem = barItem;

        // Noise control: one card button per mode
        this._noiseTitle = new PopupMenu.PopupSeparatorMenuItem('Control de ruido');
        this.menu.addMenuItem(this._noiseTitle);
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
        this._modesItem = modesItem;

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
        this._voiceItem.connect('toggled', (_i, on) => !this._syncing && ext.set('ambient_voice', on));
        this.menu.addMenuItem(this._voiceItem);

        // Equalizer: "< name >" steps through the presets the headset offers, wrapping around
        this._eqTitle = new PopupMenu.PopupSeparatorMenuItem('Ecualizador');
        this.menu.addMenuItem(this._eqTitle);
        this._eqIds = EQ_PRESETS.map(p => p[0]);
        this._eqItem = staticItem();
        const eqRow = new St.BoxLayout({x_expand: true, style_class: 'opensound-eq-row'});
        const arrow = (icon, delta, label) => {
            const b = new St.Button({
                child: new St.Icon({icon_name: icon, style_class: 'popup-menu-icon'}),
                style_class: 'opensound-arrow',
                can_focus: true,
                accessible_name: label,
            });
            b.connect('clicked', () => {
                const n = this._eqIds.length;
                const i = Math.max(0, this._eqIds.indexOf(this._eqCurrent));
                ext.set('eq_preset', this._eqIds[(i + delta + n) % n]);
            });
            return b;
        };
        this._eqLabel = new St.Label({
            style_class: 'opensound-eq-name',
            x_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        eqRow.add_child(arrow('go-previous-symbolic', -1, 'Ecualizador anterior'));
        eqRow.add_child(this._eqLabel);
        eqRow.add_child(arrow('go-next-symbolic', 1, 'Ecualizador siguiente'));
        this._eqItem.add_child(eqRow);
        this.menu.addMenuItem(this._eqItem);

        // Speak-to-chat: switch plus its options, shown while enabled
        this._stcTitle = new PopupMenu.PopupSeparatorMenuItem('Hablar para chatear');
        this.menu.addMenuItem(this._stcTitle);
        this._stcItem = new PopupMenu.PopupSwitchMenuItem('Activar', false);
        this._stcItem.connect('toggled', (_i, on) => !this._syncing && ext.set('speak_to_chat', on));
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
        this._powerItem = powerItem;
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
        // what the user chose to see (settings stored by the daemon; missing means shown)
        const show = key => s[key] !== false;
        this._panelIcon.icon_name = s.panel_icon || ICON;
        this._panelIcon.visible = show('panel_show_icon');
        this._panelBattery.icon_name = batteryIcon(level ?? 0, s.charging);
        this._panelBattery.visible = known && s.panel_show_battery_icon === true;
        this._label.text = known ? `${level}%` : '';
        this._label.visible = known && show('panel_show_percent');
        const low = known && !s.charging && level <= BATTERY_LOW;
        this._warning.visible = low;
        if (low && level <= BATTERY_CRITICAL)
            this._warning.add_style_class_name('critical');
        else
            this._warning.remove_style_class_name('critical');
        // never leave an empty button in the top bar
        if (!this._panelIcon.visible && !this._panelBattery.visible && !this._label.visible)
            this._panelIcon.visible = true;
        this._headerBattery.visible = this._barItem.visible = show('widget_show_battery');
        const noise = show('widget_show_noise');
        this._noiseTitle.visible = this._modesItem.visible = noise;
        this._powerItem.visible = show('widget_show_power');
        this._title.text = s.model || s.name || 'Auriculares';
        this._subtitle.text = [s.codec, modeLabel(s.nc_mode)].filter(Boolean).join(' · ');
        this._batteryLabel.text = known ? `${level}%` : '—';
        this._batteryIcon.icon_name = batteryIcon(level ?? 0, s.charging);
        this._chargingLabel.text = s.charging ? 'Cargando' : 'Batería';
        this._bar.value = (level ?? 0) / 100;
        if (known && level <= BATTERY_LOW && !s.charging)
            this._bar.add_style_class_name('low');
        else
            this._bar.remove_style_class_name('low');

        for (const [mode, button] of Object.entries(this._modeButtons))
            button.checked = mode === s.nc_mode;
        const ambient = s.nc_mode === 'ambient';
        this._ambientItem.visible = this._voiceItem.visible = noise && ambient;
        if (!this._dragging)
            this._slider.value = (s.ambient_level ?? 0) / AMBIENT_MAX;
        // setToggleState emits "toggled" on GNOME 50; _syncing keeps that from echoing back to the headset
        this._syncing = true;
        this._voiceItem.setToggleState(!!s.ambient_voice);
        this._syncing = false;

        const stc = s.speak_to_chat !== undefined && show('widget_show_speak_to_chat');
        this._stcTitle.visible = this._stcItem.visible = stc;
        this._syncing = true;
        this._stcItem.setToggleState(!!s.speak_to_chat);
        this._syncing = false;
        this._stcSensitivity.item.visible = this._stcTimeout.item.visible = stc && !!s.speak_to_chat;
        this._stcSensitivity.update(s.speak_to_chat_sensitivity);
        this._stcTimeout.update(s.speak_to_chat_timeout);

        if (s.eq_presets?.length > 1)
            this._eqIds = s.eq_presets;
        this._eqTitle.visible = this._eqItem.visible = s.eq_preset !== undefined && show('widget_show_equalizer');
        this._eqCurrent = s.eq_preset;
        this._eqLabel.text = eqName(s.eq_preset);
        // any preset other than Off is highlighted like a selected mode
        if (s.eq_preset !== undefined && s.eq_preset !== 0)
            this._eqLabel.add_style_class_name('active');
        else
            this._eqLabel.remove_style_class_name('active');
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
        this.visible = !!s.connected && s.widget_quick_settings !== false;
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
        const app = Shell.AppSystem.get_default().lookup_app('io.github.cpujp.OpenSound.App.desktop');
        if (app)
            app.activate();
        else
            GLib.spawn_command_line_async('opensound');
        Main.panel.closeQuickSettings();
        this._button?.menu.close();
    }
}
