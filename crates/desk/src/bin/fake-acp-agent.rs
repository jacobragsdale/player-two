//! A scripted ACP agent for the desk's tests, and nothing else: it acts on words in its prompt,
//! after the desk's preamble where there is one.
//!
//! - `read`: reads a file, without asking.
//! - `edit`: asks to edit a file, and edits it if allowed.
//! - `run`: asks to run a command, and runs it if allowed.
//! - `slow`: waits until it is cancelled, or for half a minute.
//! - `fail`: fails the turn.
//! - `sign in`: says it must be signed in.
//! - `echo`: answers with the whole prompt it was given.
//!
//! Every turn that does not fail ends with an answer saying what it did.
//!
//! It offers a way to sign in over ACP, `fake-login`, which always works. Started with
//! `--needs-sign-in <file>`, it will not open a session until signed in, which writes the file;
//! with `--no-sign-in`, it offers no way to sign in.

use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, AuthMethod, AuthMethodAgent, AuthenticateRequest, AuthenticateResponse,
    CancelNotification, ContentBlock, ContentChunk, Diff, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest, SessionNotification,
    SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Error, Stdio};
use tokio::sync::Notify;

const SESSION: &str = "fake-session";

fn text(words: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(words))
}

fn say(connection: &ConnectionTo<Client>, update: SessionUpdate) -> Result<(), Error> {
    connection.send_notification(SessionNotification::new(SESSION, update))
}

/// Asks whether a tool call may go ahead.
async fn allowed(
    connection: &ConnectionTo<Client>,
    call: &str,
    title: &str,
    kind: ToolKind,
) -> Result<bool, Error> {
    let options = vec![
        PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
        PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
    ];
    let fields = ToolCallUpdateFields::new()
        .kind(kind)
        .title(title.to_owned());
    let asked = RequestPermissionRequest::new(
        SESSION,
        ToolCallUpdate::new(call.to_owned(), fields),
        options,
    );
    let answer = connection.send_request(asked).block_task().await?;
    Ok(
        matches!(answer.outcome, RequestPermissionOutcome::Selected(selected) if selected.option_id.to_string() == "allow"),
    )
}

/// A tool call that asks first, and how it ended.
async fn asked_call(
    connection: &ConnectionTo<Client>,
    call: &str,
    title: &str,
    kind: ToolKind,
    raw: serde_json::Value,
    done: Vec<ToolCallContent>,
) -> Result<&'static str, Error> {
    say(
        connection,
        SessionUpdate::ToolCall(
            ToolCall::new(call.to_owned(), title)
                .kind(kind)
                .status(ToolCallStatus::Pending)
                .raw_input(raw),
        ),
    )?;
    let yes = allowed(connection, call, title, kind).await?;
    let fields = if yes {
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .content(done)
    } else {
        ToolCallUpdateFields::new().status(ToolCallStatus::Failed)
    };
    say(
        connection,
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(call.to_owned(), fields)),
    )?;
    Ok(if yes { "done" } else { "refused" })
}

/// One turn of the script.
async fn turn(
    connection: ConnectionTo<Client>,
    whole: String,
    cancelled: Arc<Notify>,
) -> Result<StopReason, Error> {
    let prompt = whole
        .rsplit_once("\n---\n")
        .map_or(whole.as_str(), |(_, request)| request)
        .to_owned();
    let mut did = Vec::new();
    if prompt.contains("sign in") {
        return Err(Error::auth_required());
    }
    if prompt.contains("fail") {
        return Err(Error::internal_error().data(serde_json::json!("something broke")));
    }
    if prompt.contains("read") {
        let call = ToolCall::new("read-1", "Read pip.conf")
            .kind(ToolKind::Read)
            .status(ToolCallStatus::Completed)
            .raw_input(serde_json::json!({"path": "pip.conf"}))
            .content(vec![ToolCallContent::from(text(
                "[global]\nproxy = http://old-proxy:8080",
            ))]);
        say(&connection, SessionUpdate::ToolCall(call))?;
        did.push("read pip.conf".to_owned());
    }
    if prompt.contains("edit") {
        let diff = ToolCallContent::Diff(
            Diff::new("pip.conf", "proxy = http://proxy:8080")
                .old_text("proxy = http://old-proxy:8080".to_owned()),
        );
        let ended = asked_call(
            &connection,
            "edit-1",
            "Edit pip.conf",
            ToolKind::Edit,
            serde_json::json!({"path": "pip.conf"}),
            vec![diff],
        )
        .await?;
        did.push(format!("edit {ended}"));
    }
    if prompt.contains("run") {
        let command = "pip config set global.proxy http://proxy:8080";
        let output = ToolCallContent::from(text("Writing to pip.conf"));
        let ended = asked_call(
            &connection,
            "run-1",
            command,
            ToolKind::Execute,
            serde_json::json!({"command": command}),
            vec![output],
        )
        .await?;
        did.push(format!("run {ended}"));
    }
    if prompt.contains("slow") {
        let stopped = tokio::time::timeout(Duration::from_secs(30), cancelled.notified())
            .await
            .is_ok();
        if stopped {
            return Ok(StopReason::Cancelled);
        }
    }
    let mut answer = if did.is_empty() {
        "Nothing to do.".to_owned()
    } else {
        format!("Did: {}.", did.join(", "))
    };
    if prompt.contains("echo") {
        answer = format!("{answer}\n\n{whole}");
    }
    say(
        &connection,
        SessionUpdate::AgentMessageChunk(ContentChunk::new(text(&answer))),
    )?;
    Ok(StopReason::EndTurn)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let cancelled = Arc::new(Notify::new());
    let on_cancel = Arc::clone(&cancelled);
    let args: Vec<String> = std::env::args().collect();
    let offers_sign_in = !args.iter().any(|arg| arg == "--no-sign-in");
    // The file that says it was signed in, when it must be before a session opens.
    let signed_in = args
        .iter()
        .position(|arg| arg == "--needs-sign-in")
        .and_then(|at| args.get(at + 1))
        .map(std::path::PathBuf::from);
    let (signing_in, opening) = (signed_in.clone(), signed_in);
    Agent
        .builder()
        .name("fake-acp-agent")
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                let methods = if offers_sign_in {
                    vec![AuthMethod::Agent(AuthMethodAgent::new(
                        "fake-login",
                        "Fake login",
                    ))]
                } else {
                    Vec::new()
                };
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new())
                        .auth_methods(methods),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_authenticate: AuthenticateRequest, responder, _connection| {
                if let Some(file) = &signing_in {
                    std::fs::write(file, "signed in").map_err(|_| Error::internal_error())?;
                }
                responder.respond(AuthenticateResponse::new())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_new: NewSessionRequest, responder, _connection| {
                if opening.as_ref().is_some_and(|file| !file.exists()) {
                    return responder.respond_with_error(Error::auth_required());
                }
                responder.respond(NewSessionResponse::new(SESSION))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |prompt: PromptRequest, responder, connection: ConnectionTo<Client>| {
                let words: String = prompt
                    .prompt
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let cancelled = Arc::clone(&cancelled);
                let working = connection.clone();
                // The turn runs off the dispatch loop, which must stay free to carry its answers.
                connection.spawn(async move {
                    match turn(working, words, cancelled).await {
                        Ok(stop) => responder.respond(PromptResponse::new(stop)),
                        Err(error) => responder.respond_with_error(error),
                    }
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_cancel: CancelNotification, _connection| {
                on_cancel.notify_waiters();
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await
}
