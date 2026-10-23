//! One person's side of Player Two: their hub client, places, agents, approvals and runs.

pub mod files;
pub mod hub;
pub mod ui;

use std::path::PathBuf;

use anyhow::Context;
use files::{Files, Identity};
use player_two_core::join_code::JoinCode;
use player_two_core::names::PersonName;
use ui::{Request, Response};

/// The desk: everything the window does goes through here.
#[derive(Debug)]
pub struct Desk {
    files: Files,
    http: reqwest::Client,
}

impl Desk {
    /// A desk keeping its files in `folder`.
    #[must_use]
    pub fn new(folder: PathBuf) -> Self {
        Self {
            files: Files::new(folder),
            http: reqwest::Client::new(),
        }
    }

    /// Answers one request from the window. A failure is said in words for the person.
    pub async fn handle(&self, request: Request) -> Result<Response, String> {
        self.answer(request)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn answer(&self, request: Request) -> anyhow::Result<Response> {
        match request {
            Request::Hello => Ok(Response::Hello {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            }),
            Request::Setup => Ok(Response::Setup {
                joined: self.files.identity()?.map(|identity| identity.name),
                suggested_name: suggested_name().map(String::from),
            }),
            Request::Join { code, name } => {
                self.join(&code, &name).await?;
                Ok(Response::Done)
            }
        }
    }

    async fn join(&self, code: &str, name: &str) -> anyhow::Result<()> {
        let code = JoinCode::decode(code)?;
        let name = PersonName::new(name.trim()).context(
            "a name is a lowercase letter, then lowercase letters, digits or -, up to 32 in all",
        )?;
        let token = hub::join(&self.http, &code.hub, &code.token, &name).await?;
        self.files.save_identity(&Identity {
            hub: code.hub,
            name,
            token,
        })
    }
}

/// The name to offer at first run, from the system's user name.
fn suggested_name() -> Option<PersonName> {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()?;
    PersonName::from_user_name(&user)
}
