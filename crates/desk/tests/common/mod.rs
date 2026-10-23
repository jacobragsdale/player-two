//! A real hub for the desk's tests, in-process on a free port, and desks that use it.

// Each test file uses part of this, and is compiled on its own.
#![allow(dead_code)]

use std::sync::Arc;

use player_two_core::join_code::JoinCode;
use player_two_desk::Desk;
use player_two_hub::app::{AppState, Config, router};
use player_two_hub::clock::Clock;
use player_two_hub::db::Db;
use player_two_hub::stream::Streams;

pub const JOIN_TOKEN: &str = "join-token";

/// A running hub.
pub struct Hub {
    pub base: String,
    pub state: AppState,
    folder: tempfile::TempDir,
}

impl Hub {
    pub async fn start() -> Self {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let state = AppState {
            db: Db::open(&folder.path().join("hub.db")).expect("the database opens"),
            config: Arc::new(Config {
                join_token: JOIN_TOKEN.to_owned(),
                admin_token: "admin-token".to_owned(),
                url: String::new(),
                backups: folder.path().join("backups"),
                retention_days: 30,
                release: folder.path().join("release"),
            }),
            clock: Clock::default(),
            streams: Arc::new(Streams::default()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a free port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let app = router(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self {
            base,
            state,
            folder,
        }
    }

    /// A join code for this hub.
    pub fn code(&self) -> String {
        JoinCode {
            hub: self.base.clone(),
            token: JOIN_TOKEN.to_owned(),
        }
        .encode()
    }

    /// A desk with its own folder, not yet joined.
    pub fn desk(&self, folder_name: &str) -> Desk {
        Desk::new(self.folder.path().join(folder_name))
    }
}
