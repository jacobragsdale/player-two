//! A real hub for the tests: in-process, on a free port, with its database in a temporary folder.

// Each test file uses part of this, and is compiled on its own.
#![allow(dead_code)]

use std::sync::Arc;

use player_two_hub::app::{AppState, Config, router};
use player_two_hub::clock::Clock;
use player_two_hub::db::Db;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub const JOIN_TOKEN: &str = "join-token";
pub const ADMIN_TOKEN: &str = "admin-token";

/// A running hub.
pub struct Hub {
    pub base: String,
    pub clock: Clock,
    pub client: reqwest::Client,
    _folder: tempfile::TempDir,
}

/// What a request came back with.
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    pub fn into<T: DeserializeOwned>(self) -> T {
        assert!(
            self.status < 300,
            "expected success, got {} {}",
            self.status,
            self.body
        );
        serde_json::from_value(self.body).expect("the answer has the expected shape")
    }
}

impl Hub {
    pub async fn start() -> Self {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let db = Db::open(&folder.path().join("hub.db")).expect("the database opens");
        let clock = Clock::fixed(1_700_000_000_000);
        let state = AppState {
            db,
            config: Arc::new(Config {
                join_token: JOIN_TOKEN.to_owned(),
                admin_token: ADMIN_TOKEN.to_owned(),
            }),
            clock: clock.clone(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a free port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        tokio::spawn(async move { axum::serve(listener, router(state)).await });
        Self {
            base,
            clock,
            client: reqwest::Client::new(),
            _folder: folder,
        }
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Answer {
        let response = request.send().await.expect("the hub answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("a body");
        let body = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Answer { status, body }
    }

    pub async fn get(&self, token: &str, path: &str) -> Answer {
        self.send(
            self.client
                .get(format!("{}{path}", self.base))
                .bearer_auth(token),
        )
        .await
    }

    pub async fn post(&self, token: &str, path: &str, body: &impl Serialize) -> Answer {
        self.send(
            self.client
                .post(format!("{}{path}", self.base))
                .bearer_auth(token)
                .json(body),
        )
        .await
    }

    pub async fn put(&self, token: &str, path: &str, body: &impl Serialize) -> Answer {
        self.send(
            self.client
                .put(format!("{}{path}", self.base))
                .bearer_auth(token)
                .json(body),
        )
        .await
    }

    /// Joins as `name` and returns their personal token.
    pub async fn join(&self, name: &str) -> String {
        let answer = self
            .post(
                "",
                "/join",
                &serde_json::json!({"join_token": JOIN_TOKEN, "name": name}),
            )
            .await;
        answer.body["token"].as_str().expect("a token").to_owned()
    }
}
