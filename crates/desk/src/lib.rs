//! One person's side of Player Two: their hub client, places, agents, approvals and runs.

pub mod ui;

use ui::{Request, Response};

/// The desk: everything the window does goes through here.
#[derive(Debug, Default)]
pub struct Desk {}

impl Desk {
    /// Makes a desk.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Answers one request from the window.
    pub async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Hello => Response::Hello {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
        }
    }
}
