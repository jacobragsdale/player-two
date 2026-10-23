//! Reads and writes on the database, one function each, so the routes stay about HTTP.

use player_two_core::api::ChannelInfo;
use player_two_core::names::{ChannelName, ChatId, PersonName};
use rusqlite::{Connection, OptionalExtension};

/// Whether `name` has joined the hub.
pub fn has_joined(connection: &Connection, name: &PersonName) -> anyhow::Result<bool> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM people WHERE name = ?1",
            [name.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Every channel, with who is in it.
pub fn channels(connection: &Connection) -> anyhow::Result<Vec<ChannelInfo>> {
    let mut statement = connection.prepare(
        "SELECT channels.name, members.name FROM channels
         LEFT JOIN members ON members.channel = channels.name
         ORDER BY channels.name, members.name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut channels: Vec<ChannelInfo> = Vec::new();
    for row in rows {
        let (channel, member) = row?;
        let channel = ChannelName::new(channel)?;
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
pub fn create_channel(
    connection: &mut Connection,
    name: &ChannelName,
    by: &PersonName,
    now: i64,
) -> anyhow::Result<bool> {
    let transaction = connection.transaction()?;
    let made = transaction.execute(
        "INSERT INTO channels (name, created_by, created) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        (name.as_str(), by.as_str(), now),
    )?;
    if made == 1 {
        transaction.execute(
            "INSERT INTO members (channel, name) VALUES (?1, ?2)",
            (name.as_str(), by.as_str()),
        )?;
    }
    transaction.commit()?;
    Ok(made == 1)
}

/// Puts `who` in a channel. False when there is no such channel.
pub fn join_channel(
    connection: &Connection,
    name: &ChannelName,
    who: &PersonName,
) -> anyhow::Result<bool> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM channels WHERE name = ?1",
            [name.as_str()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        connection.execute(
            "INSERT INTO members (channel, name) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            (name.as_str(), who.as_str()),
        )?;
    }
    Ok(exists)
}

/// Takes `who` out of a channel.
pub fn leave_channel(
    connection: &Connection,
    name: &ChannelName,
    who: &PersonName,
) -> anyhow::Result<()> {
    connection.execute(
        "DELETE FROM members WHERE channel = ?1 AND name = ?2",
        (name.as_str(), who.as_str()),
    )?;
    Ok(())
}

/// Whether `who` may read and write `chat`: a channel they are in, or a direct chat of theirs
/// with someone who has joined.
pub fn may_use(connection: &Connection, chat: &ChatId, who: &PersonName) -> anyhow::Result<bool> {
    match chat {
        ChatId::Channel(name) => Ok(connection
            .query_row(
                "SELECT 1 FROM members WHERE channel = ?1 AND name = ?2",
                (name.as_str(), who.as_str()),
                |_| Ok(()),
            )
            .optional()?
            .is_some()),
        ChatId::Direct(one, other) => {
            let them = if one == who { other } else { one };
            Ok(chat.is_between(who) && has_joined(connection, them)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use player_two_core::names::{ChannelName, ChatId, PersonName};

    use super::{create_channel, may_use};
    use crate::db::Db;

    #[tokio::test]
    async fn who_may_use_which_chat() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let db = Db::open(&folder.path().join("hub.db")).expect("opens");
        let answers = db
            .call(|connection| {
                connection.execute_batch(
                    "INSERT INTO people (name, joined) VALUES ('alice', 0), ('bob', 0), ('carol', 0);",
                )?;
                let name = |name: &str| PersonName::new(name).expect("a name");
                let (alice, bob, carol) = (name("alice"), name("bob"), name("carol"));
                let ops = ChannelName::new("ops")?;
                create_channel(connection, &ops, &alice, 0)?;
                let channel = ChatId::Channel(ops);
                let direct = ChatId::direct(alice.clone(), bob.clone())?;
                let with_stranger = ChatId::direct(alice.clone(), name("dave"))?;
                Ok([
                    may_use(connection, &channel, &alice)?,
                    may_use(connection, &channel, &bob)?,
                    may_use(connection, &direct, &alice)?,
                    may_use(connection, &direct, &bob)?,
                    may_use(connection, &direct, &carol)?,
                    may_use(connection, &with_stranger, &alice)?,
                ])
            })
            .await
            .expect("the checks run");
        assert_eq!(answers, [true, false, true, true, false, false]);
    }
}
