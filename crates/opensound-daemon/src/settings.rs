// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings that live on this computer instead of the headset: the daily voice greeting and how
//! the GNOME Shell extension looks. Stored as JSON in $XDG_CONFIG_HOME/opensound/settings.json.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Map, Value};

type Check = fn(&Value) -> bool;

/// Keys clients may change through `Set`, with their JSON type check.
pub const KEYS: &[(&str, Check)] = &[
    ("greeting", Value::is_boolean),
    // what the GNOME Shell extension shows
    ("panel_icon", Value::is_string),
    ("panel_show_icon", Value::is_boolean),
    ("panel_show_percent", Value::is_boolean),
    ("panel_show_battery_icon", Value::is_boolean),
    ("widget_show_battery", Value::is_boolean),
    ("widget_show_noise", Value::is_boolean),
    ("widget_show_speak_to_chat", Value::is_boolean),
    ("widget_show_equalizer", Value::is_boolean),
    ("widget_show_power", Value::is_boolean),
    ("widget_quick_settings", Value::is_boolean),
];

/// Value a key has until the user changes it; clients always see these keys.
pub fn default(key: &str) -> Option<Value> {
    match key {
        "panel_icon" => Some(Value::from("audio-headphones-symbolic")),
        "panel_show_battery_icon" => Some(Value::Bool(false)),
        k if k.starts_with("panel_show_") || k.starts_with("widget_") => Some(Value::Bool(true)),
        _ => None,
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("opensound").join("settings.json"))
}

pub fn load() -> Map<String, Value> {
    path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(settings: &Map<String, Value>) {
    let Some(p) = path() else { return };
    let result = p
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(&p, serde_json::to_vec_pretty(settings).unwrap_or_default()));
    if let Err(e) = result {
        eprintln!("opensound: cannot save {}: {e}", p.display());
    }
}

/// Local date and time: (year, month 1-12, day, weekday 0=Sunday, hour).
pub fn now() -> (i32, u32, u32, u32, u32) {
    // SAFETY: time(2) and localtime_r(3) with valid pointers; tm is plain data.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32, tm.tm_wday as u32, tm.tm_hour as u32)
    }
}

/// "2026-10-22": the day the greeting was last spoken.
pub fn today() -> String {
    let (y, m, d, _, _) = now();
    format!("{y:04}-{m:02}-{d:02}")
}

/// Spanish greeting with the date, and the battery level when known.
pub fn greeting_text((year, month, day, weekday, hour): (i32, u32, u32, u32, u32), battery: Option<u8>) -> String {
    const DAYS: [&str; 7] = ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"];
    const MONTHS: [&str; 12] = [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ];
    let hello = match hour {
        5..=11 => "Buenos días",
        12..=18 => "Buenas tardes",
        _ => "Buenas noches",
    };
    let weekday = DAYS[weekday as usize % 7];
    let month = MONTHS[(month as usize + 11) % 12];
    let mut text = format!("{hello}. Hoy es {weekday} {day} de {month} de {year}.");
    if let Some(b) = battery {
        text.push_str(&format!(" La batería de los auriculares está al {b} por ciento."));
    }
    text
}

/// Piper neural voice installed per user: (binary, model, sample rate).
fn piper() -> Option<(PathBuf, PathBuf, u32)> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    let dir = data.join("opensound/piper");
    let binary = dir.join("piper/piper");
    let model = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "onnx"))?;
    let config: Value = serde_json::from_slice(&std::fs::read(model.with_extension("onnx.json")).ok()?).ok()?;
    let rate = config["audio"]["sample_rate"].as_u64().unwrap_or(22050) as u32;
    binary.is_file().then_some((binary, model, rate))
}

/// Mono 16-bit WAV with `lead_ms` of silence first, so a waking Bluetooth link does not eat the first words.
pub fn wav(pcm: &[u8], rate: u32, lead_ms: u32) -> Vec<u8> {
    let silence = (rate * lead_ms / 1000 * 2) as usize;
    let data_len = (silence + pcm.len()) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend(b"RIFF");
    w.extend((36 + data_len).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes()); // PCM
    w.extend(1u16.to_le_bytes()); // mono
    w.extend(rate.to_le_bytes());
    w.extend((rate * 2).to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data_len.to_le_bytes());
    w.resize(44 + silence, 0);
    w.extend(pcm);
    w
}

/// Natural voice through Piper when installed; returns false so the caller can fall back.
fn speak_piper(text: &str) -> bool {
    use std::io::Write;
    let Some((binary, model, rate)) = piper() else { return false };
    let child = Command::new(&binary)
        .arg("--model")
        .arg(&model)
        .arg("--output_raw")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let Ok(out) = child.wait_with_output() else { return false };
    if !out.status.success() || out.stdout.is_empty() {
        return false;
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let file = dir.join("opensound-greeting.wav");
    if std::fs::write(&file, wav(&out.stdout, rate, 600)).is_err() {
        return false;
    }
    let played = ["pw-play", "paplay"]
        .iter()
        .any(|p| Command::new(p).arg(&file).stdin(Stdio::null()).status().is_ok_and(|s| s.success()));
    let _ = std::fs::remove_file(&file);
    played
}

/// Speaks Spanish text: Piper if installed, otherwise speech-dispatcher. The headset is the default
/// output while it is connected.
pub fn speak(text: &str) {
    if speak_piper(text) {
        return;
    }
    match Command::new("spd-say").args(["-l", "es", "-w", text]).stdin(Stdio::null()).status() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("opensound: spd-say failed: {s}"),
        Err(e) => eprintln!("opensound: cannot run spd-say (install speech-dispatcher): {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_names_the_day_and_period() {
        assert_eq!(
            greeting_text((2026, 10, 22, 4, 8), Some(47)),
            "Buenos días. Hoy es jueves 22 de octubre de 2026. La batería de los auriculares está al 47 por ciento."
        );
        assert!(greeting_text((2026, 1, 1, 0, 15), None).starts_with("Buenas tardes. Hoy es domingo 1 de enero"));
        assert!(greeting_text((2026, 12, 31, 3, 23), None).starts_with("Buenas noches"));
    }

    #[test]
    fn wav_header_and_leading_silence() {
        let w = wav(&[1, 2, 3, 4], 1000, 10); // 10 ms at 1 kHz = 10 samples = 20 bytes
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 24);
        assert_eq!(w.len(), 44 + 24);
        assert!(w[44..64].iter().all(|b| *b == 0));
        assert_eq!(&w[64..], [1, 2, 3, 4]);
    }

    #[test]
    fn widget_keys_have_defaults() {
        assert_eq!(default("widget_show_power"), Some(Value::Bool(true)));
        assert_eq!(default("panel_show_battery_icon"), Some(Value::Bool(false)));
        assert_eq!(default("panel_icon"), Some(Value::from("audio-headphones-symbolic")));
        assert_eq!(default("greeting"), None);
    }

    #[test]
    fn keys_validate_types() {
        let check = |k: &str, v: Value| KEYS.iter().find(|(n, _)| *n == k).is_some_and(|(_, ok)| ok(&v));
        assert!(check("greeting", Value::Bool(true)));
        assert!(!check("greeting", Value::from("yes")));
        assert!(!check("bogus", Value::Bool(true)));
    }
}
