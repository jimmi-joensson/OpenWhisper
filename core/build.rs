use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = crate_dir
        .parent()
        .expect("core crate must live under the repo root");
    let out_dir = repo_root
        .join("apps")
        .join("macos")
        .join("Generated");

    let bridges = vec!["src/lib.rs"];
    for path in &bridges {
        println!("cargo:rerun-if-changed={}", path);
    }

    swift_bridge_build::parse_bridges(bridges)
        .write_all_concatenated(&out_dir, "openwhisper_core");
}
