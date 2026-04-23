mod audio;

#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        fn hello_from_rust() -> String;
        fn core_version() -> String;

        fn audio_start_capture() -> Result<(), String>;
        fn audio_stop_capture();
        fn audio_drain_samples() -> Vec<f32>;
        fn audio_is_capturing() -> bool;
    }
}

fn hello_from_rust() -> String {
    "Hello from openwhisper-core (Rust)".to_string()
}

fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn audio_start_capture() -> Result<(), String> {
    audio::audio_start_capture()
}

fn audio_stop_capture() {
    audio::audio_stop_capture()
}

fn audio_drain_samples() -> Vec<f32> {
    audio::audio_drain_samples()
}

fn audio_is_capturing() -> bool {
    audio::audio_is_capturing()
}
