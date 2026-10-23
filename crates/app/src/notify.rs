//! Windows' own notifications, whose click opens the chat and thread they are about.

use player_two_core::event::EventId;
use player_two_core::names::ChatId;
use tauri::{AppHandle, Manager};

/// Shows a notification. Clicking it brings the window forward on that chat and thread.
#[cfg(windows)]
pub fn show(app: &AppHandle, chat: ChatId, thread: Option<EventId>, title: &str, body: &str) {
    use tauri_winrt_notification::Toast;

    // An installed app is known to Windows by its identifier; a development build is not, so
    // it borrows PowerShell's.
    let id = if cfg!(debug_assertions) {
        Toast::POWERSHELL_APP_ID.to_owned()
    } else {
        app.config().identifier.clone()
    };
    let handle = app.clone();
    let shown = Toast::new(&id)
        .title(title)
        .text1(body)
        .on_activated(move |_| {
            open(&handle, chat.clone(), thread);
            Ok(())
        })
        .show();
    if let Err(error) = shown {
        eprintln!("error: cannot show a notification: {error}");
    }
}

/// Shows a notification about the app itself, which a click brings forward.
#[cfg(windows)]
pub fn tell(app: &AppHandle, title: &str, body: &str) {
    use tauri_winrt_notification::Toast;

    let id = if cfg!(debug_assertions) {
        Toast::POWERSHELL_APP_ID.to_owned()
    } else {
        app.config().identifier.clone()
    };
    let handle = app.clone();
    let shown = Toast::new(&id)
        .title(title)
        .text1(body)
        .on_activated(move |_| {
            bring_forward(&handle);
            Ok(())
        })
        .show();
    if let Err(error) = shown {
        eprintln!("error: cannot show a notification: {error}");
    }
}

/// Elsewhere, where the app is only developed, notifications are left out.
#[cfg(not(windows))]
pub fn tell(_app: &AppHandle, _title: &str, _body: &str) {}

/// Elsewhere, where the app is only developed, notifications are left out.
#[cfg(not(windows))]
pub fn show(_app: &AppHandle, _chat: ChatId, _thread: Option<EventId>, _title: &str, _body: &str) {}

/// Brings the window forward, on `chat` and `thread`.
#[cfg(windows)]
fn open(app: &AppHandle, chat: ChatId, thread: Option<EventId>) {
    bring_forward(app);
    // A window that has gone is reopened by the next click; there is nothing to tell it.
    let _ = tauri::Emitter::emit(
        app,
        "desk",
        player_two_desk::ui::Event::Open { chat, thread },
    );
}

/// Shows the window, restored and in front.
pub fn bring_forward(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // Each of these is a courtesy; if the system refuses one, the window is still there.
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Whether the person is looking at the window now, in which case nothing needs a notification.
pub fn looking(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    })
}
