use tauri_build::{AppManifest, Attributes};

fn main() {
    // The app's one command. Naming it here makes Tauri require a permission for it, which only
    // the main window's capability grants.
    tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(&["desk"])))
        .expect("the Tauri build step runs");
}
