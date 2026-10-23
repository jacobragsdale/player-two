//! Events: everything said or done in a chat, as the hub stores and relays it.

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::names::{ChatId, PersonName, PlaceName};

/// The most characters a message or an agent's answer may hold.
pub const MAX_TEXT: usize = 64_000;

/// An event's number on the hub. Later events have larger numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub i64);

// TypeScript sees a `number`: ids stay far below 2^53, where a number stops being exact.
impl Type for EventId {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <i32 as Type>::definition(types)
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One event, as the hub stored it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Event {
    /// The hub's number for it.
    pub id: EventId,
    /// The chat it belongs to.
    pub chat: ChatId,
    /// The first message of the thread it is in, if it is in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<EventId>,
    /// Who posted it.
    pub author: PersonName,
    /// When the hub stored it, in Unix milliseconds by the hub's clock.
    // A `number` in TypeScript, exact for the next 280,000 years.
    #[specta(type = i32)]
    pub ts: i64,
    /// What it says.
    #[serde(flatten)]
    pub body: Body,
}

/// What an event says, by its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "body")]
pub enum Body {
    /// Something a person wrote.
    #[serde(rename = "message")]
    Message(Message),
    /// The owner let a request run.
    #[serde(rename = "run.accepted")]
    RunAccepted(RunAccepted),
    /// The owner declined a request.
    #[serde(rename = "run.declined")]
    RunDeclined(RunDeclined),
    /// The asker took their request back.
    #[serde(rename = "run.withdrawn")]
    RunWithdrawn(RunWithdrawn),
    /// What the agent did.
    #[serde(rename = "run.activity")]
    RunActivity(RunActivity),
    /// A step waiting for a yes.
    #[serde(rename = "run.prompt")]
    RunPrompt(RunPrompt),
    /// The yes or no to a step.
    #[serde(rename = "run.answer")]
    RunAnswer(RunAnswer),
    /// The agent's answer.
    #[serde(rename = "run.result")]
    RunResult(RunResult),
    /// The run is over.
    #[serde(rename = "run.ended")]
    RunEnded(RunEnded),
    /// The asker wants the run stopped.
    #[serde(rename = "run.stop")]
    RunStop(RunStop),
}

impl Body {
    /// The kind, as the wire names it.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Message(_) => "message",
            Self::RunAccepted(_) => "run.accepted",
            Self::RunDeclined(_) => "run.declined",
            Self::RunWithdrawn(_) => "run.withdrawn",
            Self::RunActivity(_) => "run.activity",
            Self::RunPrompt(_) => "run.prompt",
            Self::RunAnswer(_) => "run.answer",
            Self::RunResult(_) => "run.result",
            Self::RunEnded(_) => "run.ended",
            Self::RunStop(_) => "run.stop",
        }
    }

    /// The run this event reports on, for the kinds that report on one.
    #[must_use]
    pub fn run(&self) -> Option<EventId> {
        match self {
            Self::RunActivity(RunActivity { run, .. })
            | Self::RunPrompt(RunPrompt { run, .. })
            | Self::RunAnswer(RunAnswer { run, .. })
            | Self::RunResult(RunResult { run, .. })
            | Self::RunEnded(RunEnded { run, .. })
            | Self::RunStop(RunStop { run }) => Some(*run),
            Self::Message(_)
            | Self::RunAccepted(_)
            | Self::RunDeclined(_)
            | Self::RunWithdrawn(_) => None,
        }
    }

    /// The request this event answers, for the kinds that answer one.
    #[must_use]
    pub fn request(&self) -> Option<EventId> {
        match self {
            Self::RunAccepted(RunAccepted { request, .. })
            | Self::RunDeclined(RunDeclined { request })
            | Self::RunWithdrawn(RunWithdrawn { request }) => Some(*request),
            _ => None,
        }
    }

    /// Checks what no reader should have to cope with: text past [`MAX_TEXT`].
    pub fn check(&self) -> Result<(), String> {
        let text = match self {
            Self::Message(message) => &message.text,
            Self::RunResult(result) => &result.text,
            _ => return Ok(()),
        };
        if text.chars().count() > MAX_TEXT {
            Err(format!("text is longer than {MAX_TEXT} characters"))
        } else {
            Ok(())
        }
    }
}

/// Something a person wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Message {
    /// The text, as typed.
    pub text: String,
    /// The places it tags, as the sender's desk read them from the text.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// One tag in a message: `@person` or `@person/place`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct Tag {
    /// Whose place.
    pub person: PersonName,
    /// Which place: `machine` for `@person`.
    pub place: PlaceName,
}

/// How much a run may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub enum Mode {
    /// It may read and search, and nothing else.
    #[serde(rename = "read-only")]
    ReadOnly,
    /// Each edit or command waits for a yes.
    #[serde(rename = "ask")]
    Ask,
    /// It may do anything it asks to.
    #[serde(rename = "full")]
    Full,
}

/// Who answers a run's steps in ask mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum StepsBy {
    /// The machine's owner.
    Owner,
    /// Whoever asked.
    Asker,
}

/// Where on a machine a run happens: `windows`, or `wsl:<distro>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(type = String)]
pub enum RunsIn {
    /// On Windows itself.
    Windows,
    /// In a WSL distro, by its name.
    Wsl(String),
}

impl TryFrom<String> for RunsIn {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if text == "windows" {
            return Ok(Self::Windows);
        }
        match text.strip_prefix("wsl:") {
            Some(distro) if !distro.is_empty() => Ok(Self::Wsl(distro.to_owned())),
            _ => Err(format!("{text:?} is neither windows nor wsl:<distro>")),
        }
    }
}

impl fmt::Display for RunsIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows => f.write_str("windows"),
            Self::Wsl(distro) => write!(f, "wsl:{distro}"),
        }
    }
}

impl From<RunsIn> for String {
    fn from(runs_in: RunsIn) -> String {
        runs_in.to_string()
    }
}

/// The owner let a request run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunAccepted {
    /// The message being answered.
    pub request: EventId,
    /// The place it runs at.
    pub place: PlaceName,
    /// Where on the machine.
    pub runs_in: RunsIn,
    /// How much it may do.
    pub mode: Mode,
    /// Who answers its steps.
    pub steps_by: StepsBy,
    /// Whether the asker's follow-ups in the thread run without a new click.
    pub keep_going: bool,
}

/// The owner declined a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunDeclined {
    /// The message declined.
    pub request: EventId,
}

/// The asker took their request back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunWithdrawn {
    /// The message taken back.
    pub request: EventId,
}

/// What the agent did since the last report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunActivity {
    /// The run, by its `run.accepted`.
    pub run: EventId,
    /// What it did, in order. An item with an `id` seen before replaces that one.
    pub items: Vec<Activity>,
}

/// One thing an agent did, as verb, object and outcome: "edited", "src/main.rs", "+3 -1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Activity {
    /// The agent's own id for it, so a later report can update it.
    pub id: String,
    /// What it did.
    pub verb: String,
    /// What to.
    pub object: String,
    /// How it went, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// What it printed or read, shown to everyone in the chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// A step waiting for a yes, in ask mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunPrompt {
    /// The run, by its `run.accepted`.
    pub run: EventId,
    /// The desk's id for this step.
    pub prompt: String,
    /// What the agent wants to do.
    pub what: String,
}

/// The yes or no to a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunAnswer {
    /// The run, by its `run.accepted`.
    pub run: EventId,
    /// The step answered.
    pub prompt: String,
    /// Whether it may go ahead.
    pub allow: bool,
}

/// The agent's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunResult {
    /// The run, by its `run.accepted`.
    pub run: EventId,
    /// The answer, as Markdown.
    pub text: String,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Ending {
    /// It finished.
    Done,
    /// Its owner, or its asker, stopped it.
    Stopped,
    /// It failed.
    Failed,
}

/// The run is over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunEnded {
    /// The run, by its `run.accepted`.
    pub run: EventId,
    /// How.
    pub how: Ending,
    /// Why, when it failed or was stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The asker wants a run stopped. The owner's desk stops it as if the owner had pressed Stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RunStop {
    /// The run, by its `run.accepted`.
    pub run: EventId,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        Activity, Body, Ending, Event, EventId, MAX_TEXT, Message, Mode, RunAccepted, RunActivity,
        RunAnswer, RunDeclined, RunEnded, RunPrompt, RunResult, RunStop, RunWithdrawn, RunsIn,
        StepsBy, Tag,
    };
    use crate::api::NewEvent;
    use crate::names::{ChatId, PersonName, PlaceName};

    fn person(name: &str) -> PersonName {
        PersonName::new(name).expect("a name")
    }

    /// Every kind's JSON, as `docs/plan/protocol.md` gives it.
    #[test]
    fn each_kind_has_the_protocols_shape() {
        let run = EventId(7);
        let cases: Vec<(Body, Value)> = vec![
            (
                Body::Message(Message {
                    text: "@bob fix pip".to_owned(),
                    tags: vec![Tag {
                        person: person("bob"),
                        place: PlaceName::machine(),
                    }],
                }),
                json!({"kind": "message", "body": {"text": "@bob fix pip", "tags": [{"person": "bob", "place": "machine"}]}}),
            ),
            (
                Body::RunAccepted(RunAccepted {
                    request: EventId(6),
                    place: PlaceName::machine(),
                    runs_in: RunsIn::Wsl("Ubuntu".to_owned()),
                    mode: Mode::ReadOnly,
                    steps_by: StepsBy::Asker,
                    keep_going: true,
                }),
                json!({"kind": "run.accepted", "body": {"request": 6, "place": "machine", "runs_in": "wsl:Ubuntu", "mode": "read-only", "steps_by": "asker", "keep_going": true}}),
            ),
            (
                Body::RunDeclined(RunDeclined {
                    request: EventId(6),
                }),
                json!({"kind": "run.declined", "body": {"request": 6}}),
            ),
            (
                Body::RunWithdrawn(RunWithdrawn {
                    request: EventId(6),
                }),
                json!({"kind": "run.withdrawn", "body": {"request": 6}}),
            ),
            (
                Body::RunActivity(RunActivity {
                    run,
                    items: vec![Activity {
                        id: "t1".to_owned(),
                        verb: "ran".to_owned(),
                        object: "pip config list".to_owned(),
                        outcome: Some("exit 0".to_owned()),
                        output: None,
                    }],
                }),
                json!({"kind": "run.activity", "body": {"run": 7, "items": [{"id": "t1", "verb": "ran", "object": "pip config list", "outcome": "exit 0"}]}}),
            ),
            (
                Body::RunPrompt(RunPrompt {
                    run,
                    prompt: "p1".to_owned(),
                    what: "Run pip config set".to_owned(),
                }),
                json!({"kind": "run.prompt", "body": {"run": 7, "prompt": "p1", "what": "Run pip config set"}}),
            ),
            (
                Body::RunAnswer(RunAnswer {
                    run,
                    prompt: "p1".to_owned(),
                    allow: false,
                }),
                json!({"kind": "run.answer", "body": {"run": 7, "prompt": "p1", "allow": false}}),
            ),
            (
                Body::RunResult(RunResult {
                    run,
                    text: "Done.".to_owned(),
                }),
                json!({"kind": "run.result", "body": {"run": 7, "text": "Done."}}),
            ),
            (
                Body::RunEnded(RunEnded {
                    run,
                    how: Ending::Stopped,
                    reason: None,
                }),
                json!({"kind": "run.ended", "body": {"run": 7, "how": "stopped"}}),
            ),
            (
                Body::RunStop(RunStop { run }),
                json!({"kind": "run.stop", "body": {"run": 7}}),
            ),
        ];
        for (body, expected) in cases {
            let written = serde_json::to_value(&body).expect("a body serializes");
            assert_eq!(written, expected, "{}", body.kind());
            assert_eq!(written["kind"], body.kind());
            let read: Body = serde_json::from_value(expected).expect("a body deserializes");
            assert_eq!(read, body);
        }
    }

    #[test]
    fn an_event_carries_its_envelope_beside_its_kind() {
        let event = Event {
            id: EventId(3),
            chat: ChatId::try_from("#ops".to_owned()).expect("a chat"),
            thread: None,
            author: person("alice"),
            ts: 1_700_000_000_000,
            body: Body::RunDeclined(RunDeclined {
                request: EventId(2),
            }),
        };
        let expected = json!({"id": 3, "chat": "#ops", "author": "alice", "ts": 1_700_000_000_000_i64, "kind": "run.declined", "body": {"request": 2}});
        assert_eq!(serde_json::to_value(&event).expect("serializes"), expected);
        let read: Event = serde_json::from_value(expected).expect("deserializes");
        assert_eq!(read, event);

        let posted: NewEvent = serde_json::from_value(
            json!({"chat": "alice+bob", "thread": 9, "kind": "message", "body": {"text": "hi"}}),
        )
        .expect("a new event deserializes");
        assert_eq!(posted.thread, Some(EventId(9)));
        assert_eq!(
            posted.body,
            Body::Message(Message {
                text: "hi".to_owned(),
                tags: vec![]
            })
        );
    }

    #[test]
    fn unknown_kinds_and_bad_values_are_refused() {
        for bad in [
            json!({"kind": "message.edit", "body": {}}),
            json!({"kind": "run.accepted", "body": {"request": 1, "place": "machine", "runs_in": "mac", "mode": "ask", "steps_by": "owner", "keep_going": false}}),
            json!({"kind": "run.accepted", "body": {"request": 1, "place": "machine", "runs_in": "windows", "mode": "sudo", "steps_by": "owner", "keep_going": false}}),
            json!({"kind": "message", "body": {"text": "hi", "tags": [{"person": "Bob", "place": "machine"}]}}),
        ] {
            assert!(
                serde_json::from_value::<Body>(bad.clone()).is_err(),
                "{bad} is refused"
            );
        }
    }

    #[test]
    fn text_has_a_limit() {
        let message = |length: usize| {
            Body::Message(Message {
                text: "é".repeat(length),
                tags: vec![],
            })
        };
        assert!(message(MAX_TEXT).check().is_ok());
        assert!(message(MAX_TEXT + 1).check().is_err());
    }
}
