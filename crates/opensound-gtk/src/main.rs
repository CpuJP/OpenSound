// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSound GTK 4 / libadwaita window. Talks to the daemon over the session D-Bus.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use opensound_core::protocol::{self, NcMode, State};
use serde_json::{json, Value};

mod find_eq;

const APP_ID: &str = "io.github.cpujp.OpenSound.App";
const BUS_NAME: &str = "io.github.cpujp.OpenSound";
const OBJ_PATH: &str = "/io/github/cpujp/OpenSound";
/// Icons the top-bar indicator can use: (icon name, label).
const PANEL_ICONS: [(&str, &str); 5] = [
    ("audio-headphones-symbolic", "Auriculares"),
    ("audio-headset-symbolic", "Auriculares con micrófono"),
    ("audio-speakers-symbolic", "Altavoces"),
    ("audio-x-generic-symbolic", "Nota musical"),
    ("bluetooth-symbolic", "Bluetooth"),
];
const NC_MODES: [(&str, &str); 3] =
    [("nc", "Supresión de ruido"), ("ambient", "Sonido ambiente"), ("off", "Desactivado")];

struct Ui {
    window: adw::ApplicationWindow,
    proxy: gio::DBusProxy,
    toast: adw::ToastOverlay,
    groups: Vec<adw::PreferencesGroup>,
    status: adw::ActionRow,
    battery: adw::ActionRow,
    battery_bar: gtk::LevelBar,
    codec: adw::ActionRow,
    nc_toggle: adw::ToggleGroup,
    amb_level: gtk::Scale,
    amb_voice: adw::SwitchRow,
    eq: Vec<adw::ComboRow>, // one drop-down per band, index 0..=20 for −10..+10
    switches: Vec<(adw::SwitchRow, &'static str)>,
    combos: Vec<(adw::ComboRow, &'static str, RefCell<Vec<u8>>)>,
    panel_icon: adw::ComboRow,
    last: RefCell<State>,
    fw: adw::ActionRow,
    voice: adw::SwitchRow,
    syncing: Cell<bool>, // true while widgets are updated from device state
    timers: RefCell<HashMap<&'static str, glib::SourceId>>,
    alert_open: Cell<bool>,
}

impl Ui {
    fn set(&self, key: &str, value: Value) {
        if self.syncing.get() {
            return;
        }
        let toast = self.toast.clone();
        self.proxy.call(
            "Set",
            Some(&(key, value.to_string()).to_variant()),
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
            move |res| {
                if let Err(mut e) = res {
                    gio::DBusError::strip_remote_error(&mut e);
                    toast.add_toast(adw::Toast::new(e.message()));
                }
            },
        );
    }

    /// Debounces slider drags so only the final value reaches the headphones.
    fn set_later(self: &Rc<Self>, key: &'static str, value: impl Fn(&Ui) -> Value + 'static) {
        if self.syncing.get() {
            return;
        }
        if let Some(id) = self.timers.borrow_mut().remove(key) {
            id.remove();
        }
        let ui = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(250), move || {
            ui.timers.borrow_mut().remove(key);
            ui.set(key, value(&ui));
        });
        self.timers.borrow_mut().insert(key, id);
    }

    fn show_alert(self: &Rc<Self>, alert: &protocol::Alert) {
        if self.alert_open.replace(true) {
            return;
        }
        let d = adw::AlertDialog::new(Some("Confirmar cambio"), Some(&alert.text));
        if !alert.confirm_only {
            d.add_response("cancel", "Cancelar");
        }
        d.add_response("ok", "Aceptar");
        d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("ok"));
        d.set_close_response(if alert.confirm_only { "ok" } else { "cancel" });
        let ui = self.clone();
        d.connect_response(None, move |_, response| {
            ui.alert_open.set(false);
            ui.set("alert_response", json!(response == "ok"));
        });
        d.present(Some(&self.window));
    }

    fn set_ambient_sensitive(&self, ambient: bool) {
        if let Some(row) = self.amb_level.ancestor(adw::ActionRow::static_type()) {
            row.set_sensitive(ambient);
        }
        self.amb_voice.set_sensitive(ambient);
    }

    fn apply_json(self: &Rc<Self>, json: &str) {
        let raw: Value = serde_json::from_str(json).unwrap_or_default();
        self.apply(&serde_json::from_value(raw.clone()).unwrap_or_default(), &raw);
    }

    fn apply(self: &Rc<Self>, s: &State, raw: &Value) {
        self.last.replace(s.clone());
        self.syncing.set(true);
        for g in &self.groups {
            g.set_sensitive(s.connected);
        }
        // local settings are present even while disconnected
        let state = raw;
        if let Some(i) = PANEL_ICONS.iter().position(|(name, _)| state["panel_icon"].as_str() == Some(*name)) {
            self.panel_icon.set_selected(i as u32);
        }
        for (row, key) in &self.switches {
            if let Some(v) = state[*key].as_bool() {
                row.set_active(v);
            }
        }
        if !s.connected {
            self.status.set_title("Desconectado");
            self.status.set_subtitle("Esperando auriculares…");
            self.syncing.set(false);
            return;
        }
        if let Some(alert) = &s.alert {
            self.show_alert(alert);
        }
        self.status.set_title(s.model.as_deref().or(s.name.as_deref()).unwrap_or("Auriculares"));
        self.status.set_subtitle(s.address.as_deref().unwrap_or(""));
        if let Some(level) = s.battery {
            let charging = if s.charging == Some(true) { " · cargando" } else { "" };
            self.battery.set_subtitle(&format!("{level} %{charging}"));
            self.battery_bar.set_value(level.into());
        }
        self.codec.set_subtitle(s.codec.as_deref().unwrap_or("—"));
        if let Some(mode) = s.nc_mode {
            let name = serde_json::to_value(mode).ok().and_then(|v| v.as_str().map(str::to_owned));
            self.nc_toggle.set_active_name(name.as_deref());
            self.set_ambient_sensitive(mode == NcMode::Ambient);
        }
        if let Some(level) = s.ambient_level {
            self.amb_level.set_value(level.into());
        }
        self.amb_voice.set_active(s.ambient_voice.unwrap_or(false));
        let custom = s.eq_preset.is_some_and(|p| protocol::CUSTOM_PRESETS.contains(&p));
        for (row, v) in self.eq.iter().zip(s.eq.iter().flatten()) {
            row.set_selected((i32::from(*v) + 10).clamp(0, 20) as u32);
        }
        for row in &self.eq {
            row.set_sensitive(custom);
        }
        if let Some(lang) = &s.voice_language {
            self.voice.set_subtitle(&format!("Idioma: {lang}"));
        }
        for (row, key, values) in &self.combos {
            // option lists the headset reports replace the built-in ones
            let reported = match *key {
                "auto_power_off" => s.auto_power_off_options.as_ref().map(|o| (o, protocol::AUTO_POWER_OFF)),
                "eq_preset" => s.eq_presets.as_ref().map(|o| (o, protocol::EQ_PRESETS)),
                _ => None,
            };
            if let Some((opts, table)) = reported {
                if *opts != *values.borrow() {
                    let labels: Vec<String> = opts
                        .iter()
                        .map(|o| protocol::label(table, *o).map_or_else(|| format!("0x{o:02x}"), str::to_owned))
                        .collect();
                    *values.borrow_mut() = opts.clone();
                    row.set_model(Some(&gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>())));
                }
            }
            if let Some(i) = state[*key].as_u64().and_then(|v| values.borrow().iter().position(|x| u64::from(*x) == v))
            {
                row.set_selected(i as u32);
            }
        }
        let model = s.model.as_deref().unwrap_or("Auriculares");
        self.fw.set_title(&format!("{model} Versión"));
        self.fw.set_subtitle(s.firmware.as_deref().unwrap_or("—"));
        self.syncing.set(false);
    }
}

fn group(page: &adw::PreferencesPage, title: &str, description: Option<&str>) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::builder().title(title).build();
    g.set_description(description);
    page.add(&g);
    g
}

fn scale_row(group: &adw::PreferencesGroup, title: &str, lo: f64, hi: f64) -> gtk::Scale {
    let row = adw::ActionRow::builder().title(title).build();
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, lo, hi, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_digits(0);
    scale.set_size_request(220, -1);
    if lo < 0.0 {
        scale.add_mark(0.0, gtk::PositionType::Bottom, None);
    }
    row.add_suffix(&scale);
    group.add(&row);
    scale
}

fn combo_row(group: &adw::PreferencesGroup, title: &str, options: &[(u8, &str)]) -> (adw::ComboRow, RefCell<Vec<u8>>) {
    let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
    let row = adw::ComboRow::builder().title(title).model(&gtk::StringList::new(&labels)).build();
    group.add(&row);
    (row, RefCell::new(options.iter().map(|(id, _)| *id).collect()))
}

fn build_ui(app: &adw::Application) {
    if let Some(win) = app.active_window() {
        return win.present();
    }
    let proxy = match gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::NONE,
        None,
        BUS_NAME,
        OBJ_PATH,
        BUS_NAME,
        None::<&gio::Cancellable>,
    ) {
        Ok(p) => p,
        Err(e) => return eprintln!("opensound: cannot reach the session bus: {e}"),
    };

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("OpenSound")
        .default_width(480)
        .default_height(820)
        .build();
    let toast = adw::ToastOverlay::new();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&toast));
    window.set_content(Some(&view));
    let page = adw::PreferencesPage::new();
    toast.set_child(Some(&page));

    // Live monitor
    let g = group(&page, "Estado", None);
    let status = adw::ActionRow::builder().title("Desconectado").subtitle("Esperando auriculares…").build();
    status.add_prefix(&gtk::Image::builder().icon_name("audio-headphones-symbolic").pixel_size(32).build());
    g.add(&status);
    let battery = adw::ActionRow::builder().title("Batería").build();
    let battery_bar =
        gtk::LevelBar::builder().min_value(0.0).max_value(100.0).valign(gtk::Align::Center).width_request(120).build();
    battery.add_suffix(&battery_bar);
    g.add(&battery);
    let codec = adw::ActionRow::builder().title("Códec").build();
    g.add(&codec);

    let mut groups = Vec::new();
    // Ambient sound control
    let g = group(&page, "Control de sonido ambiental", None);
    let nc_toggle = adw::ToggleGroup::builder().valign(gtk::Align::Center).margin_top(8).margin_bottom(8).build();
    for (name, label) in NC_MODES {
        nc_toggle.add(adw::Toggle::builder().name(name).label(label).build());
    }
    let row = adw::PreferencesRow::new();
    row.set_child(Some(&nc_toggle));
    g.add(&row);
    let amb_level = scale_row(&g, "Nivel de sonido ambiente", 1.0, f64::from(protocol::AMBIENT_LEVEL_MAX));
    let amb_voice = adw::SwitchRow::builder().title("Enfocar en la voz").build();
    g.add(&amb_voice);
    groups.push(g);

    // Speak-to-chat
    let g = group(&page, "Hablar para chatear", None);
    let stc = adw::SwitchRow::builder()
        .title("Hablar para chatear")
        .subtitle("Empieza a hablar con los auriculares puestos para iniciar una conversación")
        .build();
    g.add(&stc);
    let (stc_sens, stc_sens_values) = combo_row(&g, "Sensibilidad de detección de voz", protocol::STC_SENSITIVITY);
    let (stc_time, stc_time_values) = combo_row(&g, "Tiempo hasta que finaliza el modo", protocol::STC_TIMEOUT);
    groups.push(g);

    // Sound quality: band 0 is Clear Bass; show it after the bands
    let g = group(
        &page,
        "Calidad de sonido / volumen",
        Some("Las bandas se pueden editar en Manual, Personalizado 1 y Personalizado 2 (−10 a +10)"),
    );
    let (eq_preset, eq_preset_values) = combo_row(&g, "Ecualizador", protocol::EQ_PRESETS);
    let find_eq =
        adw::ButtonRow::builder().title("Encontrar su ecualizador").start_icon_name("audio-speakers-symbolic").build();
    g.add(&find_eq);
    // drop-downs instead of sliders: scrolling the page must never change a band
    let levels: Vec<String> = (-10..=10).map(|v: i32| if v > 0 { format!("+{v}") } else { v.to_string() }).collect();
    let levels = gtk::StringList::new(&levels.iter().map(String::as_str).collect::<Vec<_>>());
    let mut eq: Vec<Option<adw::ComboRow>> = vec![None; protocol::EQ_BANDS.len()];
    for i in [1, 2, 3, 4, 5, 0] {
        let band = protocol::EQ_BANDS[i];
        let title = if i == 0 { band.to_owned() } else { format!("{band} Hz") };
        let row = adw::ComboRow::builder().title(title).model(&levels).build();
        g.add(&row);
        eq[i] = Some(row);
    }
    let eq: Vec<adw::ComboRow> = eq.into_iter().flatten().collect();
    groups.push(g);

    // Connection
    let g = group(&page, "Conexión", None);
    let multipoint = adw::SwitchRow::builder().title("Conectar a 2 dispositivos simultáneamente").build();
    g.add(&multipoint);
    let (conn_mode, conn_values) = combo_row(&g, "Calidad de conexión Bluetooth", protocol::CONNECTION_MODE);
    groups.push(g);

    // Controls
    let g = group(&page, "Controles", None);
    let (nc_button, nc_values) = combo_row(&g, "Ajustes de operación [NC/AMB]", protocol::NC_AMB_BUTTON);
    let touch = adw::SwitchRow::builder().title("Panel de control con sensor táctil").build();
    g.add(&touch);
    let pause = adw::SwitchRow::builder().title("Se detiene al quitarse los auriculares").build();
    g.add(&pause);
    groups.push(g);

    // Button taps: the headset handles them itself and does not tell the computer
    let g = group(
        &page,
        "Acción de botón rápido",
        Some("Qué hacen los auriculares al tocar dos o tres veces el botón [NC/AMB]"),
    );
    let (tap2, tap2_values) = combo_row(&g, "Doble toque en los auriculares", protocol::BUTTON_FUNCTIONS);
    let (tap3, tap3_values) = combo_row(&g, "Triple toque en los auriculares", protocol::BUTTON_FUNCTIONS);
    groups.push(g);

    // Power
    let g = group(&page, "Energía / Batería", None);
    let (apo, apo_values) = combo_row(&g, "Apagado automático", protocol::AUTO_POWER_OFF);
    let power_off =
        adw::ButtonRow::builder().title("Apagar auriculares").start_icon_name("system-shutdown-symbolic").build();
    power_off.add_css_class("destructive-action");
    g.add(&power_off);
    groups.push(g);

    // GNOME Shell extension: what the top bar and the card show (stored by the daemon)
    let g = group(&page, "Widget de GNOME", Some("Elige qué se ve en la barra superior y en la tarjeta del widget"));
    let icon_labels: Vec<&str> = PANEL_ICONS.iter().map(|(_, l)| *l).collect();
    let panel_icon =
        adw::ComboRow::builder().title("Icono en la barra superior").model(&gtk::StringList::new(&icon_labels)).build();
    g.add(&panel_icon);
    let mut widget_switches = Vec::new();
    for (key, title) in [
        ("panel_show_icon", "Mostrar el icono"),
        ("panel_show_percent", "Mostrar el porcentaje de batería"),
        ("panel_show_battery_icon", "Mostrar el icono de batería"),
        ("widget_show_battery", "Tarjeta: batería"),
        ("widget_show_noise", "Tarjeta: control de ruido"),
        ("widget_show_speak_to_chat", "Tarjeta: hablar para chatear"),
        ("widget_show_equalizer", "Tarjeta: ecualizador"),
        ("widget_show_power", "Tarjeta: apagar auriculares"),
        ("widget_quick_settings", "Botón en Ajustes rápidos"),
    ] {
        let row = adw::SwitchRow::builder().title(title).build();
        g.add(&row);
        widget_switches.push((row, key));
    }
    // not in `groups`: these settings live on this computer and work without headphones

    // System
    let g = group(&page, "Sistema", None);
    let fw = adw::ActionRow::builder().title("Versión").build();
    g.add(&fw);
    let voice = adw::SwitchRow::builder().title("Notificación y guía de voz").build();
    g.add(&voice);
    let greeting = adw::SwitchRow::builder()
        .title("Saludo de voz al empezar el día")
        .subtitle("La primera vez que se conectan cada día dicen la fecha y la batería (en español)")
        .build();
    g.add(&greeting);
    let greeting_test =
        adw::ButtonRow::builder().title("Probar saludo").start_icon_name("audio-volume-high-symbolic").build();
    g.add(&greeting_test);
    groups.push(g);

    let ui = Rc::new(Ui {
        window: window.clone(),
        proxy: proxy.clone(),
        toast,
        groups,
        status,
        battery,
        battery_bar,
        codec,
        nc_toggle: nc_toggle.clone(),
        amb_level: amb_level.clone(),
        amb_voice: amb_voice.clone(),
        eq: eq.clone(),
        switches: vec![
            (multipoint, "multipoint"),
            (touch, "touch_panel"),
            (pause, "pause_on_removal"),
            (stc, "speak_to_chat"),
            (voice.clone(), "voice_guidance"),
            (greeting, "greeting"),
        ]
        .into_iter()
        .chain(widget_switches)
        .collect(),
        combos: vec![
            (conn_mode, "connection_mode", conn_values),
            (nc_button, "nc_amb_button", nc_values),
            (apo, "auto_power_off", apo_values),
            (stc_sens, "speak_to_chat_sensitivity", stc_sens_values),
            (stc_time, "speak_to_chat_timeout", stc_time_values),
            (eq_preset, "eq_preset", eq_preset_values),
            (tap2, "button_double", tap2_values),
            (tap3, "button_triple", tap3_values),
        ],
        last: RefCell::default(),
        panel_icon: panel_icon.clone(),
        fw,
        voice,
        syncing: Cell::new(false),
        timers: RefCell::default(),
        alert_open: Cell::new(false),
    });

    // outgoing
    let u = ui.clone();
    nc_toggle.connect_active_name_notify(move |t| {
        if let Some(name) = t.active_name() {
            u.set("nc_mode", json!(name.as_str()));
            u.set_ambient_sensitive(name == "ambient");
        }
    });
    let u = ui.clone();
    amb_level.connect_value_changed(move |_| u.set_later("ambient_level", |ui| json!(ui.amb_level.value() as i64)));
    let u = ui.clone();
    amb_voice.connect_active_notify(move |r| u.set("ambient_voice", json!(r.is_active())));
    for row in &eq {
        let u = ui.clone();
        row.connect_selected_notify(move |_| {
            u.set("eq", json!(u.eq.iter().map(|r| i64::from(r.selected()) - 10).collect::<Vec<_>>()))
        });
    }
    let u = ui.clone();
    power_off.connect_activated(move |_| {
        let d = adw::AlertDialog::new(
            Some("¿Apagar los auriculares?"),
            Some("Para volver a usarlos tendrás que encenderlos con su botón."),
        );
        d.add_response("cancel", "Cancelar");
        d.add_response("off", "Apagar");
        d.set_response_appearance("off", adw::ResponseAppearance::Destructive);
        d.set_close_response("cancel");
        let ui = u.clone();
        d.connect_response(Some("off"), move |_, _| ui.set("power_off", json!(true)));
        d.present(Some(&u.window));
    });
    let u = ui.clone();
    panel_icon.connect_selected_notify(move |r| {
        if let Some((name, _)) = PANEL_ICONS.get(r.selected() as usize) {
            u.set("panel_icon", json!(name));
        }
    });
    let u = ui.clone();
    greeting_test.connect_activated(move |_| u.set("greeting_test", json!(true)));
    let u = ui.clone();
    find_eq.connect_activated(move |_| find_eq::run(&u));
    for (row, key) in &ui.switches {
        let (u, key) = (ui.clone(), *key);
        row.connect_active_notify(move |r| u.set(key, json!(r.is_active())));
    }
    for i in 0..ui.combos.len() {
        let u = ui.clone();
        ui.combos[i].0.connect_selected_notify(move |r| {
            let (_, key, values) = &u.combos[i];
            if let Some(v) = values.borrow().get(r.selected() as usize) {
                u.set(key, json!(v));
            }
        });
    }

    // incoming
    let u = ui.clone();
    proxy.connect_local("g-signal", false, move |args| {
        let signal = args.get(2).and_then(|v| v.get::<String>().ok());
        let params = args.get(3).and_then(|v| v.get::<glib::Variant>().ok());
        if let (Some("StateChanged"), Some(params)) = (signal.as_deref(), params) {
            if let Some((json,)) = params.get::<(String,)>() {
                u.apply_json(&json);
            }
        }
        None
    });
    match proxy.call_sync("GetState", None, gio::DBusCallFlags::NONE, -1, None::<&gio::Cancellable>) {
        Ok(v) => {
            let json = v.get::<(String,)>().map(|(j,)| j).unwrap_or_default();
            ui.apply_json(&json);
        }
        Err(e) => {
            ui.apply(&State::default(), &json!({}));
            ui.status.set_subtitle(&format!("Servicio no disponible: {}", e.message()));
        }
    }
    window.present();
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}
