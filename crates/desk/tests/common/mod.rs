//! A real hub for the desk's tests, in-process on a free port, and desks that use it.

// Each test file uses part of this, and is compiled on its own.
#![allow(dead_code)]

use std::sync::Arc;

use player_two_core::event::EventId;
use player_two_core::join_code::JoinCode;
use player_two_core::names::ChatId;
use player_two_desk::Desk;
use player_two_desk::ui::{ChatView, Connection, Overview, Request, Response};
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

    /// A desk with its own folder, not yet joined and not running.
    pub fn desk(&self, folder_name: &str) -> Desk {
        Desk::new(self.folder(folder_name)).0
    }

    /// The folder a desk of that name keeps its files in.
    pub fn folder(&self, folder_name: &str) -> std::path::PathBuf {
        self.folder.path().join(folder_name)
    }

    /// A desk joined as `name` and running.
    pub async fn joined(&self, name: &str) -> Desk {
        let desk = self.desk(name);
        let joined = desk
            .handle(Request::Join {
                code: self.code(),
                name: name.to_owned(),
            })
            .await;
        assert_eq!(joined, Ok(Response::Done));
        let running = desk.clone();
        tokio::spawn(async move { running.run().await });
        until(&desk, "it connects", |overview| {
            overview.connection == Connection::Connected
        })
        .await;
        desk
    }

    /// Lets `name` join again, as `player-two-hub reset NAME` does.
    pub async fn reset(&self, name: &str) {
        let answer = reqwest::Client::new()
            .post(format!("{}/admin/reset", self.base))
            .bearer_auth("admin-token")
            .json(&serde_json::json!({"name": name}))
            .send()
            .await
            .expect("the hub answers");
        assert!(answer.status().is_success());
    }
}

/// The desk's overview.
pub async fn overview(desk: &Desk) -> Overview {
    match desk.handle(Request::Overview).await {
        Ok(Response::Overview(overview)) => overview,
        other => panic!("an overview, not {other:?}"),
    }
}

/// One chat as the desk shows it.
pub async fn chat(desk: &Desk, chat: &str, thread: Option<EventId>) -> ChatView {
    let chat = ChatId::try_from(chat.to_owned()).expect("a chat id");
    match desk.handle(Request::Chat { chat, thread }).await {
        Ok(Response::Chat(view)) => view,
        other => panic!("a chat, not {other:?}"),
    }
}

/// Waits up to five seconds for the desk's overview to satisfy `test`.
pub async fn until(desk: &Desk, what: &str, test: impl Fn(&Overview) -> bool) -> Overview {
    for _ in 0..250 {
        let overview = overview(desk).await;
        if test(&overview) {
            return overview;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("waited too long for {what}: {:?}", overview(desk).await);
}

/// Waits up to five seconds for a chat's texts to be `expected`.
pub async fn until_texts(
    desk: &Desk,
    id: &str,
    thread: Option<EventId>,
    expected: &[&str],
) -> ChatView {
    for _ in 0..250 {
        let view = chat(desk, id, thread).await;
        if view
            .items
            .iter()
            .map(|item| item.text.as_str())
            .eq(expected.iter().copied())
        {
            return view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "waited too long for {expected:?} in {id}: {:?}",
        chat(desk, id, thread).await.items
    );
}
