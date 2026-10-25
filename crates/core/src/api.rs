//! The hub's HTTP API: what each route takes and gives back. See `docs/plan/protocol.md`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::event::{Body, EventId};
use crate::names::{ChannelName, ChatId, PersonName, PlaceName};

/// `GET /version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VersionInfo {
    /// The hub's version, which the app must match.
    pub version: String,
}

/// `POST /join`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    /// The shared join token.
    pub join_token: String,
    /// The name to join under.
    pub name: PersonName,
}

/// What `POST /join` gives back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Joined {
    /// The personal token for every later request.
    pub token: String,
}

/// Whether a place is a machine or a shared folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    /// The person's machine.
    Machine,
    /// One of their shared folders. The wire calls it `repo`.
    Repo,
}

/// A place as the hub lists it: its name and kind, and nothing about where it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct PlaceInfo {
    /// Its name.
    pub name: PlaceName,
    /// Machine or folder.
    pub kind: PlaceKind,
}

/// One person, as `GET /people` lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Person {
    /// Their name.
    pub name: PersonName,
    /// Whether they have a stream open now.
    pub here: bool,
    /// When their last stream closed, in Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // A `number` in TypeScript, as the event's `ts` is.
    #[specta(type = Option<i32>)]
    pub last_seen: Option<i64>,
    /// The places their desk advertises.
    pub places: Vec<PlaceInfo>,
}

/// One channel, as `GET /channels` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ChannelInfo {
    /// Its name, without the `#`.
    pub name: ChannelName,
    /// Who is in it.
    pub members: Vec<PersonName>,
}

/// `POST /channels`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewChannel {
    /// The new channel's name.
    pub name: ChannelName,
}

/// `POST /events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEvent {
    /// The chat to post in.
    pub chat: ChatId,
    /// The thread to post in, by its first message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<EventId>,
    /// What it says.
    #[serde(flatten)]
    pub body: Body,
}

/// What `POST /events` gives back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posted {
    /// The hub's number for the new event.
    pub id: EventId,
    /// When the hub stored it.
    pub ts: i64,
}

/// A presence change on the stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Presence {
    /// Whose.
    pub name: PersonName,
    /// Whether they are here now.
    pub here: bool,
}

/// What `POST /admin/join-code` gives back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCodeReply {
    /// The code to send someone.
    pub code: String,
}

/// `POST /admin/reset` and `POST /admin/remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetRequest {
    /// The person to let join again, or to remove.
    pub name: PersonName,
}

/// Every refusal from the hub has this body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// What went wrong, in words a person can act on.
    pub error: String,
}
