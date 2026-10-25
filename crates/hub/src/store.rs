//! Reads and writes on the database, one function each, so the routes stay about HTTP.

use std::collections::HashMap;

use anyhow::Context;
use player_two_core::api::{ChannelInfo, PlaceInfo, PlaceKind};
use player_two_core::event::{Body, Event, EventId};
use player_two_core::names::{ChannelName, ChatId, PersonName, PlaceName};
use serde_json::{Value, json};
use tokio_postgres::{Client, GenericClient, Row};

/// Whether `name` has joined the hub.
pub async fn has_joined(client: &impl GenericClient, name: &PersonName) -> anyhow::Result<bool> {
    Ok(client
        .query_opt("SELECT 1 FROM people WHERE name = $1", &[&name.as_str()])
        .await?
        .is_some())
}

/// Every channel, with who is in it.
pub async fn channels(client: &impl GenericClient) -> anyhow::Result<Vec<ChannelInfo>> {
    let rows = client
        .query(
            "SELECT channels.name, members.name FROM channels
             LEFT JOIN members ON members.channel = channels.name
             ORDER BY channels.name, members.name",
            &[],
        )
        .await?;
    let mut channels: Vec<ChannelInfo> = Vec::new();
    for row in rows {
        let channel = ChannelName::new(row.try_get::<_, String>(0)?)?;
        let member: Option<String> = row.try_get(1)?;
        if channels.last().is_none_or(|last| last.name != channel) {
            channels.push(ChannelInfo {
                name: channel,
                members: Vec::new(),
            });
        }
        if let (Some(member), Some(last)) = (member, channels.last_mut()) {
            last.members.push(PersonName::new(member)?);
        }
    }
    Ok(channels)
}

/// Makes a channel with `by` in it. False when the name is taken.
pub async fn create_channel(
    client: &mut Client,
    name: &ChannelName,
    by: &PersonName,
    now: i64,
) -> anyhow::Result<bool> {
    let transaction = client.transaction().await?;
    let made = transaction
        .execute(
            "INSERT INTO channels (name, created_by, created) VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
            &[&name.as_str(), &by.as_str(), &now],
        )
        .await?;
    if made == 1 {
        transaction
            .execute(
                "INSERT INTO members (channel, name) VALUES ($1, $2)",
                &[&name.as_str(), &by.as_str()],
            )
            .await?;
    }
    transaction.commit().await?;
    Ok(made == 1)
}

/// Puts `who` in a channel. False when there is no such channel.
pub async fn join_channel(
    client: &impl GenericClient,
    name: &ChannelName,
    who: &PersonName,
) -> anyhow::Result<bool> {
    let exists = client
        .query_opt("SELECT 1 FROM channels WHERE name = $1", &[&name.as_str()])
        .await?
        .is_some();
    if exists {
        client
            .execute(
                "INSERT INTO members (channel, name) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                &[&name.as_str(), &who.as_str()],
            )
            .await?;
    }
    Ok(exists)
}

/// Takes `who` out of a channel.
pub async fn leave_channel(
    client: &impl GenericClient,
    name: &ChannelName,
    who: &PersonName,
) -> anyhow::Result<()> {
    client
        .execute(
            "DELETE FROM members WHERE channel = $1 AND name = $2",
            &[&name.as_str(), &who.as_str()],
        )
        .await?;
    Ok(())
}

/// Whether `who` may read and write `chat`: a channel they are in, or a direct chat of theirs
/// with someone who has joined.
pub async fn may_use(
    client: &impl GenericClient,
    chat: &ChatId,
    who: &PersonName,
) -> anyhow::Result<bool> {
    match chat {
        ChatId::Channel(name) => Ok(client
            .query_opt(
                "SELECT 1 FROM members WHERE channel = $1 AND name = $2",
                &[&name.as_str(), &who.as_str()],
            )
            .await?
            .is_some()),
        ChatId::Direct(one, other) => {
            let them = if one == who { other } else { one };
            Ok(chat.is_between(who) && has_joined(client, them).await?)
        }
    }
}

/// Stores an event and gives it back as readers will see it.
pub async fn insert_event(
    client: &impl GenericClient,
    chat: &ChatId,
    thread: Option<EventId>,
    author: &PersonName,
    body: &Body,
    now: i64,
) -> anyhow::Result<Event> {
    let mut written = serde_json::to_value(body)?;
    let content = written
        .get_mut("body")
        .map(Value::take)
        .context("an event's body is written beside its kind")?;
    let row = client
        .query_one(
            "INSERT INTO events (chat, thread, author, kind, body, ts)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            &[
                &chat.to_string(),
                &thread.map(|thread| thread.0),
                &author.as_str(),
                &body.kind(),
                &content.to_string(),
                &now,
            ],
        )
        .await?;
    Ok(Event {
        id: EventId(row.try_get(0)?),
        chat: chat.clone(),
        thread,
        author: author.clone(),
        ts: now,
        body: body.clone(),
    })
}

/// The columns [`read_event`] reads, in order.
pub const EVENT_COLUMNS: &str = "id, chat, thread, author, kind, body, ts";

/// Reads one row of [`EVENT_COLUMNS`] back into an event.
pub fn read_event(row: &Row) -> anyhow::Result<Event> {
    let kind: String = row.try_get(4)?;
    let content: String = row.try_get(5)?;
    let body: Body = serde_json::from_value(
        json!({"kind": kind, "body": serde_json::from_str::<Value>(&content)?}),
    )?;
    Ok(Event {
        id: EventId(row.try_get(0)?),
        chat: ChatId::try_from(row.try_get::<_, String>(1)?)?,
        thread: row.try_get::<_, Option<i64>>(2)?.map(EventId),
        author: PersonName::new(row.try_get::<_, String>(3)?)?,
        ts: row.try_get(6)?,
        body,
    })
}

/// One event by its id.
pub async fn event(client: &impl GenericClient, id: EventId) -> anyhow::Result<Option<Event>> {
    client
        .query_opt(
            &format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = $1"),
            &[&id.0],
        )
        .await?
        .as_ref()
        .map(read_event)
        .transpose()
}

/// Up to `limit` events of a chat from before `before` (or the newest), oldest first.
pub async fn page(
    client: &impl GenericClient,
    chat: &ChatId,
    before: Option<EventId>,
    limit: u32,
) -> anyhow::Result<Vec<Event>> {
    let rows = client
        .query(
            &format!(
                "SELECT {EVENT_COLUMNS} FROM events WHERE chat = $1 AND id < $2
                 ORDER BY id DESC LIMIT $3"
            ),
            &[
                &chat.to_string(),
                &before.map_or(i64::MAX, |id| id.0),
                &i64::from(limit),
            ],
        )
        .await?;
    let mut events = rows
        .iter()
        .map(read_event)
        .collect::<anyhow::Result<Vec<Event>>>()?;
    events.reverse();
    Ok(events)
}

/// Every event after `after` that `who` may see now, oldest first.
pub async fn events_after(
    client: &impl GenericClient,
    after: EventId,
    who: &PersonName,
) -> anyhow::Result<Vec<Event>> {
    let rows = client
        .query(
            &format!("SELECT {EVENT_COLUMNS} FROM events WHERE id > $1 ORDER BY id"),
            &[&after.0],
        )
        .await?;
    let mut allowed: HashMap<ChatId, bool> = HashMap::new();
    let mut events = Vec::new();
    for row in &rows {
        let event = read_event(row)?;
        let may = match allowed.get(&event.chat) {
            Some(may) => *may,
            None => {
                let may = may_use(client, &event.chat, who).await?;
                allowed.insert(event.chat.clone(), may);
                may
            }
        };
        if may {
            events.push(event);
        }
    }
    Ok(events)
}

/// Replaces the places `owner` advertises.
pub async fn set_places(
    client: &mut Client,
    owner: &PersonName,
    places: &[PlaceInfo],
) -> anyhow::Result<()> {
    let transaction = client.transaction().await?;
    transaction
        .execute("DELETE FROM places WHERE owner = $1", &[&owner.as_str()])
        .await?;
    for place in places {
        let kind = match place.kind {
            PlaceKind::Machine => "machine",
            PlaceKind::Repo => "repo",
        };
        transaction
            .execute(
                "INSERT INTO places (owner, name, kind) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                &[&owner.as_str(), &place.name.as_str(), &kind],
            )
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

/// One person as the database knows them: name, when last seen, and their places.
pub type PersonRow = (PersonName, Option<i64>, Vec<PlaceInfo>);

/// Everyone who has joined, by name, with when they were last seen and the places they advertise.
pub async fn people(client: &impl GenericClient) -> anyhow::Result<Vec<PersonRow>> {
    let mut places: HashMap<String, Vec<PlaceInfo>> = HashMap::new();
    let rows = client
        .query(
            "SELECT owner, name, kind FROM places ORDER BY owner, kind, name",
            &[],
        )
        .await?;
    for row in rows {
        let kind = match row.try_get::<_, String>(2)?.as_str() {
            "machine" => PlaceKind::Machine,
            _ => PlaceKind::Repo,
        };
        places.entry(row.try_get(0)?).or_default().push(PlaceInfo {
            name: PlaceName::new(row.try_get::<_, String>(1)?)?,
            kind,
        });
    }
    let rows = client
        .query("SELECT name, last_seen FROM people ORDER BY name", &[])
        .await?;
    let mut people = Vec::new();
    for row in rows {
        let name: String = row.try_get(0)?;
        let owned = places.remove(&name).unwrap_or_default();
        people.push((PersonName::new(name)?, row.try_get(1)?, owned));
    }
    Ok(people)
}

#[cfg(test)]
mod tests {
    use player_two_core::names::{ChannelName, ChatId, PersonName};

    use super::{create_channel, may_use};
    use crate::db::{Db, TestDatabase};

    #[tokio::test]
    async fn who_may_use_which_chat() {
        let scratch = TestDatabase::new().await.expect("a test database");
        let (db, _) = Db::connect(&scratch.config()).await.expect("set up");
        let mut client = db.lock().await;
        client
            .batch_execute(
                "INSERT INTO people (name, joined) VALUES ('alice', 0), ('bob', 0), ('carol', 0);",
            )
            .await
            .expect("people are added");
        let name = |name: &str| PersonName::new(name).expect("a name");
        let (alice, bob, carol) = (name("alice"), name("bob"), name("carol"));
        let ops = ChannelName::new("ops").expect("a channel");
        create_channel(&mut client, &ops, &alice, 0)
            .await
            .expect("the channel is made");
        let channel = ChatId::Channel(ops);
        let direct = ChatId::direct(alice.clone(), bob.clone()).expect("a chat");
        let with_stranger = ChatId::direct(alice.clone(), name("dave")).expect("a chat");
        let mut answers = Vec::new();
        for (chat, who) in [
            (&channel, &alice),
            (&channel, &bob),
            (&direct, &alice),
            (&direct, &bob),
            (&direct, &carol),
            (&with_stranger, &alice),
        ] {
            answers.push(may_use(&*client, chat, who).await.expect("the check runs"));
        }
        assert_eq!(answers, [true, false, true, true, false, false]);
    }
}
