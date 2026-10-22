// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenSound daemon: owns the single RFCOMM link and exposes it on the session bus as
//! `io.github.cpujp.OpenSound` at `/io/github/cpujp/OpenSound`.
//! The GTK app and the GNOME Shell extension are clients; the headset accepts only one control link.

mod bt;
mod link;

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

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
}

impl Daemon {
    fn emit(&self) {
        let json = self.state.lock().unwrap().to_json();
        if let Some(bus) = self.bus.get() {
            if let Err(e) = bus.emit_signal(None::<()>, OBJ_PATH, BUS_NAME, "StateChanged", &(json,)) {
                eprintln!("opensound: cannot emit StateChanged: {e}");
            }
        }
    }

    fn on_message(&self, data_type: u8, payload: &[u8]) {
        // emit even when unchanged: a rejected Set comes back with the old value and clients must revert
        protocol::parse(data_type, payload, &mut self.state.lock().unwrap());
        self.emit();
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
        self.0.state.lock().unwrap().to_json()
    }

    fn set(&self, key: &str, value_json: &str) -> Result<(), Error> {
        let value = serde_json::from_str(value_json).map_err(|e| Error::Error(format!("invalid JSON: {e}")))?;
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
        self.0.jobs.lock().unwrap().send(frame).map_err(|_| Error::Error("daemon is shutting down".into()))
    }

    #[zbus(signal)]
    async fn state_changed(emitter: &zbus::object_server::SignalEmitter<'_>, json: &str) -> zbus::Result<()>;
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
        let daemon =
            Daemon { state: Mutex::new(state), link: Mutex::default(), jobs: Mutex::new(tx), bus: OnceLock::new() };
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
    fn get_state_is_valid_json() {
        let (svc, _) = service(State::default());
        assert_eq!(svc.get_state(), r#"{"connected":false}"#);
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
