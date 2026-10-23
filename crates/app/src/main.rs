//! The Tauri shell around the window and the desk.

// A release build is a windowed app with no console of its own.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use player_two_desk::Desk;
use player_two_desk::ui::{Request, Response};
use tauri::{Manager, State};

/// Passes one request from the window to the desk. This is the only command the webview may call.
#[tauri::command]
async fn desk(request: Request, desk: State<'_, Desk>) -> Result<Response, String> {
    Ok(desk.handle(request).await)
}

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
        .manage(Desk::new())
        .invoke_handler(tauri::generate_handler![desk])
        .run(tauri::generate_context!())?;
    Ok(())
}
