//! The app the hub hands out: the installer its image carries, and the manifest Tauri's updater
//! reads to find it.

use std::path::Path;

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::app::{AppState, Refusal, Reply};

/// The installer's name in the release folder; its signature is beside it with `.sig` added.
pub const INSTALLER: &str = "player-two-setup.exe";

fn missing() -> Refusal {
    Refusal::new(StatusCode::NOT_FOUND, "this hub carries no installer")
}

async fn read(folder: &Path, name: &str) -> Reply<Vec<u8>> {
    tokio::fs::read(folder.join(name))
        .await
        .map_err(|_| missing())
}

/// `GET /download`.
pub async fn download(State(state): State<AppState>) -> Reply<Response> {
    let installer = read(&state.config.release, INSTALLER).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{INSTALLER}\""),
            ),
        ],
        installer,
    )
        .into_response())
}

/// `GET /update`: Tauri's static updater manifest, pointing at `/download` on this hub.
pub async fn update(State(state): State<AppState>) -> Reply<Json<Value>> {
    let signature = read(&state.config.release, &format!("{INSTALLER}.sig")).await?;
    // The installer must be there too, or the manifest would point at nothing.
    read(&state.config.release, INSTALLER).await?;
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platforms": {
            "windows-x86_64": {
                "signature": String::from_utf8_lossy(&signature).trim(),
                "url": format!("{}/download", state.config.url),
            }
        }
    })))
}
