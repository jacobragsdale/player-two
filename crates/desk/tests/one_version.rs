//! The workspace's version in Cargo.toml is the only version number: the hub, the desk and the
//! app have it from Cargo, and the window's package.json must say the same.

use std::fs;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_window_has_the_workspaces_version() {
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("ui/package.json")).expect("ui/package.json reads"),
    )
    .expect("it is JSON");
    assert_eq!(
        manifest["version"],
        env!("CARGO_PKG_VERSION"),
        "set ui/package.json's version to the workspace's"
    );
}

#[test]
fn tauri_takes_the_version_from_cargo() {
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root().join("crates/app/tauri.conf.json"))
            .expect("tauri.conf.json reads"),
    )
    .expect("it is JSON");
    assert!(
        config.get("version").is_none(),
        "tauri.conf.json must not set a version of its own"
    );
}
