#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        fn hello_from_rust() -> String;
        fn core_version() -> String;
    }
}

fn hello_from_rust() -> String {
    "Hello from openwhisper-core (Rust)".to_string()
}

fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
