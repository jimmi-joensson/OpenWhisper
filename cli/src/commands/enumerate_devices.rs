//! `openwhisper enumerate-devices` — list input devices the engine
//! would consider for capture, with the same filter rules the
//! desktop's Settings → Audio pane applies (virtual mics dropped on
//! Mac via CoreAudio TransportType, devices that fail to open
//! dropped via probe-open).

use anyhow::Result;
use openwhisper_core::audio::{self, AudioDeviceInfo};

pub fn run(json: bool) -> Result<()> {
    let devices = audio::audio_list_input_devices();
    if json {
        print_json(&devices)?;
    } else {
        print_text(&devices);
    }
    Ok(())
}

fn print_text(devices: &[AudioDeviceInfo]) {
    if devices.is_empty() {
        eprintln!("(no input devices visible)");
        return;
    }
    // Tab-separated: id\tlabel\tis_default. Stable across platforms
    // and machine-parseable without --json.
    for d in devices {
        let default = if d.is_default { "default" } else { "" };
        println!("{}\t{}\t{}", d.id, d.label, default);
    }
}

fn print_json(devices: &[AudioDeviceInfo]) -> Result<()> {
    let arr: Vec<_> = devices
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "label": d.label,
                "is_default": d.is_default,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&arr)?);
    Ok(())
}
