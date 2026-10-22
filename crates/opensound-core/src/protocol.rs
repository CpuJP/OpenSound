// SPDX-License-Identifier: GPL-3.0-or-later
//! WH-1000XM5 MDR v2 commands: request builders and reply parser.
//! Byte layouts are documented in docs/PROTOCOL.md and were confirmed on firmware 2.5.1.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::frame::{DT_MDR, DT_MDR2};

pub const CODECS: &[(u8, &str)] =
    &[(0x01, "SBC"), (0x02, "AAC"), (0x10, "LDAC"), (0x20, "aptX"), (0x21, "aptX HD"), (0x30, "LC3")];
/// Auto power off options (type 0x05).
pub const AUTO_POWER_OFF: &[(u8, &str)] = &[
    (0x00, "5 min"),
    (0x01, "30 min"),
    (0x02, "60 min"),
    (0x03, "180 min"),
    (0x04, "15 min"),
    (0x10, "Al quitarse los auriculares"),
    (0x11, "No apagar"),
];
/// Functions the [NC/AMB] button can cycle through (type 0x30).
pub const NC_AMB_BUTTON: &[(u8, &str)] = &[
    (0x01, "NC / Ambiente / Desactivado"),
    (0x02, "NC / Ambiente"),
    (0x03, "NC / Desactivado"),
    (0x04, "Ambiente / Desactivado"),
];
pub const CONNECTION_MODE: &[(u8, &str)] =
    &[(0x00, "Prioridad a la calidad de sonido"), (0x01, "Prioridad a la conexión estable")];
pub const LANGUAGES: &[(u8, &str)] = &[
    (0, "Indefinido"),
    (1, "Inglés"),
    (2, "Francés"),
    (3, "Alemán"),
    (4, "Español"),
    (5, "Italiano"),
    (6, "Portugués"),
    (7, "Neerlandés"),
    (8, "Sueco"),
    (9, "Finlandés"),
    (10, "Ruso"),
    (11, "Japonés"),
    (13, "Portugués (Brasil)"),
    (15, "Coreano"),
    (16, "Turco"),
    (0xF0, "Chino"),
];
pub const EQ_BANDS: [&str; 6] = ["Clear Bass", "400", "1k", "2.5k", "6.3k", "16k"];
/// Editable equalizer presets: custom and user 1..5.
pub const CUSTOM_PRESETS: std::ops::RangeInclusive<u8> = 0xA0..=0xA5;
pub const AMBIENT_LEVEL_MAX: u8 = 20;
/// Alert message IDs: confirmations the headset asks for before disruptive changes.
pub const ALERTS: &[(u8, &str)] = &[
    (0x00, "Cambiar la calidad de conexión desconectará y reconectará los auriculares."),
    (0x06, "Al cambiar la conexión a 2 dispositivos, los auriculares se reconectarán y LDAC no estará disponible."),
    (0x07, "Al cambiar la conexión a 2 dispositivos, los auriculares se desconectarán y volverán a conectarse."),
    (0x0A, "Si desactivas el panel táctil, no podrás usarlo para la música, las llamadas ni el asistente de voz."),
    (
        0x0B,
        "Si desactivas el panel táctil, no podrás usarlo para la música, las llamadas ni el asistente de voz. \
         Los auriculares se reconectarán.",
    ),
    (0x0C, "Se activará el panel táctil."),
    (0x0D, "Se activará el panel táctil y los auriculares se reconectarán."),
    (0x1C, "Con prioridad a la calidad de sonido, la conexión puede ser menos estable."),
    (0x70, "Con 2 dispositivos conectados y LDAC, el sonido puede cortarse."),
    (0x71, "Prioridad a la calidad de sonido con 2 dispositivos conectados puede provocar cortes."),
];

/// (data type, payload) queried right after connecting.
pub const INIT_QUERIES: &[(u8, &[u8])] = &[
    (DT_MDR, &[0x00, 0x00]),       // protocol info (handshake)
    (DT_MDR, &[0x94, 0x00, 0x00]), // enable alerts; without it the headset rejects changes that need confirmation
    (DT_MDR, &[0x04, 0x01]),       // model name
    (DT_MDR, &[0x04, 0x02]),       // firmware version
    (DT_MDR, &[0x22, 0x00]),       // battery
    (DT_MDR, &[0x12, 0x02]),       // codec
    (DT_MDR, &[0x66, 0x17]),       // NC / ambient sound
    (DT_MDR, &[0x66, 0x30]),       // NC/AMB button
    (DT_MDR, &[0x56, 0x00]),       // equalizer
    (DT_MDR, &[0xE6, 0x00]),       // connection quality
    (DT_MDR, &[0xD6, 0xD1]),       // touch panel (general setting 1)
    (DT_MDR, &[0xD6, 0xD2]),       // multipoint (general setting 2)
    (DT_MDR, &[0xF6, 0x01]),       // pause when headphones are removed
    (DT_MDR, &[0x20, 0x05]),       // auto power off capability
    (DT_MDR, &[0x26, 0x05]),       // auto power off
    (DT_MDR2, &[0x46, 0x01]),      // voice guidance
];

pub fn label(table: &[(u8, &'static str)], id: u8) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NcMode {
    Nc,
    Ambient,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    pub id: u8,
    pub confirm_only: bool,
    pub text: String,
}

/// Headset state shared over D-Bus as JSON. Unknown values are omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nc_mode: Option<NcMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_voice: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nc_amb_button: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_preset: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<Vec<i8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_mode: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touch_panel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multipoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_on_removal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_power_off_options: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_power_off: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_guidance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert: Option<Alert>,
}

impl State {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("state is always serializable")
    }
}

/// Protocol booleans: ON = 0, OFF = 1.
fn on(b: u8) -> bool {
    b == 0
}

fn flag(v: bool) -> u8 {
    if v {
        0
    } else {
        1
    }
}

/// Updates `state` from one incoming payload. Returns true when something changed.
/// Truncated or unknown payloads are ignored.
pub fn parse(data_type: u8, p: &[u8], state: &mut State) -> bool {
    let before = state.clone();
    match (data_type, p) {
        (DT_MDR, [0x05, kind @ (1 | 2), len, rest @ ..]) => {
            if let Some(text) = rest.get(..*len as usize) {
                let text = String::from_utf8_lossy(text).into_owned();
                if *kind == 1 {
                    state.model = Some(text);
                } else {
                    state.firmware = Some(text);
                }
            }
        }
        (DT_MDR, [0x23 | 0x25, 0x00, level, charging, ..]) => {
            state.battery = Some(*level);
            state.charging = Some(*charging == 1);
        }
        (DT_MDR, [0x13 | 0x15, 0x02, codec, ..]) => {
            state.codec = Some(label(CODECS, *codec).map_or_else(|| format!("0x{codec:02x}"), str::to_owned));
        }
        (DT_MDR, [0x67 | 0x69, 0x17, _, active, mode, voice, level, ..]) => {
            state.nc_mode = Some(match (*active, *mode) {
                (0, _) => NcMode::Off,
                (_, 1) => NcMode::Ambient,
                _ => NcMode::Nc,
            });
            state.ambient_voice = Some(*voice == 1);
            state.ambient_level = Some(*level);
        }
        (DT_MDR, [0x67 | 0x69, 0x30, function, ..]) => state.nc_amb_button = Some(*function),
        (DT_MDR, [0x57 | 0x59, 0x00, preset, count, bands @ ..]) => {
            state.eq_preset = Some(*preset);
            state.eq = Some(bands.iter().take(*count as usize).map(|&b| b as i8 - 10).collect());
        }
        (DT_MDR, [0xE7 | 0xE9, 0x00, mode, ..]) => state.connection_mode = Some(*mode),
        (DT_MDR, [0xD7 | 0xD9, 0xD1, _, v, ..]) => state.touch_panel = Some(on(*v)),
        (DT_MDR, [0xD7 | 0xD9, 0xD2, _, v, ..]) => state.multipoint = Some(on(*v)),
        (DT_MDR, [0xF7 | 0xF9, 0x01, v, ..]) => state.pause_on_removal = Some(on(*v)),
        (DT_MDR, [0x21, 0x05, count, options @ ..]) => {
            state.auto_power_off_options = Some(options.iter().take(*count as usize).copied().collect());
        }
        (DT_MDR, [0x27 | 0x29, 0x05, active, ..]) => state.auto_power_off = Some(*active),
        (DT_MDR, [0x99, 0x00, id, kind, ..]) => {
            let text = label(ALERTS, *id)
                .map_or_else(|| format!("Los auriculares piden confirmación (código {id})."), str::to_owned);
            state.alert = Some(Alert { id: *id, confirm_only: *kind == 0, text });
        }
        (DT_MDR2, [0x47 | 0x49, 0x01, v, rest @ ..]) => {
            state.voice_guidance = Some(on(*v));
            if let Some(lang) = rest.first() {
                state.voice_language = Some(label(LANGUAGES, *lang).map_or_else(|| lang.to_string(), str::to_owned));
            }
        }
        _ => {}
    }
    *state != before
}

fn as_bool(v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| "expected a boolean".to_owned())
}

fn as_u8(v: &Value) -> Result<u8, String> {
    v.as_u64().and_then(|n| u8::try_from(n).ok()).ok_or_else(|| "expected an integer 0..255".to_owned())
}

fn one_of(v: &Value, table: &[(u8, &'static str)], key: &str) -> Result<u8, String> {
    let id = as_u8(v)?;
    label(table, id).map(|_| id).ok_or_else(|| format!("unknown {key} value"))
}

/// Returns the (data type, payload) that sets `key` to `value`.
pub fn build(key: &str, value: &Value, state: &State) -> Result<(u8, Vec<u8>), String> {
    let frame = match key {
        "nc_mode" | "ambient_level" | "ambient_voice" => {
            let mode = if key == "nc_mode" {
                serde_json::from_value(value.clone()).map_err(|_| "nc_mode must be nc/ambient/off".to_owned())?
            } else {
                state.nc_mode.unwrap_or(NcMode::Ambient)
            };
            let level =
                if key == "ambient_level" { as_u8(value)? } else { state.ambient_level.unwrap_or(AMBIENT_LEVEL_MAX) };
            let voice = if key == "ambient_voice" { as_bool(value)? } else { state.ambient_voice.unwrap_or(false) };
            if level > AMBIENT_LEVEL_MAX {
                return Err(format!("ambient_level must be 0..{AMBIENT_LEVEL_MAX}"));
            }
            (
                DT_MDR,
                vec![
                    0x68,
                    0x17,
                    0x01,
                    (mode != NcMode::Off) as u8,
                    (mode == NcMode::Ambient) as u8,
                    voice as u8,
                    level,
                ],
            )
        }
        "eq" => {
            let err = || "eq needs 6 integers (Clear Bass, 400, 1k, 2.5k, 6.3k, 16k)".to_owned();
            let bands = value
                .as_array()
                .filter(|a| a.len() == EQ_BANDS.len())
                .ok_or_else(err)?
                .iter()
                .map(|v| v.as_i64().ok_or_else(err))
                .collect::<Result<Vec<_>, _>>()?;
            if bands.iter().any(|v| !(-10..=10).contains(v)) {
                return Err("eq values must be in -10..10".to_owned());
            }
            let preset = state.eq_preset.filter(|p| CUSTOM_PRESETS.contains(p)).unwrap_or(0xA0);
            let mut p = vec![0x58, 0x00, preset, bands.len() as u8];
            p.extend(bands.iter().map(|v| (v + 10) as u8));
            (DT_MDR, p)
        }
        "nc_amb_button" => (DT_MDR, vec![0x68, 0x30, one_of(value, NC_AMB_BUTTON, key)?]),
        "connection_mode" => (DT_MDR, vec![0xE8, 0x00, one_of(value, CONNECTION_MODE, key)?]),
        "touch_panel" => (DT_MDR, vec![0xD8, 0xD1, 0x00, flag(as_bool(value)?)]),
        "multipoint" => (DT_MDR, vec![0xD8, 0xD2, 0x00, flag(as_bool(value)?)]),
        "pause_on_removal" => (DT_MDR, vec![0xF8, 0x01, flag(as_bool(value)?)]),
        // second byte is a remembered timer; the WH-1000XM5 has none, so send 0x00
        "auto_power_off" => (DT_MDR, vec![0x28, 0x05, one_of(value, AUTO_POWER_OFF, key)?, 0x00]),
        "alert_response" => {
            let alert = state.alert.as_ref().ok_or_else(|| "no pending alert".to_owned())?;
            let yes = as_bool(value)? || alert.confirm_only;
            (DT_MDR, vec![0x98, 0x00, alert.id, yes as u8])
        }
        "voice_guidance" => (DT_MDR2, vec![0x48, 0x01, flag(as_bool(value)?)]),
        _ => return Err(format!("unknown setting {key:?}")),
    };
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn parsed(dt: u8, s: &str) -> State {
        let mut st = State::default();
        parse(dt, &hex(s), &mut st);
        st
    }

    // --- payloads captured from a WH-1000XM5, firmware 2.5.1 ---

    #[test]
    fn device_info() {
        assert_eq!(parsed(DT_MDR, "05 01 0a 57 48 2d 31 30 30 30 58 4d 35").model.as_deref(), Some("WH-1000XM5"));
        assert_eq!(parsed(DT_MDR, "05 02 05 32 2e 35 2e 31").firmware.as_deref(), Some("2.5.1"));
    }

    #[test]
    fn battery_reply_and_notification() {
        let st = parsed(DT_MDR, "23 00 3a 00");
        assert_eq!((st.battery, st.charging), (Some(58), Some(false)));
        let st = parsed(DT_MDR, "25 00 64 01");
        assert_eq!((st.battery, st.charging), (Some(100), Some(true)));
    }

    #[test]
    fn codecs() {
        assert_eq!(parsed(DT_MDR, "13 02 10").codec.as_deref(), Some("LDAC"));
        assert_eq!(parsed(DT_MDR, "15 02 02").codec.as_deref(), Some("AAC"));
        assert_eq!(parsed(DT_MDR, "13 02 7f").codec.as_deref(), Some("0x7f"));
    }

    #[test]
    fn ncasm_modes() {
        let st = parsed(DT_MDR, "67 17 01 01 00 00 14");
        assert_eq!((st.nc_mode, st.ambient_voice, st.ambient_level), (Some(NcMode::Nc), Some(false), Some(20)));
        assert_eq!(parsed(DT_MDR, "69 17 01 01 01 00 0a").nc_mode, Some(NcMode::Ambient));
        assert_eq!(parsed(DT_MDR, "69 17 01 00 00 00 0a").nc_mode, Some(NcMode::Off));
        assert_eq!(parsed(DT_MDR, "69 17 01 01 01 01 05").ambient_voice, Some(true));
    }

    #[test]
    fn equalizer() {
        let st = parsed(DT_MDR, "57 00 a2 06 10 0a 0a 0b 0c 0d");
        assert_eq!(st.eq, Some(vec![6, 0, 0, 1, 2, 3]));
        assert_eq!(st.eq_preset, Some(0xA2));
        assert_eq!(parsed(DT_MDR, "59 00 a0 06 00 14 0a 0a 0a 0a").eq, Some(vec![-10, 10, 0, 0, 0, 0]));
    }

    #[test]
    fn equalizer_count_larger_than_payload_is_clamped() {
        assert_eq!(parsed(DT_MDR, "57 00 a0 06 0a 0a").eq, Some(vec![0, 0]));
    }

    #[test]
    fn settings() {
        let mut st = State::default();
        for p in ["d7 d1 00 00", "d7 d2 00 01", "f7 01 00", "27 05 10 00", "21 05 02 10 11", "e7 00 00", "67 30 02"] {
            parse(DT_MDR, &hex(p), &mut st);
        }
        parse(DT_MDR2, &hex("47 01 00 04"), &mut st);
        assert_eq!(st.touch_panel, Some(true));
        assert_eq!(st.multipoint, Some(false));
        assert_eq!(st.pause_on_removal, Some(true));
        assert_eq!(st.auto_power_off, Some(0x10));
        assert_eq!(st.auto_power_off_options, Some(vec![0x10, 0x11]));
        assert_eq!(st.connection_mode, Some(0));
        assert_eq!(st.nc_amb_button, Some(2));
        assert_eq!(st.voice_guidance, Some(true));
        assert_eq!(st.voice_language.as_deref(), Some("Español"));
    }

    #[test]
    fn notifications_update_existing_state() {
        let mut st = parsed(DT_MDR, "d7 d2 00 00");
        assert!(parse(DT_MDR, &hex("d9 d2 00 01"), &mut st));
        assert_eq!(st.multipoint, Some(false));
        assert!(!parse(DT_MDR, &hex("d9 d2 00 01"), &mut st), "same value reports no change");
    }

    #[test]
    fn voice_guidance_without_language_and_unknown_language() {
        let st = parsed(DT_MDR2, "49 01 01");
        assert_eq!((st.voice_guidance, st.voice_language), (Some(false), None));
        assert_eq!(parsed(DT_MDR2, "47 01 00 63").voice_language.as_deref(), Some("99"));
    }

    #[test]
    fn table_mismatch_is_ignored() {
        // a table 2 payload must not be parsed as table 1 and vice versa
        assert_eq!(parsed(DT_MDR, "47 01 00 04"), State::default());
        assert_eq!(parsed(DT_MDR2, "23 00 3a 00"), State::default());
    }

    #[test]
    fn device_noise_is_ignored() {
        for p in ["c9 01 00 0a 6f 70 53 54 43 53 74 61 72 74 00", "a5 01 00 02 00", "01 00 03 00 20 16 00 00"] {
            assert_eq!(parsed(DT_MDR, p), State::default(), "{p}");
        }
    }

    #[test]
    fn truncated_payloads_never_panic() {
        // every command byte with every short length, for both tables
        for dt in [DT_MDR, DT_MDR2] {
            for cmd in 0..=255u8 {
                for kind in [0x00, 0x01, 0x02, 0x05, 0x17, 0x30, 0xD1, 0xD2] {
                    for extra in 0..8 {
                        let mut p = vec![cmd, kind];
                        p.extend(std::iter::repeat_n(0xFF, extra));
                        parse(dt, &p, &mut State::default());
                        parse(dt, &p[..1], &mut State::default());
                    }
                }
            }
        }
    }

    // --- alerts ---

    #[test]
    fn alert_roundtrip() {
        let st = parsed(DT_MDR, "99 00 07 01");
        let alert = st.alert.clone().unwrap();
        assert_eq!((alert.id, alert.confirm_only), (7, false));
        assert!(alert.text.contains("2 dispositivos"));
        assert_eq!(build("alert_response", &json!(false), &st).unwrap(), (DT_MDR, hex("98 00 07 00")));
        assert_eq!(build("alert_response", &json!(true), &st).unwrap(), (DT_MDR, hex("98 00 07 01")));
    }

    #[test]
    fn confirm_only_alert_is_always_positive() {
        let st = parsed(DT_MDR, "99 00 0a 00");
        assert_eq!(build("alert_response", &json!(false), &st).unwrap().1, hex("98 00 0a 01"));
    }

    #[test]
    fn unknown_alert_gets_generic_text() {
        assert!(parsed(DT_MDR, "99 00 55 01").alert.unwrap().text.contains("85"));
    }

    #[test]
    fn alert_response_without_alert_fails() {
        assert!(build("alert_response", &json!(true), &State::default()).is_err());
    }

    // --- builders ---

    #[test]
    fn build_ncasm() {
        let mut st = State {
            nc_mode: Some(NcMode::Nc),
            ambient_level: Some(20),
            ambient_voice: Some(false),
            ..Default::default()
        };
        assert_eq!(build("nc_mode", &json!("ambient"), &st).unwrap(), (DT_MDR, hex("68 17 01 01 01 00 14")));
        assert_eq!(build("nc_mode", &json!("off"), &st).unwrap().1, hex("68 17 01 00 00 00 14"));
        assert_eq!(build("nc_mode", &json!("nc"), &st).unwrap().1, hex("68 17 01 01 00 00 14"));
        st.nc_mode = Some(NcMode::Ambient);
        assert_eq!(build("ambient_level", &json!(5), &st).unwrap().1, hex("68 17 01 01 01 00 05"));
        assert_eq!(build("ambient_voice", &json!(true), &st).unwrap().1, hex("68 17 01 01 01 01 14"));
    }

    #[test]
    fn build_ncasm_defaults_when_state_unknown() {
        assert_eq!(build("ambient_level", &json!(3), &State::default()).unwrap().1, hex("68 17 01 01 01 00 03"));
    }

    #[test]
    fn build_ncasm_rejects_bad_input() {
        let st = State::default();
        assert!(build("nc_mode", &json!("loud"), &st).is_err());
        assert!(build("nc_mode", &json!(1), &st).is_err());
        assert!(build("ambient_level", &json!(21), &st).is_err());
        assert!(build("ambient_level", &json!(-1), &st).is_err());
        assert!(build("ambient_voice", &json!("yes"), &st).is_err());
    }

    #[test]
    fn build_eq() {
        let st = State { eq_preset: Some(0xA2), ..Default::default() };
        assert_eq!(
            build("eq", &json!([6, 0, 0, 1, 2, 3]), &st).unwrap(),
            (DT_MDR, hex("58 00 a2 06 10 0a 0a 0b 0c 0d"))
        );
        let rock = State { eq_preset: Some(0x01), ..Default::default() };
        assert_eq!(
            build("eq", &json!([0, 0, 0, 0, 0, 0]), &rock).unwrap().1[2],
            0xA0,
            "non-editable preset switches to custom"
        );
        assert_eq!(
            build("eq", &json!([-10, 10, 0, 0, 0, 0]), &State::default()).unwrap().1,
            hex("58 00 a0 06 00 14 0a 0a 0a 0a")
        );
    }

    #[test]
    fn build_eq_rejects_bad_input() {
        let st = State::default();
        for bad in [
            json!([11, 0, 0, 0, 0, 0]),
            json!([0, 0, 0, 0, 0]),
            json!([0, 0, 0, 0, 0, "x"]),
            json!("flat"),
            json!([0.5, 0, 0, 0, 0, 0]),
        ] {
            assert!(build("eq", &bad, &st).is_err(), "{bad}");
        }
    }

    #[test]
    fn build_settings() {
        let st = State::default();
        assert_eq!(build("multipoint", &json!(true), &st).unwrap().1, hex("d8 d2 00 00"));
        assert_eq!(build("multipoint", &json!(false), &st).unwrap().1, hex("d8 d2 00 01"));
        assert_eq!(build("touch_panel", &json!(false), &st).unwrap().1, hex("d8 d1 00 01"));
        assert_eq!(build("pause_on_removal", &json!(true), &st).unwrap().1, hex("f8 01 00"));
        assert_eq!(build("auto_power_off", &json!(0x11), &st).unwrap().1, hex("28 05 11 00"));
        assert_eq!(build("connection_mode", &json!(1), &st).unwrap().1, hex("e8 00 01"));
        assert_eq!(build("nc_amb_button", &json!(4), &st).unwrap().1, hex("68 30 04"));
        assert_eq!(build("voice_guidance", &json!(false), &st).unwrap(), (DT_MDR2, hex("48 01 01")));
    }

    #[test]
    fn build_settings_reject_bad_input() {
        let st = State::default();
        assert!(build("nope", &json!(1), &st).is_err());
        assert!(build("multipoint", &json!(1), &st).is_err());
        assert!(build("auto_power_off", &json!(0x12), &st).is_err());
        assert!(build("connection_mode", &json!(2), &st).is_err());
        assert!(build("nc_amb_button", &json!(0), &st).is_err());
        assert!(build("nc_amb_button", &json!(300), &st).is_err());
    }

    #[test]
    fn every_build_roundtrips_through_its_notification() {
        // SET payload with the reply/notify command byte must parse back to the same value
        let st = State::default();
        let cases = [
            ("touch_panel", json!(false), 0xD9u8),
            ("multipoint", json!(true), 0xD9),
            ("pause_on_removal", json!(false), 0xF9),
            ("connection_mode", json!(1), 0xE9),
            ("nc_amb_button", json!(3), 0x69),
            ("auto_power_off", json!(0x11), 0x29),
        ];
        for (key, value, ntfy) in cases {
            let (dt, mut p) = build(key, &value, &st).unwrap();
            p[0] = ntfy;
            let mut got = State::default();
            parse(dt, &p, &mut got);
            assert_eq!(serde_json::to_value(&got).unwrap()[key], value, "{key}");
        }
    }

    // --- JSON contract with the GNOME extension and the GUI ---

    #[test]
    fn json_omits_unknown_fields() {
        assert_eq!(State::default().to_json(), r#"{"connected":false}"#);
    }

    #[test]
    fn json_uses_expected_keys_and_values() {
        let st = State {
            connected: true,
            model: Some("WH-1000XM5".into()),
            battery: Some(57),
            charging: Some(false),
            codec: Some("LDAC".into()),
            nc_mode: Some(NcMode::Ambient),
            ambient_level: Some(20),
            ..Default::default()
        };
        let v: Value = serde_json::from_str(&st.to_json()).unwrap();
        assert_eq!(
            v,
            json!({"connected": true, "model": "WH-1000XM5", "battery": 57, "charging": false,
                   "codec": "LDAC", "nc_mode": "ambient", "ambient_level": 20})
        );
        assert_eq!(serde_json::from_value::<State>(v).unwrap(), st);
    }

    #[test]
    fn json_tolerates_unknown_and_missing_keys() {
        let st: State = serde_json::from_str(r#"{"connected":true,"future_key":1}"#).unwrap();
        assert!(st.connected && st.model.is_none());
    }

    #[test]
    fn init_queries_start_with_handshake_then_alerts() {
        assert_eq!(INIT_QUERIES[0], (DT_MDR, &[0x00, 0x00][..]));
        assert_eq!(INIT_QUERIES[1], (DT_MDR, &[0x94, 0x00, 0x00][..]));
    }
}
