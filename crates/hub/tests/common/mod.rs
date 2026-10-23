//! A real hub for the tests: in-process, on a free port, with its database in a temporary folder.

// Each test file uses part of this, and is compiled on its own.
#![allow(dead_code)]

use std::sync::Arc;

use player_two_hub::app::{AppState, Config, router};
use player_two_hub::clock::Clock;
use player_two_hub::db::Db;
use player_two_hub::stream::Streams;
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
                url: "https://hub.example".to_owned(),
            }),
            clock: clock.clone(),
            streams: Arc::new(Streams::default()),
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

/// One message read off a stream.
#[derive(Debug)]
pub struct Line {
    pub kind: String,
    pub data: Value,
}

/// An open `GET /stream`.
pub struct Lines {
    response: reqwest::Response,
    buffer: String,
}

impl Lines {
    /// The next message, waiting up to five seconds for it.
    pub async fn next(&mut self) -> Line {
        tokio::time::timeout(std::time::Duration::from_secs(5), self.read())
            .await
            .expect("a message arrives in time")
    }

    /// Whether the hub has ended the stream, waiting up to five seconds.
    pub async fn ended(&mut self) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match self.response.chunk().await {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => return true,
                }
            }
        })
        .await
        .unwrap_or(false)
    }

    async fn read(&mut self) -> Line {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                let mut kind = "message".to_owned();
                let mut data = String::new();
                for line in block.lines() {
                    if let Some(value) = line.strip_prefix("event:") {
                        kind = value.trim().to_owned();
                    } else if let Some(value) = line.strip_prefix("data:") {
                        data.push_str(value.trim_start());
                    }
                }
                if data.is_empty() {
                    continue; // A keep-alive comment.
                }
                return Line {
                    kind,
                    data: serde_json::from_str(&data).expect("JSON data"),
                };
            }
            let chunk = self
                .response
                .chunk()
                .await
                .expect("the stream reads")
                .expect("the stream stays open");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}

impl Hub {
    /// Opens a stream, resuming after `after` when given.
    pub async fn stream(&self, token: &str, after: Option<i64>) -> Lines {
        let mut request = self
            .client
            .get(format!("{}/stream", self.base))
            .bearer_auth(token);
        if let Some(after) = after {
            request = request.header("last-event-id", after.to_string());
        }
        let response = request.send().await.expect("the hub answers");
        assert_eq!(response.status().as_u16(), 200);
        Lines {
            response,
            buffer: String::new(),
        }
    }
}
