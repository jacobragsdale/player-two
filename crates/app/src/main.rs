//! The Tauri shell around the window and the desk.

// A release build is a windowed app with no console of its own.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() -> anyhow::Result<()> {
    tauri::Builder::default()
        // A second start brings the running window forward instead of opening another desk.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                // Bringing it forward is a courtesy; if the system refuses, the window is still there.
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .run(tauri::generate_context!())?;
    Ok(())
}
