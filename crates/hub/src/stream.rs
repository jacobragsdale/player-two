//! `GET /stream`: each person's open line to the hub, carrying their events and everyone's
//! presence as it changes.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event as SseEvent, KeepAlive, KeepAliveStream, Sse};
use player_two_core::api::Presence;
use player_two_core::event::{Event, EventId};
use player_two_core::names::{ChatId, PersonName};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::app::{AppState, Caller};
use crate::store;

/// How often an idle stream sends a comment, so that nothing between ends it for being quiet.
const KEEP_ALIVE: Duration = Duration::from_secs(20);

/// What every open stream hears about.
#[derive(Debug, Clone)]
enum Live {
    Event(Arc<Event>),
    Presence(Presence),
    /// The person's token was reset: their streams end.
    Close(PersonName),
}

/// The open streams: who has one, and the channel that feeds them all.
#[derive(Debug)]
pub struct Streams {
    sender: broadcast::Sender<Live>,
    open: Mutex<HashMap<PersonName, usize>>,
}

impl Default for Streams {
    fn default() -> Self {
        Self {
            sender: broadcast::channel(1024).0,
            open: Mutex::new(HashMap::new()),
        }
    }
}

impl Streams {
    /// Sends a new event to every stream that may see it.
    pub fn publish(&self, event: Event) {
        // Nobody listening is not a failure.
        let _ = self.sender.send(Live::Event(Arc::new(event)));
    }

    /// Ends every stream of `name`.
    pub fn close(&self, name: &PersonName) {
        let _ = self.sender.send(Live::Close(name.clone()));
    }

    /// Whether `name` has a stream open.
    #[must_use]
    pub fn is_here(&self, name: &PersonName) -> bool {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .is_some_and(|count| *count > 0)
    }

    /// Counts a stream of `name` opening, and tells everyone when it is their first.
    fn arrive(&self, name: &PersonName) {
        let first = {
            let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            let count = open.entry(name.clone()).or_insert(0);
            *count += 1;
            *count == 1
        };
        if first {
            let _ = self.sender.send(Live::Presence(Presence {
                name: name.clone(),
                here: true,
            }));
        }
    }

    /// Counts a stream of `name` closing. True when it was their last.
    fn leave(&self, name: &PersonName) -> bool {
        let last = {
            let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            let count = open.entry(name.clone()).or_insert(1);
            *count = count.saturating_sub(1);
            *count == 0
        };
        if last {
            let _ = self.sender.send(Live::Presence(Presence {
                name: name.clone(),
                here: false,
            }));
        }
        last
    }
}

/// Keeps a person counted as here while their stream's task runs, and records when they left.
struct Here {
    state: AppState,
    name: PersonName,
}

impl Drop for Here {
    fn drop(&mut self) {
        if self.state.streams.leave(&self.name) {
            let (db, name, now) = (
                self.state.db.clone(),
                self.name.clone(),
                self.state.clock.now(),
            );
            tokio::spawn(async move {
                let recorded = db
                    .call(move |connection| {
                        connection.execute(
                            "UPDATE people SET last_seen = ?1 WHERE name = ?2",
                            (now, name.as_str()),
                        )?;
                        Ok(())
                    })
                    .await;
                if let Err(error) = recorded {
                    eprintln!("error: cannot record when someone left: {error:#}");
                }
            });
        }
    }
}

type Lines = Sse<KeepAliveStream<ReceiverStream<Result<SseEvent, Infallible>>>>;

/// `GET /stream`.
pub async fn stream(
    State(state): State<AppState>,
    Caller(me): Caller,
    headers: HeaderMap,
) -> Lines {
    let after = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .map(EventId);
    // Listening starts before the backlog is read, so nothing falls between the two.
    let live = state.streams.sender.subscribe();
    let (lines, out) = mpsc::channel(64);
    tokio::spawn(feed(state, me, after, live, lines));
    Sse::new(ReceiverStream::new(out)).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}

/// Writes one person's stream until they hang up, fall too far behind, or are reset.
async fn feed(
    state: AppState,
    me: PersonName,
    after: Option<EventId>,
    mut live: broadcast::Receiver<Live>,
    lines: mpsc::Sender<Result<SseEvent, Infallible>>,
) {
    state.streams.arrive(&me);
    let _here = Here {
        state: state.clone(),
        name: me.clone(),
    };
    let mut sent = after.unwrap_or(EventId(0));
    if let Some(after) = after {
        let who = me.clone();
        let backlog = state
            .db
            .call(move |connection| store::events_after(connection, after, &who))
            .await;
        let Ok(backlog) = backlog else {
            return;
        };
        for event in backlog {
            sent = event.id;
            if lines.send(Ok(line(&event))).await.is_err() {
                return;
            }
        }
    }
    loop {
        // Hanging up is noticed at once, not only when there is next something to send.
        let received = tokio::select! {
            received = live.recv() => received,
            () = lines.closed() => return,
        };
        let next = match received {
            Ok(Live::Event(event)) if event.id > sent => {
                if !may_see(&state, &event.chat, &me).await {
                    continue;
                }
                sent = event.id;
                line(&event)
            }
            Ok(Live::Event(_)) => continue,
            Ok(Live::Presence(presence)) => SseEvent::default()
                .event("presence")
                .data(serde_json::to_string(&presence).unwrap_or_default()),
            Ok(Live::Close(name)) if name == me => return,
            Ok(Live::Close(_)) => continue,
            // Too far behind, or the hub is stopping: the desk reconnects and catches up.
            Err(_) => return,
        };
        if lines.send(Ok(next)).await.is_err() {
            return;
        }
    }
}

fn line(event: &Event) -> SseEvent {
    SseEvent::default()
        .id(event.id.to_string())
        .data(serde_json::to_string(event).unwrap_or_default())
}

async fn may_see(state: &AppState, chat: &ChatId, me: &PersonName) -> bool {
    let (chat, me) = (chat.clone(), me.clone());
    state
        .db
        .call(move |connection| store::may_use(connection, &chat, &me))
        .await
        .unwrap_or(false)
}
