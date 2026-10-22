// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSound daemon: owns the single RFCOMM link and exposes it on the session bus as
//! `io.github.cpujp.OpenSound` at `/io/github/cpujp/OpenSound`.
//! The GTK app and the GNOME Shell extension are clients; the headset accepts only one control link.

mod bt;
mod link;
mod settings;

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{Map, Value};

use opensound_core::protocol::{self, State};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use link::Link;

const BUS_NAME: &str = "io.github.cpujp.OpenSound";
const OBJ_PATH: &str = "/io/github/cpujp/OpenSound";
const POLL: Duration = Duration::from_secs(5);

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.cpujp.OpenSound")]
enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    Error(String),
}

struct Daemon {
    state: Mutex<State>,
    link: Mutex<Option<Arc<Link>>>,
    jobs: Mutex<mpsc::Sender<(u8, Vec<u8>)>>,
    bus: OnceLock<Connection>,
    settings: Mutex<Map<String, Value>>,
    battery_alert: Mutex<u8>, // highest alert already shown: 0 none, 1 low, 2 critical
}

impl Daemon {
    /// Headset state plus the settings stored on this computer, as one JSON object for clients.
    fn state_json(&self) -> String {
        let mut v = serde_json::to_value(&*self.state.lock().unwrap()).unwrap_or_default();
        if let Value::Object(obj) = &mut v {
            for (k, _) in settings::KEYS {
                if let Some(s) = self.settings.lock().unwrap().get(*k).cloned().or_else(|| settings::default(k)) {
                    obj.insert((*k).to_owned(), s);
                }
            }
        }
        v.to_string()
    }

    fn emit(&self) {
        let json = self.state_json();
        if let Some(bus) = self.bus.get() {
            if let Err(e) = bus.emit_signal(None::<()>, OBJ_PATH, BUS_NAME, "StateChanged", &(json,)) {
                eprintln!("opensound: cannot emit StateChanged: {e}");
            }
        }
    }

    fn on_message(&self, data_type: u8, payload: &[u8]) {
        if std::env::var_os("OPENSOUND_DEBUG").is_some() {
            let hex: Vec<String> = payload.iter().map(|b| format!("{b:02x}")).collect();
            eprintln!("opensound: <- {data_type:02x} {}", hex.join(" "));
        }
        // putting the headset on is the moment a phone would say good morning
        if payload.starts_with(&[0xA5, 0x01, 0x00, 0x01]) {
            self.greet_once_a_day();
        }
        // emit even when unchanged: a rejected Set comes back with the old value and clients must revert
        protocol::parse(data_type, payload, &mut self.state.lock().unwrap());
        self.emit();
        self.check_battery();
    }

    /// Desktop notification when the battery gets low or critical, once per level until it charges.
    fn check_battery(&self) {
        let (level, charging) = {
            let st = self.state.lock().unwrap();
            (st.battery, st.charging.unwrap_or(false))
        };
        let Some(level) = level else { return };
        let mut sent = self.battery_alert.lock().unwrap();
        let (next, alert) = battery_alert(level, charging, *sent);
        *sent = next;
        drop(sent);
        let Some(critical) = alert else { return };
        let (summary, icon) = if critical {
            ("Batería de los auriculares crítica", "battery-level-0-symbolic")
        } else {
            ("Batería de los auriculares baja", "battery-level-10-symbolic")
        };
        let body = format!("Queda un {level} %. Conecta los auriculares para cargarlos.");
        let hints: HashMap<&str, zbus::zvariant::Value> =
            HashMap::from([("urgency", zbus::zvariant::Value::U8(if critical { 2 } else { 1 }))]);
        if let Some(bus) = self.bus.get() {
            let args = ("OpenSound", 0u32, icon, summary, body.as_str(), Vec::<&str>::new(), hints, -1i32);
            if let Err(e) = bus.call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &args,
            ) {
                eprintln!("opensound: cannot show the battery notification: {e}");
            }
        }
    }

    fn on_close(&self) {
        *self.link.lock().unwrap() = None;
        *self.state.lock().unwrap() = State::default();
        self.emit();
    }

    fn connect(self: &Arc<Self>, address: &str, name: &str) {
        let (on_msg, on_close) = (self.clone(), self.clone());
        let link = match Link::connect(address, move |dt, p| on_msg.on_message(dt, &p), move || on_close.on_close()) {
            Ok(link) => link,
            Err(e) => return eprintln!("opensound: cannot connect to {address}: {e}"),
        };
        *self.link.lock().unwrap() = Some(link.clone());
        {
            let mut st = self.state.lock().unwrap();
            st.connected = true;
            st.address = Some(address.to_owned());
            st.name = Some(name.to_owned());
        }
        self.emit();
        for (dt, payload) in protocol::INIT_QUERIES {
            if let Err(e) = link.send(*dt, payload) {
                eprintln!("opensound: link error: {e}");
                return link.close();
            }
        }
        self.greet_once_a_day();
    }

    /// The first connection or first wear of the day says hello with the date, like a phone does in the morning.
    fn greet_once_a_day(&self) {
        let today = settings::today();
        {
            let mut s = self.settings.lock().unwrap();
            if s.get("greeting") != Some(&Value::Bool(true))
                || s.get("last_greeting") == Some(&Value::from(today.as_str()))
            {
                return;
            }
            s.insert("last_greeting".into(), Value::from(today));
            settings::save(&s);
        }
        self.speak_greeting();
    }

    fn speak_greeting(&self) {
        let battery = self.state.lock().unwrap().battery;
        std::thread::spawn(move || {
            // give the audio server a moment to switch to the headset
            std::thread::sleep(Duration::from_secs(3));
            settings::speak(&settings::greeting_text(settings::now(), battery));
        });
    }

    /// Single worker: keeps the link alive and sends queued frames, so link state has one owner.
    fn run(self: Arc<Self>, jobs: mpsc::Receiver<(u8, Vec<u8>)>, system: Connection) {
        loop {
            let job = jobs.recv_timeout(POLL);
            let link = self.link.lock().unwrap().clone();
            match (job, link) {
                (Ok((dt, payload)), Some(link)) => {
                    if let Err(e) = link.send(dt, &payload) {
                        eprintln!("opensound: link error: {e}");
                        link.close();
                    }
                }
                (Err(RecvTimeoutError::Disconnected), _) => return,
                (_, None) => match find_headset(&system) {
                    Ok(Some((address, name))) => self.connect(&address, &name),
                    Ok(None) => {}
                    Err(e) => eprintln!("opensound: BlueZ query failed: {e}"),
                },
                (Err(RecvTimeoutError::Timeout), Some(_)) => {}
            }
        }
    }
}

struct Service(Arc<Daemon>);

#[zbus::interface(name = "io.github.cpujp.OpenSound")]
impl Service {
    fn get_state(&self) -> String {
        self.0.state_json()
    }

    fn set(&self, key: &str, value_json: &str) -> Result<(), Error> {
        let value: Value = serde_json::from_str(value_json).map_err(|e| Error::Error(format!("invalid JSON: {e}")))?;
        if key == "greeting_test" {
            self.0.speak_greeting();
            return Ok(());
        }
        if let Some((_, valid)) = settings::KEYS.iter().find(|(k, _)| *k == key) {
            if !valid(&value) {
                return Err(Error::Error(format!("invalid value for {key}")));
            }
            let mut s = self.0.settings.lock().unwrap();
            s.insert(key.to_owned(), value);
            settings::save(&s);
            drop(s);
            self.0.emit();
            return Ok(());
        }
        let frame = {
            let mut st = self.0.state.lock().unwrap();
            if !st.connected {
                return Err(Error::Error("headphones not connected".into()));
            }
            let frame = protocol::build(key, &value, &st).map_err(Error::Error)?;
            if key == "alert_response" {
                st.alert = None;
            }
            frame
        };
        if key == "alert_response" {
            self.0.emit();
        }
        let jobs = self.0.jobs.lock().unwrap();
        let gone = |_| Error::Error("daemon is shutting down".into());
        jobs.send(frame).map_err(gone)?;
        if key == "eq" {
            // band writes are applied silently; read them back so clients see the result
            jobs.send((opensound_core::frame::DT_MDR, vec![0x56, 0x00])).map_err(gone)?;
        }
        Ok(())
    }

    #[zbus(signal)]
    async fn state_changed(emitter: &zbus::object_server::SignalEmitter<'_>, json: &str) -> zbus::Result<()>;
}

pub const BATTERY_LOW: u8 = 20;
pub const BATTERY_CRITICAL: u8 = 10;

/// Next alert state and the alert to show now (`Some(true)` critical, `Some(false)` low).
/// Charging or going back above the low mark re-arms both alerts.
fn battery_alert(level: u8, charging: bool, sent: u8) -> (u8, Option<bool>) {
    match level {
        _ if charging || level > BATTERY_LOW => (0, None),
        l if l <= BATTERY_CRITICAL && sent < 2 => (2, Some(true)),
        _ if sent < 1 => (1, Some(false)),
        _ => (sent, None),
    }
}

type Props = HashMap<String, OwnedValue>;

/// First connected device that advertises the MDR v2 service: (address, alias).
fn pick_headset<'a>(devices: impl IntoIterator<Item = &'a Props>) -> Option<(String, String)> {
    devices.into_iter().find_map(|props| {
        let get = |k: &str| props.get(k).cloned();
        let connected = get("Connected").and_then(|v| bool::try_from(v).ok())?;
        let uuids = get("UUIDs").and_then(|v| Vec::<String>::try_from(v).ok())?;
        let address = get("Address").and_then(|v| String::try_from(v).ok())?;
        let alias = get("Alias").and_then(|v| String::try_from(v).ok()).unwrap_or_default();
        (connected && uuids.iter().any(|u| u.eq_ignore_ascii_case(bt::UUID_V2_STR))).then_some((address, alias))
    })
}

fn find_headset(system: &Connection) -> zbus::Result<Option<(String, String)>> {
    let objects: HashMap<OwnedObjectPath, HashMap<String, Props>> = system
        .call_method(Some("org.bluez"), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())?
        .body()
        .deserialize()?;
    Ok(pick_headset(objects.values().filter_map(|ifaces| ifaces.get("org.bluez.Device1"))))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel();
    let daemon = Arc::new(Daemon {
        state: Mutex::default(),
        link: Mutex::default(),
        jobs: Mutex::new(tx),
        bus: OnceLock::new(),
        settings: Mutex::new(settings::load()),
        battery_alert: Mutex::default(),
    });
    let bus = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJ_PATH, Service(daemon.clone()))?
        .build()?;
    let _ = daemon.bus.set(bus);
    let system = Connection::system()?;
    daemon.run(rx, system);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn device(connected: bool, uuids: &[&str], address: &str) -> Props {
        let mut p = Props::new();
        p.insert("Connected".into(), Value::from(connected).try_into().unwrap());
        p.insert(
            "UUIDs".into(),
            Value::from(uuids.iter().map(|s| s.to_string()).collect::<Vec<_>>()).try_into().unwrap(),
        );
        p.insert("Address".into(), Value::from(address).try_into().unwrap());
        p.insert("Alias".into(), Value::from("WH-1000XM5").try_into().unwrap());
        p
    }

    fn service(state: State) -> (Service, mpsc::Receiver<(u8, Vec<u8>)>) {
        let (tx, rx) = mpsc::channel();
        let daemon = Daemon {
            state: Mutex::new(state),
            link: Mutex::default(),
            jobs: Mutex::new(tx),
            bus: OnceLock::new(),
            settings: Mutex::default(),
            battery_alert: Mutex::default(),
        };
        (Service(Arc::new(daemon)), rx)
    }

    fn connected() -> State {
        State { connected: true, ..Default::default() }
    }

    #[test]
    fn set_queues_the_built_frame() {
        let (svc, jobs) = service(connected());
        svc.set("multipoint", "false").unwrap();
        assert_eq!(jobs.try_recv().unwrap(), (opensound_core::frame::DT_MDR, vec![0xD8, 0xD2, 0x00, 0x01]));
    }

    #[test]
    fn eq_write_is_followed_by_a_read_back() {
        let (svc, jobs) = service(State { connected: true, eq_preset: Some(0xA1), ..Default::default() });
        svc.set("eq", "[2,0,0,0,0,0]").unwrap();
        assert_eq!(jobs.try_recv().unwrap().1[..3], [0x58, 0x00, 0xA1]);
        assert_eq!(jobs.try_recv().unwrap().1, [0x56, 0x00]);
    }

    #[test]
    fn set_rejects_when_disconnected_or_invalid() {
        let (svc, jobs) = service(State::default());
        assert!(matches!(svc.set("multipoint", "true"), Err(Error::Error(m)) if m.contains("not connected")));
        let (svc, _) = service(connected());
        assert!(matches!(svc.set("multipoint", "nope"), Err(Error::Error(m)) if m.contains("invalid JSON")));
        assert!(matches!(svc.set("bogus", "1"), Err(Error::Error(_))));
        assert!(jobs.try_recv().is_err(), "nothing must be sent for rejected calls");
    }

    #[test]
    fn alert_response_clears_the_pending_alert() {
        let mut st = connected();
        protocol::parse(opensound_core::frame::DT_MDR, &[0x99, 0x00, 0x07, 0x01], &mut st);
        let (svc, jobs) = service(st);
        assert!(svc.get_state().contains("\"alert\""));
        svc.set("alert_response", "false").unwrap();
        assert_eq!(jobs.try_recv().unwrap().1, vec![0x98, 0x00, 0x07, 0x00]);
        assert!(!svc.get_state().contains("alert"));
        assert!(svc.set("alert_response", "true").is_err(), "a second answer has no alert to answer");
    }

    #[test]
    fn battery_alerts_once_per_level_and_rearm_when_charging() {
        assert_eq!(battery_alert(50, false, 0), (0, None));
        assert_eq!(battery_alert(20, false, 0), (1, Some(false)));
        assert_eq!(battery_alert(15, false, 1), (1, None));
        assert_eq!(battery_alert(10, false, 1), (2, Some(true)));
        assert_eq!(battery_alert(5, false, 2), (2, None));
        assert_eq!(battery_alert(5, true, 2), (0, None));
        assert_eq!(battery_alert(8, false, 0), (2, Some(true)), "first reading already critical");
    }

    #[test]
    fn get_state_is_valid_json() {
        let (svc, _) = service(State::default());
        let v: serde_json::Value = serde_json::from_str(&svc.get_state()).unwrap();
        assert_eq!(v["connected"], false);
        // local settings are always present, with their defaults
        assert_eq!(v["panel_icon"], "audio-headphones-symbolic");
        assert_eq!(v["widget_show_power"], true);
        assert!(v.get("greeting").is_none());
    }

    #[test]
    fn picks_only_connected_mdr_devices() {
        let mouse = device(true, &["00001124-0000-1000-8000-00805f9b34fb"], "11:11:11:11:11:11");
        let idle = device(false, &[bt::UUID_V2_STR], "22:22:22:22:22:22");
        let headset = device(
            true,
            &["0000110b-0000-1000-8000-00805f9b34fb", &bt::UUID_V2_STR.to_uppercase()],
            "33:33:33:33:33:33",
        );
        assert_eq!(pick_headset([&mouse, &idle]), None);
        assert_eq!(pick_headset([&mouse, &idle, &headset]), Some(("33:33:33:33:33:33".into(), "WH-1000XM5".into())));
    }

    #[test]
    fn ignores_devices_with_missing_properties() {
        let mut p = device(true, &[bt::UUID_V2_STR], "33:33:33:33:33:33");
        p.remove("Address");
        assert_eq!(pick_headset([&p]), None);
    }
}
