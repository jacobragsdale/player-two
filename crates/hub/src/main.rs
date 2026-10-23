//! `player-two-hub`: runs the hub, or asks a running one to do an admin task.

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use player_two_core::api::{ApiError, JoinCodeReply, ResetRequest};
use player_two_core::names::PersonName;
use player_two_hub::app::{AppState, Config, router};
use player_two_hub::clock::Clock;
use player_two_hub::db::Db;
use player_two_hub::stream::Streams;

/// Where the hub listens, and where the admin commands find it, unless `PLAYER_TWO_PORT` says.
const PORT: u16 = 8787;

fn port() -> anyhow::Result<u16> {
    env::var("PLAYER_TWO_PORT").map_or(Ok(PORT), |port| {
        port.parse()
            .context("PLAYER_TWO_PORT must be a port number")
    })
}

const USAGE: &str = "usage:
  player-two-hub [serve]        run the hub
  player-two-hub join-code      print a join code for the running hub
  player-two-hub reset NAME     let NAME join again, for example from a new machine
  player-two-hub --version";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => serve().await,
        ["join-code"] => {
            let reply: JoinCodeReply = admin("join-code", &serde_json::json!({})).await?;
            println!("{}", reply.code);
            Ok(())
        }
        ["reset", name] => {
            let name = PersonName::new(*name)?;
            admin::<serde_json::Value>("reset", &ResetRequest { name: name.clone() }).await?;
            println!("{name} can join again with a join code.");
            Ok(())
        }
        ["--version"] => {
            println!("player-two-hub {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

/// A setting from the environment, which must be there.
fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).with_context(|| format!("{name} must be set"))
}

async fn serve() -> anyhow::Result<()> {
    let config = Config {
        join_token: required("PLAYER_TWO_JOIN_TOKEN")?,
        admin_token: required("PLAYER_TWO_ADMIN_TOKEN")?,
        url: required("PLAYER_TWO_URL")?.trim_end_matches('/').to_owned(),
    };
    let path = env::var_os("PLAYER_TWO_DB")
        .map_or_else(|| PathBuf::from("/data/player-two.db"), PathBuf::from);
    let state = AppState {
        db: Db::open(&path)?,
        config: Arc::new(config),
        clock: Clock::default(),
        streams: Arc::new(Streams::default()),
    };
    let port = port()?;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("cannot listen on port {port}"))?;
    println!(
        "player-two-hub {} listening on port {port}",
        env!("CARGO_PKG_VERSION")
    );
    axum::serve(listener, router(state))
        .with_graceful_shutdown(stopped())
        .await?;
    Ok(())
}

/// Resolves when the hub is told to stop: Ctrl+C, or Kubernetes' SIGTERM.
async fn stopped() {
    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(_) => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = terminate => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Calls the running hub's `/admin/{path}` with the admin token from the environment.
async fn admin<T: serde::de::DeserializeOwned>(
    path: &str,
    body: &impl serde::Serialize,
) -> anyhow::Result<T> {
    let token = required("PLAYER_TWO_ADMIN_TOKEN")?;
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/admin/{path}", port()?))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .context("cannot reach the hub on this machine; run this beside it, as in kubectl exec")?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        let error = serde_json::from_str::<ApiError>(&text).map_or(text, |error| error.error);
        bail!("the hub refused: {error}");
    }
    // An empty answer reads as JSON null.
    Ok(serde_json::from_str(if text.is_empty() {
        "null"
    } else {
        &text
    })?)
}
