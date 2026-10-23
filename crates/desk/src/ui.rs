//! What crosses between the window and the desk: a request and its response, and the events the
//! desk sends unasked.
//!
//! These are the only types the webview sees. A test writes them to `ui/src/desk.ts`, so the
//! window is typed by what is here and never by hand.

use player_two_core::api::{ChannelInfo, Person};
use player_two_core::event::{EventId, RunsIn, Tag};
use player_two_core::names::{ChatId, PersonName, PlaceName};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::places::Folder;

/// Something the window asks of the desk.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Asks the desk to say who it is.
    Hello,
    /// Asks whether this machine has joined, and what name to offer if not.
    Setup,
    /// Joins the hub in a join code under a name.
    Join {
        /// The join code, as pasted.
        code: String,
        /// The name to join under.
        name: String,
    },
    /// Asks for the chat list, the people and how the connection stands.
    Overview,
    /// Asks for one chat, or one thread of it.
    Chat {
        /// Which chat.
        chat: ChatId,
        /// The thread, by its first message, or the chat's top level.
        thread: Option<EventId>,
    },
    /// Fetches the chat's next older page from the hub.
    Older {
        /// Which chat.
        chat: ChatId,
    },
    /// Sends a message.
    Send {
        /// Where.
        chat: ChatId,
        /// In which thread, if any.
        thread: Option<EventId>,
        /// What it says.
        text: String,
    },
    /// Marks everything in a chat as read.
    Read {
        /// Which chat.
        chat: ChatId,
    },
    /// Makes a channel and joins it.
    CreateChannel {
        /// Its name, without `#`.
        name: String,
    },
    /// Joins a channel.
    JoinChannel {
        /// Its name, without `#`.
        name: String,
    },
    /// Leaves a channel.
    LeaveChannel {
        /// Its name, without `#`.
        name: String,
    },
    /// Installs the hub's version of the app, then restarts. The app does this, not the desk.
    Update,
    /// Asks for the person's places.
    Places,
    /// Asks for checkouts worth sharing, found under `dev` folders.
    Suggestions,
    /// Shares a checkout.
    AddFolder {
        /// Where it is: a Windows path, or a Linux path in a WSL distro.
        path: String,
        /// Which system it is on.
        runs_in: RunsIn,
    },
    /// Stops sharing a checkout.
    RemoveFolder {
        /// Its name.
        name: PlaceName,
    },
    /// Sets where `@me` runs by default.
    SetMachineRunsIn {
        /// Windows, or a WSL distro.
        runs_in: RunsIn,
    },
}

/// The desk's answer to one [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The answer to [`Request::Hello`].
    Hello {
        /// The version the hub, the app and the desk all share.
        version: String,
    },
    /// The answer to [`Request::Setup`].
    Setup {
        /// The name this machine joined under, once it has.
        joined: Option<PersonName>,
        /// The name to offer, from the Windows user name.
        suggested_name: Option<String>,
    },
    /// The answer to [`Request::Overview`].
    Overview(Overview),
    /// The answer to [`Request::Chat`].
    Chat(ChatView),
    /// The answer to [`Request::Places`].
    Places(PlacesView),
    /// The answer to [`Request::Suggestions`].
    Suggestions {
        /// Checkouts not shared yet.
        folders: Vec<Folder>,
    },
    /// The request was done, and there is nothing more to say.
    Done,
}

/// Something the desk tells the window without being asked: what to ask for again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The overview changed.
    Overview,
    /// A chat changed.
    Chat {
        /// Which.
        chat: ChatId,
    },
    /// Something worth a notification arrived: a direct message, or a tag of the person or
    /// their places. The app shows it while the window is not in front.
    Notify {
        /// Where.
        chat: ChatId,
        /// Who, and where.
        title: String,
        /// The start of what they said.
        body: String,
    },
    /// The person clicked a notification: the window opens its chat.
    Open {
        /// Which.
        chat: ChatId,
    },
}

/// How the desk stands with the hub.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Connection {
    /// This machine has not joined a hub.
    NotJoined,
    /// Connecting, or loading what is there.
    Connecting,
    /// Up to date, and hearing new events as they come.
    Connected,
    /// Not connected; trying again.
    Offline {
        /// Why, in words.
        reason: String,
    },
    /// The hub runs another version, so the app must change before anything else.
    WrongVersion {
        /// The hub's version.
        hub: String,
    },
}

/// The person's places.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
pub struct PlacesView {
    /// Where `@me` runs by default.
    pub machine_runs_in: RunsIn,
    /// The checkouts shared.
    pub folders: Vec<Folder>,
    /// The WSL distros installed, none without WSL.
    pub distros: Vec<String>,
}

/// The chat list, the people and how the connection stands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
pub struct Overview {
    /// Who this machine joined as.
    pub me: Option<PersonName>,
    /// How the desk stands with the hub.
    pub connection: Connection,
    /// The chats to list: the person's channels, then a direct chat with everyone else.
    pub chats: Vec<ChatSummary>,
    /// Everyone who has joined.
    pub people: Vec<Person>,
    /// Every channel, joined or not.
    pub channels: Vec<ChannelInfo>,
}

/// One line of the chat list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
pub struct ChatSummary {
    /// The chat.
    pub chat: ChatId,
    /// What to call it: `#name`, or the other person's name.
    pub title: String,
    /// Messages from others since the person last read it.
    pub unread: u32,
    /// For a direct chat, whether the other person is here.
    pub here: Option<bool>,
}

/// One chat, or one thread of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
pub struct ChatView {
    /// The chat.
    pub chat: ChatId,
    /// What to call it.
    pub title: String,
    /// Who is in it.
    pub members: Vec<PersonName>,
    /// The thread shown, by its first message, or none for the top level.
    pub thread: Option<EventId>,
    /// Its messages, oldest first.
    pub items: Vec<Item>,
    /// Whether the hub has older events than these.
    pub has_older: bool,
    /// The first message the person has not read, if any.
    pub first_unread: Option<EventId>,
}

/// One message as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
pub struct Item {
    /// Its id.
    pub id: EventId,
    /// Who wrote it.
    pub author: PersonName,
    /// When, in Unix milliseconds.
    // A `number` in TypeScript, as the event's `ts` is.
    #[specta(type = i32)]
    pub ts: i64,
    /// What it says. The window draws it as text.
    pub text: String,
    /// The places it tags.
    pub tags: Vec<Tag>,
    /// How many replies its thread has.
    pub replies: u32,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use specta::Types;
    use specta_typescript::Typescript;

    use super::{Event, Request, Response};

    const HEADER: &str = "// Written by the desk's tests from crates/desk/src/ui.rs.";

    /// Rewrites `ui/src/desk.ts` when it is out of date, and fails so that the change is seen.
    #[test]
    fn the_window_has_the_desks_types() {
        let types = Types::default()
            .register::<Request>()
            .register::<Response>()
            .register::<Event>();
        let written = Typescript::default()
            .header(HEADER)
            .export(&types, specta_serde::Format)
            .expect("the desk's types can be written as TypeScript");
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/desk.ts");
        let found = fs::read_to_string(&path).unwrap_or_default();
        if found != written {
            fs::write(&path, &written).expect("ui/src/desk.ts can be written");
            panic!("ui/src/desk.ts was out of date; it has been rewritten, so commit it");
        }
    }
}
