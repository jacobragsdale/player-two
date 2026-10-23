//! The desk's side of the hub's HTTP API, and the reader for its stream.

use std::time::Duration;

use player_two_core::api::{
    ApiError, ChannelInfo, JoinRequest, Joined, NewChannel, NewEvent, Person, PlaceInfo, Posted,
    Presence, VersionInfo,
};
use player_two_core::event::{Event, EventId};
use player_two_core::names::{ChannelName, ChatId, PersonName};
use reqwest::{RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::files::Identity;

/// How long a stream may stay silent before it counts as broken. The hub sends a keep-alive
/// every 20 seconds.
const SILENCE: Duration = Duration::from_secs(60);

/// Sends a request and reads its JSON answer, turning a refusal into the hub's own words.
async fn call<T: DeserializeOwned>(base: &str, request: RequestBuilder) -> anyhow::Result<T> {
    let response = request
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("cannot reach the hub at {base}: {error}"))?;
    read(response).await
}

/// The hub no longer knows this machine's token: someone ran `reset`, or the hub is new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotJoined;

impl std::fmt::Display for NotJoined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the hub no longer knows this machine; join again with a join code")
    }
}

impl std::error::Error for NotJoined {}

/// Reads a response: its JSON when it succeeded, the hub's reason when it did not.
async fn read<T: DeserializeOwned>(response: Response) -> anyhow::Result<T> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(NotJoined.into());
    }
    let text = response.text().await?;
    if !status.is_success() {
        let reason = serde_json::from_str::<ApiError>(&text).map_or_else(
            |_| format!("the hub answered {status}"),
            |error| error.error,
        );
        anyhow::bail!(reason);
    }
    Ok(serde_json::from_str(if text.is_empty() {
        "null"
    } else {
        &text
    })?)
}

/// Joins the hub at `base` as `name`, giving back the personal token.
pub async fn join(
    http: &reqwest::Client,
    base: &str,
    join_token: &str,
    name: &PersonName,
) -> anyhow::Result<String> {
    let request = http.post(format!("{base}/join")).json(&JoinRequest {
        join_token: join_token.to_owned(),
        name: name.clone(),
    });
    let joined: Joined = call(base, request).await?;
    Ok(joined.token)
}

/// A chat id as a path segment: `#` would start a fragment.
fn segment(chat: &ChatId) -> String {
    chat.to_string().replace('#', "%23")
}

/// The hub, as the person this machine joined as.
#[derive(Debug, Clone)]
pub struct Hub {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Hub {
    /// The hub `identity` joined.
    #[must_use]
    pub fn new(http: reqwest::Client, identity: &Identity) -> Self {
        Self {
            http,
            base: identity.hub.clone(),
            token: identity.token.clone(),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let request = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        call(&self.base, request).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &impl Serialize,
    ) -> anyhow::Result<T> {
        let request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .json(body);
        call(&self.base, request).await
    }

    /// The hub's version.
    pub async fn version(&self) -> anyhow::Result<String> {
        Ok(self.get::<VersionInfo>("/version").await?.version)
    }

    /// Every channel.
    pub async fn channels(&self) -> anyhow::Result<Vec<ChannelInfo>> {
        self.get("/channels").await
    }

    /// Everyone who has joined.
    pub async fn people(&self) -> anyhow::Result<Vec<Person>> {
        self.get("/people").await
    }

    /// Up to `limit` events of a chat from before `before`, or the newest, oldest first.
    pub async fn page(
        &self,
        chat: &ChatId,
        before: Option<EventId>,
        limit: u32,
    ) -> anyhow::Result<Vec<Event>> {
        let before = before.map_or_else(String::new, |id| format!("&before={id}"));
        self.get(&format!(
            "/chats/{}/events?limit={limit}{before}",
            segment(chat)
        ))
        .await
    }

    /// Posts an event.
    pub async fn post(&self, event: &NewEvent) -> anyhow::Result<Posted> {
        self.send(reqwest::Method::POST, "/events", event).await
    }

    /// Makes a channel and joins it.
    pub async fn create_channel(&self, name: &ChannelName) -> anyhow::Result<()> {
        let body = NewChannel { name: name.clone() };
        self.send(reqwest::Method::POST, "/channels", &body).await
    }

    /// Joins or leaves a channel.
    pub async fn membership(&self, name: &ChannelName, join: bool) -> anyhow::Result<()> {
        let path = format!("/channels/{name}/{}", if join { "join" } else { "leave" });
        self.send(reqwest::Method::POST, &path, &serde_json::json!({}))
            .await
    }

    /// Replaces the places this person advertises.
    pub async fn set_places(&self, places: &[PlaceInfo]) -> anyhow::Result<()> {
        self.send(reqwest::Method::PUT, "/places", &places).await
    }

    /// Opens the stream, resuming after `after` when given.
    pub async fn stream(&self, after: Option<EventId>) -> anyhow::Result<Stream> {
        let mut request = self
            .http
            .get(format!("{}/stream", self.base))
            .bearer_auth(&self.token);
        if let Some(after) = after {
            request = request.header("last-event-id", after.to_string());
        }
        let response = request
            .send()
            .await
            .map_err(|error| anyhow::anyhow!("cannot reach the hub at {}: {error}", self.base))?;
        if !response.status().is_success() {
            read::<serde_json::Value>(response).await?;
            anyhow::bail!("the hub would not open a stream");
        }
        Ok(Stream {
            response,
            buffer: String::new(),
        })
    }
}

/// Something that arrived on the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// An event this person may see.
    Event(Event),
    /// Someone arrived or left.
    Presence(Presence),
}

/// An open stream from the hub.
#[derive(Debug)]
pub struct Stream {
    response: Response,
    buffer: String,
}

impl Stream {
    /// The next event or presence change. `None` when the hub ended the stream.
    pub async fn next(&mut self) -> anyhow::Result<Option<Incoming>> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                if let Some(incoming) = parse(&block)? {
                    return Ok(Some(incoming));
                }
                continue;
            }
            let chunk = tokio::time::timeout(SILENCE, self.response.chunk())
                .await
                .map_err(|_| anyhow::anyhow!("the hub went quiet"))??;
            let Some(chunk) = chunk else {
                return Ok(None);
            };
            // The stream is text; carriage returns, which the format allows, are dropped.
            self.buffer
                .push_str(&String::from_utf8_lossy(&chunk).replace('\r', ""));
        }
    }
}

/// One block of the stream: an event, a presence change, or a keep-alive comment (`None`).
fn parse(block: &str) -> anyhow::Result<Option<Incoming>> {
    let mut kind = "message";
    let mut data = String::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            kind = value.trim();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    Ok(Some(match kind {
        "presence" => Incoming::Presence(serde_json::from_str(&data)?),
        _ => Incoming::Event(serde_json::from_str(&data)?),
    }))
}

#[cfg(test)]
mod tests {
    use super::{Incoming, parse};

    #[test]
    fn stream_blocks_are_read() {
        assert_eq!(parse(":\n\n").ok(), Some(None));
        let presence = parse("event: presence\ndata: {\"name\":\"bob\",\"here\":true}\n\n");
        assert!(matches!(presence, Ok(Some(Incoming::Presence(p))) if p.here));
        let event = parse(
            "id: 4\ndata: {\"id\":4,\"chat\":\"#ops\",\"author\":\"bob\",\"ts\":1,\"kind\":\"message\",\"body\":{\"text\":\"hi\"}}\n\n",
        );
        assert!(matches!(event, Ok(Some(Incoming::Event(e))) if e.id.0 == 4));
        assert!(parse("data: {not json}\n\n").is_err());
    }
}
