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

const APP_ID: &str = "io.github.cpujp.OpenSound.App";
const BUS_NAME: &str = "io.github.cpujp.OpenSound";
const OBJ_PATH: &str = "/io/github/cpujp/OpenSound";
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
    eq: Vec<gtk::Scale>,
    switches: Vec<(adw::SwitchRow, &'static str)>,
    combos: Vec<(adw::ComboRow, &'static str, RefCell<Vec<u8>>)>,
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

    fn apply(self: &Rc<Self>, s: &State) {
        self.syncing.set(true);
        for g in &self.groups {
            g.set_sensitive(s.connected);
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
        for (scale, v) in self.eq.iter().zip(s.eq.iter().flatten()) {
            scale.set_value((*v).into());
        }
        let state = serde_json::to_value(s).unwrap_or_default();
        for (row, key) in &self.switches {
            if let Some(v) = state[*key].as_bool() {
                row.set_active(v);
            }
        }
        if let Some(lang) = &s.voice_language {
            self.voice.set_subtitle(&format!("Idioma: {lang}"));
        }
        for (row, key, values) in &self.combos {
            if *key == "auto_power_off" {
                if let Some(opts) = s.auto_power_off_options.as_ref().filter(|o| **o != *values.borrow()) {
                    let labels: Vec<String> = opts
                        .iter()
                        .map(|o| {
                            protocol::label(protocol::AUTO_POWER_OFF, *o)
                                .map_or_else(|| format!("0x{o:02x}"), str::to_owned)
                        })
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

    // Sound quality: band 0 is Clear Bass; show it after the bands
    let g = group(&page, "Calidad de sonido / volumen", Some("Ecualizador (−10 a +10)"));
    let mut eq: Vec<Option<gtk::Scale>> = vec![None; protocol::EQ_BANDS.len()];
    for i in [1, 2, 3, 4, 5, 0] {
        let band = protocol::EQ_BANDS[i];
        let title = if i == 0 { band.to_owned() } else { format!("{band} Hz") };
        eq[i] = Some(scale_row(&g, &title, -10.0, 10.0));
    }
    let eq: Vec<gtk::Scale> = eq.into_iter().flatten().collect();
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

    // Power
    let g = group(&page, "Energía / Batería", None);
    let (apo, apo_values) = combo_row(&g, "Apagado automático", protocol::AUTO_POWER_OFF);
    let power_off =
        adw::ButtonRow::builder().title("Apagar auriculares").start_icon_name("system-shutdown-symbolic").build();
    power_off.add_css_class("destructive-action");
    g.add(&power_off);
    groups.push(g);

    // System
    let g = group(&page, "Sistema", None);
    let fw = adw::ActionRow::builder().title("Versión").build();
    g.add(&fw);
    let voice = adw::SwitchRow::builder().title("Notificación y guía de voz").build();
    g.add(&voice);
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
            (voice.clone(), "voice_guidance"),
        ],
        combos: vec![
            (conn_mode, "connection_mode", conn_values),
            (nc_button, "nc_amb_button", nc_values),
            (apo, "auto_power_off", apo_values),
        ],
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
    for scale in &eq {
        let u = ui.clone();
        scale.connect_value_changed(move |_| {
            u.set_later("eq", |ui| json!(ui.eq.iter().map(|s| s.value() as i64).collect::<Vec<_>>()))
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
                u.apply(&serde_json::from_str(&json).unwrap_or_default());
            }
        }
        None
    });
    match proxy.call_sync("GetState", None, gio::DBusCallFlags::NONE, -1, None::<&gio::Cancellable>) {
        Ok(v) => {
            let json = v.get::<(String,)>().map(|(j,)| j).unwrap_or_default();
            ui.apply(&serde_json::from_str(&json).unwrap_or_default());
        }
        Err(e) => {
            ui.apply(&State::default());
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
