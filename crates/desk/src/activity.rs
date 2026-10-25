//! What an agent does, as the chat shows it: ACP's tool calls turned into activity items that
//! read as verb, object and outcome, each one changing in place as the agent reports more of it.

use std::collections::HashMap;

use agent_client_protocol::schema::v1::{
    ContentBlock, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolKind,
};
use player_two_core::event::Activity;

/// The most characters of what a tool call printed or changed that the chat is shown.
pub const OUTPUT: usize = 8_000;
/// The most characters of an object, on one line.
const OBJECT: usize = 200;

/// A tool call as described so far: each update says only what changed.
#[derive(Debug, Clone, Default)]
struct Call {
    kind: Option<ToolKind>,
    title: String,
    status: Option<ToolCallStatus>,
    content: Vec<ToolCallContent>,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
}

/// The tool calls of one session, kept so that each update can be read whole.
#[derive(Debug, Default)]
pub struct Calls {
    calls: HashMap<String, Call>,
}

impl Calls {
    /// Takes in a new tool call and gives back its item.
    pub fn started(&mut self, call: &ToolCall) -> Activity {
        let id = call.tool_call_id.to_string();
        let state = Call {
            kind: Some(call.kind),
            title: call.title.clone(),
            status: Some(call.status),
            content: call.content.clone(),
            raw_input: call.raw_input.clone(),
            raw_output: call.raw_output.clone(),
        };
        let item = item(&id, &state);
        self.calls.insert(id, state);
        item
    }

    /// Takes in an update to a tool call and gives back its item as it now stands.
    pub fn updated(&mut self, update: &ToolCallUpdate) -> Activity {
        let id = update.tool_call_id.to_string();
        let state = self.calls.entry(id.clone()).or_default();
        let fields = &update.fields;
        if let Some(kind) = fields.kind {
            state.kind = Some(kind);
        }
        if let Some(title) = &fields.title {
            state.title.clone_from(title);
        }
        if let Some(status) = fields.status {
            state.status = Some(status);
        }
        if let Some(content) = &fields.content {
            state.content.clone_from(content);
        }
        if fields.raw_input.is_some() {
            state.raw_input.clone_from(&fields.raw_input);
        }
        if fields.raw_output.is_some() {
            state.raw_output.clone_from(&fields.raw_output);
        }
        item(&id, state)
    }
}

/// The verb for a kind of tool call, as a person would put it; `None` for one named by its title.
fn verb(kind: Option<ToolKind>) -> Option<&'static str> {
    Some(match kind? {
        ToolKind::Read => "read",
        ToolKind::Edit => "edited",
        ToolKind::Delete => "deleted",
        ToolKind::Move => "moved",
        ToolKind::Search => "searched for",
        ToolKind::Execute => "ran",
        ToolKind::Fetch => "fetched",
        _ => return None,
    })
}

/// One line, cut to `most` characters.
fn one_line(text: &str, most: usize) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() > most {
        format!("{}…", line.chars().take(most).collect::<String>())
    } else {
        line.to_owned()
    }
}

/// What a tool call acts on: the command it runs, the file it touches, the words it looks for,
/// or the address it fetches, as its raw input names them; else its title.
fn object(call: &Call) -> String {
    let named = ["command", "path", "file_path", "query", "pattern", "url"]
        .iter()
        .find_map(|key| match call.raw_input.as_ref()?.get(key)? {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Array(words) => Some(
                words
                    .iter()
                    .filter_map(|word| word.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        });
    one_line(named.as_deref().unwrap_or(&call.title), OBJECT)
}

/// What a tool call printed or changed, cut to [`OUTPUT`] characters.
fn output(call: &Call) -> Option<String> {
    let mut parts = Vec::new();
    for content in &call.content {
        match content {
            ToolCallContent::Content(content) => {
                if let ContentBlock::Text(text) = &content.content {
                    parts.push(text.text.clone());
                }
            }
            ToolCallContent::Diff(diff) => {
                let mut lines = vec![format!("--- {}", diff.path.display())];
                lines.extend(
                    diff.old_text
                        .iter()
                        .flat_map(|old| old.lines().map(|line| format!("-{line}"))),
                );
                lines.extend(diff.new_text.lines().map(|line| format!("+{line}")));
                parts.push(lines.join("\n"));
            }
            _ => {}
        }
    }
    let joined = parts.join("\n");
    if joined.is_empty() {
        return None;
    }
    Some(if joined.chars().count() > OUTPUT {
        format!("{}\n…", joined.chars().take(OUTPUT).collect::<String>())
    } else {
        joined
    })
}

/// How a tool call went, once it has: done or failed, with the exit code when it says.
fn outcome(call: &Call) -> Option<String> {
    let exit = call
        .raw_output
        .as_ref()
        .and_then(|raw| raw.get("exit_code").or_else(|| raw.get("exitCode")))
        .and_then(serde_json::Value::as_i64);
    match call.status? {
        ToolCallStatus::Completed => {
            Some(exit.map_or_else(|| "done".to_owned(), |code| format!("exit {code}")))
        }
        ToolCallStatus::Failed => Some(exit.map_or_else(
            || "failed".to_owned(),
            |code| format!("failed, exit {code}"),
        )),
        _ => None,
    }
}

fn item(id: &str, call: &Call) -> Activity {
    let object = object(call);
    let verb = verb(call.kind).map_or_else(|| one_line(&call.title, OBJECT), str::to_owned);
    Activity {
        id: id.to_owned(),
        // A call named by its title alone has no separate object.
        object: if verb == object {
            String::new()
        } else {
            object
        },
        verb,
        outcome: outcome(call),
        output: output(call),
    }
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v1::{
        ContentBlock, Diff, TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
        ToolCallUpdateFields, ToolKind,
    };

    use super::Calls;

    #[test]
    fn a_command_reads_as_ran_its_command_and_how_it_ended() {
        let mut calls = Calls::default();
        let started = calls.started(
            &ToolCall::new("t1", "Terminal")
                .kind(ToolKind::Execute)
                .status(ToolCallStatus::InProgress)
                .raw_input(serde_json::json!({"command": "pip config list\nsecond line"})),
        );
        assert_eq!(
            (
                started.verb.as_str(),
                started.object.as_str(),
                started.outcome
            ),
            ("ran", "pip config list", None)
        );
        let fields = ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .raw_output(serde_json::json!({"exit_code": 0}))
            .content(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new("global.proxy=x"),
            ))]);
        let done = calls.updated(&ToolCallUpdate::new("t1", fields));
        assert_eq!(done.outcome.as_deref(), Some("exit 0"));
        assert_eq!(done.output.as_deref(), Some("global.proxy=x"));
    }

    #[test]
    fn an_edit_shows_its_diff() {
        let mut calls = Calls::default();
        let diff = ToolCallContent::Diff(Diff::new("pip.conf", "new").old_text("old".to_owned()));
        let edited = calls.started(
            &ToolCall::new("t2", "Edit pip.conf")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Completed)
                .content(vec![diff]),
        );
        assert_eq!(
            (edited.verb.as_str(), edited.object.as_str()),
            ("edited", "Edit pip.conf")
        );
        assert_eq!(edited.output.as_deref(), Some("--- pip.conf\n-old\n+new"));
    }
}
