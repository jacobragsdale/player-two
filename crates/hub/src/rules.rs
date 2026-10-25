//! Who may post what: the "who may post it" column of `docs/plan/protocol.md`'s events table.

use player_two_core::api::NewEvent;
use player_two_core::event::{Body, Event, EventId, Message, RunAccepted};
use player_two_core::names::PersonName;
use tokio_postgres::GenericClient;

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
pub async fn may_post(
    client: &impl GenericClient,
    new: &NewEvent,
    author: &PersonName,
) -> anyhow::Result<Verdict> {
    if let Err(problem) = new.body.check() {
        return Ok(Verdict::Refused(problem));
    }
    if !store::may_use(client, &new.chat, author).await? {
        return refused("you are not in that chat");
    }
    // An event may only point at events of its own chat.
    let in_chat = async |id: EventId| -> anyhow::Result<Option<Event>> {
        Ok(store::event(client, id)
            .await?
            .filter(|event| event.chat == new.chat))
    };
    if let Some(thread) = new.thread {
        let root = in_chat(thread).await?;
        if !root.is_some_and(|root| root.thread.is_none() && matches!(root.body, Body::Message(_)))
        {
            return refused("that thread does not start with a message in this chat");
        }
    }
    let request = match new.body.request() {
        Some(id) => match in_chat(id).await? {
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
        Some(id) => match in_chat(id).await? {
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
        (Body::RunAccepted(accepted), Some(request), _) => tags(&request)
            .iter()
            .any(|tag| &tag.person == author && tag.place == accepted.place),
        (Body::RunDeclined(_), Some(request), _) => {
            tags(&request).iter().any(|tag| &tag.person == author)
        }
        (Body::RunWithdrawn(_), Some(request), _) => &request.author == author,
        (Body::RunStop(_), _, Some((_, accepted))) => asked_for(client, &accepted, author).await?,
        (_, _, Some((owner, _))) => &owner == author,
        _ => false,
    };
    if allowed {
        Ok(Verdict::Allowed)
    } else {
        refused("only the place's owner, or for some kinds whoever asked, may post that")
    }
}

/// Whether `author` wrote the request the run answers.
async fn asked_for(
    client: &impl GenericClient,
    accepted: &RunAccepted,
    author: &PersonName,
) -> anyhow::Result<bool> {
    Ok(store::event(client, accepted.request)
        .await?
        .is_some_and(|request| &request.author == author))
}
