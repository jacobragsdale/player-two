//! Who may post what: the "who may post it" column of `docs/plan/protocol.md`'s events table.

use player_two_core::api::NewEvent;
use player_two_core::event::{Body, Event, EventId, Message, RunAccepted, StepsBy};
use player_two_core::names::{PersonName, PlaceName};
use rusqlite::Connection;

use crate::store;

/// Whether an event may be posted, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// It may.
    Allowed,
    /// It may not, for this reason.
    Refused(String),
}

fn refused(reason: &str) -> anyhow::Result<Verdict> {
    Ok(Verdict::Refused(reason.to_owned()))
}

/// Decides whether `author` may post `new`.
pub fn may_post(
    connection: &Connection,
    new: &NewEvent,
    author: &PersonName,
) -> anyhow::Result<Verdict> {
    if let Err(problem) = new.body.check() {
        return Ok(Verdict::Refused(problem));
    }
    if !store::may_use(connection, &new.chat, author)? {
        return refused("you are not in that chat");
    }
    // An event may only point at events of its own chat.
    let in_chat = |id: EventId| -> anyhow::Result<Option<Event>> {
        Ok(store::event(connection, id)?.filter(|event| event.chat == new.chat))
    };
    if let Some(thread) = new.thread {
        let root = in_chat(thread)?;
        if !root.is_some_and(|root| root.thread.is_none() && matches!(root.body, Body::Message(_)))
        {
            return refused("that thread does not start with a message in this chat");
        }
    }
    let request = match new.body.request() {
        Some(id) => match in_chat(id)? {
            Some(
                event @ Event {
                    body: Body::Message(_),
                    ..
                },
            ) => Some(event),
            _ => return refused("that request is not a message in this chat"),
        },
        None => None,
    };
    let run = match new.body.run() {
        Some(id) => match in_chat(id)? {
            Some(Event {
                author,
                body: Body::RunAccepted(accepted),
                ..
            }) => Some((author, accepted)),
            _ => return refused("that run did not start in this chat"),
        },
        None => None,
    };
    let tags = |event: &Event| match &event.body {
        Body::Message(Message { tags, .. }) => tags.clone(),
        _ => Vec::new(),
    };
    let allowed = match (&new.body, request, run) {
        (Body::Message(_), _, _) => true,
        (Body::RunAccepted(accepted), Some(request), _) => {
            tags(&request)
                .iter()
                .any(|tag| &tag.person == author && tag.place == accepted.place)
                || follows_run_of(connection, &request, author, Some(&accepted.place))?
        }
        (Body::RunDeclined(_), Some(request), _) => {
            tags(&request).iter().any(|tag| &tag.person == author)
                || follows_run_of(connection, &request, author, None)?
        }
        (Body::RunWithdrawn(_), Some(request), _) => &request.author == author,
        (Body::RunAnswer(_), _, Some((owner, accepted))) => {
            &owner == author || answered_by_asker(connection, &accepted, author)?
        }
        (Body::RunStop(_), _, Some((_, accepted))) => asked_for(connection, &accepted, author)?,
        (_, _, Some((owner, _))) => &owner == author,
        _ => false,
    };
    if allowed {
        Ok(Verdict::Allowed)
    } else {
        refused("only the place's owner, or for some kinds whoever asked, may post that")
    }
}

/// Whether `request` is a follow-up of a run `owner` let go at `place` (any place, when none is
/// given): a message in that run's thread, written by whoever asked for the run.
fn follows_run_of(
    connection: &Connection,
    request: &Event,
    owner: &PersonName,
    place: Option<&PlaceName>,
) -> anyhow::Result<bool> {
    let Some(thread) = request.thread else {
        return Ok(false);
    };
    let mut statement = connection.prepare(&format!(
        "SELECT {} FROM events WHERE chat = ?1 AND thread = ?2 AND kind = 'run.accepted' AND author = ?3 AND id < ?4",
        store::EVENT_COLUMNS
    ))?;
    let mut rows = statement.query((
        request.chat.to_string(),
        thread.0,
        owner.as_str(),
        request.id.0,
    ))?;
    while let Some(row) = rows.next()? {
        let event = store::read_event(row)?;
        let Body::RunAccepted(accepted) = &event.body else {
            continue;
        };
        if place.is_some_and(|place| place != &accepted.place) {
            continue;
        }
        if store::event(connection, accepted.request)?
            .is_some_and(|asked| asked.author == request.author)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `author` asked for the run and its owner handed them the steps.
fn answered_by_asker(
    connection: &Connection,
    accepted: &RunAccepted,
    author: &PersonName,
) -> anyhow::Result<bool> {
    if accepted.steps_by != StepsBy::Asker {
        return Ok(false);
    }
    asked_for(connection, accepted, author)
}

/// Whether `author` wrote the request the run answers.
fn asked_for(
    connection: &Connection,
    accepted: &RunAccepted,
    author: &PersonName,
) -> anyhow::Result<bool> {
    Ok(
        store::event(connection, accepted.request)?
            .is_some_and(|request| &request.author == author),
    )
}
