//! Talking to an agent over ACP: one session in a folder, turns sent to it one at a time, and what
//! it does reported as activity. A run its owner let go may do anything, so each permission the
//! agent still asks for is given, once.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AuthMethod, AuthenticateRequest, CancelNotification, ContentBlock, InitializeRequest,
    NewSessionRequest, PermissionOptionKind, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, ErrorCode};
use player_two_core::event::Activity;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::activity::Calls;
use crate::agents::{AgentKind, Found};
use crate::process::{self, Running};

/// How a turn ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// It finished, with the agent's answer.
    Answered(String),
    /// It was cancelled.
    Cancelled,
}

/// A turn waiting to be sent, and where its ending goes.
struct Turn {
    text: String,
    ended: oneshot::Sender<anyhow::Result<Ended>>,
}

/// An open session with an agent.
pub struct Session {
    turns: mpsc::Sender<Turn>,
    cancel: mpsc::Sender<()>,
    process: Arc<tokio::sync::Mutex<Running>>,
    kind: AgentKind,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// How to sign an agent in, for when it says it must be.
#[must_use]
pub fn sign_in(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Cursor => "run `agent login`",
        AgentKind::Copilot => "run `copilot`, then type /login",
        AgentKind::Claude => "run `claude`, then type /login",
    }
}

/// The agent says it must be signed in before it does anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedsSignIn(pub AgentKind);

impl std::fmt::Display for NeedsSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} needs signing in: press Sign in in Settings, or {}",
            self.0.label(),
            sign_in(self.0)
        )
    }
}

impl std::error::Error for NeedsSignIn {}

/// How signing in went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedIn {
    /// The agent signed in.
    Done,
    /// The agent offers no way to sign in over ACP; its own command does it.
    NotOffered,
}

/// How long signing in, in the browser the agent opened, may take.
const SIGNING_IN: Duration = Duration::from_secs(300);

/// Starts the agent and signs it in over ACP, with the way it offers: `cursor_login` for Cursor,
/// which opens the browser. An agent that offers none is left to its own command.
pub async fn sign_in_over_acp(agent: &Found, folder: &str) -> anyhow::Result<SignedIn> {
    let (mut running, stdin, stdout) = process::start(agent, folder)?;
    let kind = agent.kind;
    let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
    let signing = Client.connect_with(transport, async move |connection: ConnectionTo<Agent>| {
        let initialized = connection
            .send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task()
            .await?;
        // A terminal method is the agent's own command, which the person runs themselves.
        let offered: Vec<&AuthMethod> = initialized
            .auth_methods
            .iter()
            .filter(|method| matches!(method, AuthMethod::Agent(_)))
            .collect();
        let method = offered
            .iter()
            .find(|method| method.id().to_string() == "cursor_login")
            .or_else(|| offered.first());
        let Some(method) = method else {
            return Ok(SignedIn::NotOffered);
        };
        connection
            .send_request(AuthenticateRequest::new(method.id().clone()))
            .block_task()
            .await?;
        Ok(SignedIn::Done)
    });
    let signed = tokio::time::timeout(SIGNING_IN, signing).await;
    let last = running.last_errors();
    let _ = running.stop().await;
    match signed {
        Ok(Ok(signed)) => Ok(signed),
        Ok(Err(error)) => Err(explain(kind, &error, &last)),
        Err(_) => Err(anyhow::anyhow!(
            "{} was not signed in after five minutes",
            kind.label()
        )),
    }
}

/// What an agent's error says, in words for the person.
fn explain(
    kind: AgentKind,
    error: &agent_client_protocol::Error,
    last_errors: &[String],
) -> anyhow::Error {
    if error.code == ErrorCode::AuthRequired {
        return NeedsSignIn(kind).into();
    }
    let detail = error
        .data
        .as_ref()
        .map(|data| format!(" ({data})"))
        .unwrap_or_default();
    let printed = last_errors
        .last()
        .map(|line| format!("; it printed: {line}"))
        .unwrap_or_default();
    anyhow::anyhow!(
        "{} failed: {}{detail}{printed}",
        kind.label(),
        error.message
    )
}

impl Session {
    /// Starts the agent in `folder` and opens a session there. What the agent does is reported on
    /// `said`, for as long as the session lasts.
    pub async fn open(
        agent: &Found,
        folder: &str,
        said: mpsc::UnboundedSender<Activity>,
    ) -> anyhow::Result<Self> {
        let (running, stdin, stdout) = process::start(agent, folder)?;
        let process = Arc::new(tokio::sync::Mutex::new(running));
        let (turns, mut waiting) = mpsc::channel::<Turn>(1);
        let (cancel, mut cancelled) = mpsc::channel::<()>(1);
        let (opened, ready) = oneshot::channel::<anyhow::Result<()>>();
        let kind = agent.kind;
        let cwd = folder.to_owned();
        let calls = Arc::new(Mutex::new(Calls::default()));
        let answers = Arc::new(Mutex::new(String::new()));
        let (for_updates, for_turns) = (calls, Arc::clone(&answers));
        let errors = Arc::clone(&process);
        tokio::spawn(async move {
            let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
            let connected = Client
                .builder()
                .name("player-two")
                .on_receive_notification(
                    async move |notification: SessionNotification, _connection| {
                        let item = {
                            let mut calls = for_updates.lock().unwrap_or_else(PoisonError::into_inner);
                            match &notification.update {
                                SessionUpdate::ToolCall(call) => Some(calls.started(call)),
                                SessionUpdate::ToolCallUpdate(update) => Some(calls.updated(update)),
                                SessionUpdate::AgentMessageChunk(chunk) => {
                                    if let ContentBlock::Text(text) = &chunk.content {
                                        answers.lock().unwrap_or_else(PoisonError::into_inner).push_str(&text.text);
                                    }
                                    None
                                }
                                _ => None,
                            }
                        };
                        if let Some(item) = item {
                            // A run that has gone hears nothing more; the session is ending too.
                            let _ = said.send(item);
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _connection: ConnectionTo<Agent>| {
                        // Once is enough: an agent may keep an "always" in its own settings, past this run.
                        let wanted = [PermissionOptionKind::AllowOnce, PermissionOptionKind::AllowAlways];
                        let chosen = wanted.iter().find_map(|kind| request.options.iter().find(|option| option.kind == *kind));
                        let outcome = match chosen {
                            Some(option) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option.option_id.clone())),
                            None => RequestPermissionOutcome::Cancelled,
                        };
                        responder.respond(RequestPermissionResponse::new(outcome))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
                    let started = async {
                        connection.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await?;
                        let session = connection.send_request(NewSessionRequest::new(cwd)).block_task().await?;
                        Ok::<_, agent_client_protocol::Error>(session.session_id)
                    };
                    let session = match started.await {
                        Ok(session) => {
                            let _ = opened.send(Ok(()));
                            session
                        }
                        Err(error) => {
                            let last = errors.lock().await.last_errors();
                            let _ = opened.send(Err(explain(kind, &error, &last)));
                            return Ok(());
                        }
                    };
                    while let Some(turn) = waiting.recv().await {
                        for_turns.lock().unwrap_or_else(PoisonError::into_inner).clear();
                        let prompt = PromptRequest::new(session.clone(), vec![ContentBlock::Text(TextContent::new(turn.text))]);
                        let mut answer = std::pin::pin!(connection.send_request(prompt).block_task());
                        let response = loop {
                            tokio::select! {
                                response = &mut answer => break response,
                                Some(()) = cancelled.recv() => {
                                    let _ = connection.send_notification(CancelNotification::new(session.clone()));
                                }
                            }
                        };
                        let ended = match response {
                            Ok(response) if response.stop_reason == StopReason::Cancelled => Ok(Ended::Cancelled),
                            Ok(_) => Ok(Ended::Answered(std::mem::take(&mut *for_turns.lock().unwrap_or_else(PoisonError::into_inner)))),
                            Err(error) => {
                                let last = errors.lock().await.last_errors();
                                Err(explain(kind, &error, &last))
                            }
                        };
                        let _ = turn.ended.send(ended);
                    }
                    Ok(())
                })
                .await;
            if let Err(error) = connected {
                eprintln!("error: the agent's connection ended: {error}");
            }
        });
        match ready.await {
            Ok(Ok(())) => Ok(Self {
                turns,
                cancel,
                process,
                kind,
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                let last = process.lock().await.last_errors();
                let printed = last
                    .last()
                    .map(|line| format!(": {line}"))
                    .unwrap_or_default();
                Err(anyhow::anyhow!(
                    "{} stopped before it was ready{printed}",
                    kind.label()
                ))
            }
        }
    }

    /// Sends one turn and waits for it to end.
    pub async fn turn(&self, text: String) -> anyhow::Result<Ended> {
        let (ended, waited) = oneshot::channel();
        self.turns
            .send(Turn { text, ended })
            .await
            .map_err(|_| anyhow::anyhow!("{} has stopped", self.kind.label()))?;
        waited.await.map_err(|_| {
            anyhow::anyhow!("{} stopped in the middle of the turn", self.kind.label())
        })?
    }

    /// Asks the agent to stop the turn it is on.
    pub fn cancel(&self) {
        // A turn already over needs no cancelling.
        let _ = self.cancel.try_send(());
    }

    /// Stops the agent and everything it started.
    pub async fn stop(&self) -> anyhow::Result<()> {
        self.process.lock().await.stop().await
    }
}
