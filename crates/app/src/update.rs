//! Installing the hub's version of the app. The hub serves its installer and Tauri's manifest
//! for it, so the app always moves to the hub's version, newer or older.

use anyhow::Context;
use player_two_desk::Desk;
use tauri::{AppHandle, Url};
use tauri_plugin_updater::UpdaterExt;

/// Downloads and runs the hub's installer, then restarts into the new version.
pub async fn install(app: &AppHandle, desk: &Desk) -> anyhow::Result<()> {
    let hub = desk
        .hub_address()?
        .context("this machine has not joined a hub")?;
    let updater = app
        .updater_builder()
        .endpoints(vec![Url::parse(&format!("{hub}/update"))?])?
        // One version for the hub and the app: any difference is an update, either way.
        .version_comparator(|current, remote| remote.version != current)
        .build()?;
    let update = updater
        .check()
        .await?
        .context("the hub has no installer to give")?;
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart()
}
