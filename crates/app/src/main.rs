//! The Tauri shell around the window and the desk: one window, a tray icon that keeps the app
//! running when the window is closed, start at login, and Windows' notifications.

// A release build is a windowed app with no console of its own.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod notify;
mod update;

use player_two_desk::Desk;
use player_two_desk::ui::{Event, Request, Response};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::webview::NewWindowResponse;
use tauri::{
    App, AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// The argument start at login passes, so the app starts in the tray rather than in front.
const MINIMIZED: &str = "--minimized";

/// Passes one request from the window to the desk, but for an update, which only the app can
/// install. This is the only command the webview may call.
#[tauri::command]
async fn desk(app: AppHandle, request: Request, desk: State<'_, Desk>) -> Result<Response, String> {
    if request == Request::Update {
        return update::install(&app, &desk)
            .await
            .map(|()| Response::Done)
            .map_err(|error| format!("{error:#}"));
    }
    desk.handle(request).await
}

fn main() -> anyhow::Result<()> {
    tauri::Builder::default()
        // A second start brings the running window forward instead of opening another desk.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            notify::bring_forward(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED]),
        ))
        .setup(setup)
        // Closing the window keeps the app in the tray, where requests can still reach it.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                // A window that cannot hide stays open; nothing is lost.
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![desk])
        .run(tauri::generate_context!())?;
    Ok(())
}

fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let (desk, mut told) = Desk::new(app.path().app_data_dir()?);
    let running = desk.clone();
    tauri::async_runtime::spawn(async move { running.run().await });
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = told.recv().await {
            if let Event::Notify { chat, title, body } = &event
                && !notify::looking(&handle)
            {
                notify::show(&handle, chat.clone(), title, body);
            }
            // A window still loading misses nothing it needs: it asks for everything first.
            let _ = handle.emit("desk", event);
        }
    });
    app.manage(desk);
    window(app, !std::env::args().any(|arg| arg == MINIMIZED))?;
    tray(app)?;
    // Starting at login is how requests reach a machine whose owner has not opened the app.
    if let Err(error) = app.autolaunch().enable() {
        eprintln!("error: cannot start at login: {error}");
    }
    Ok(())
}

/// Whether a URL is the app's own page, which is all the window may show.
fn own(url: &Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

/// The one window. It shows the app's own page and nothing else: a link someone wrote opens in
/// the system's browser, if it is a web address at all, and never in the app.
fn window(app: &App, visible: bool) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Player Two")
        .inner_size(1100.0, 720.0)
        .min_inner_size(640.0, 420.0)
        .visible(visible)
        .on_navigation(|url| {
            if !own(url) {
                outside(url);
            }
            own(url)
        })
        .on_new_window(|url, _features| {
            outside(&url);
            NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

/// Opens a web address in the system's browser; anything else is dropped.
fn outside(url: &Url) {
    if matches!(url.scheme(), "http" | "https")
        && let Err(error) = tauri_plugin_opener::open_url(url.as_str(), None::<&str>)
    {
        eprintln!("error: cannot open {url}: {error}");
    }
}

/// The tray icon: a click opens the window, and its menu opens the window or quits the app.
fn tray(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Player Two", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Player Two")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => notify::bring_forward(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                notify::bring_forward(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
