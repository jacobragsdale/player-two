//! The chats as the desk knows them: every event it has seen, who is here, and what the person
//! has read. No network and no disk, so the window's views can be tested on their own.

use std::collections::{BTreeMap, HashSet};

use player_two_core::api::{ChannelInfo, Person, Presence};
use player_two_core::event::{Body, Event, EventId, Message};
use player_two_core::names::{ChatId, PersonName};

use crate::ui::{ChatSummary, ChatView, Item};

/// Everything the desk has seen of the person's chats.
#[derive(Debug, Clone)]
pub struct Chats {
    me: PersonName,
    events: BTreeMap<ChatId, BTreeMap<EventId, Event>>,
    /// Chats whose oldest event the desk has: there is nothing older to fetch.
    complete: HashSet<ChatId>,
    read: BTreeMap<ChatId, EventId>,
    channels: Vec<ChannelInfo>,
    people: Vec<Person>,
}

impl Chats {
    /// The chats of `me`, with what they had read before.
    #[must_use]
    pub fn new(me: PersonName, read: BTreeMap<ChatId, EventId>) -> Self {
        Self {
            me,
            events: BTreeMap::new(),
            complete: HashSet::new(),
            read,
            channels: Vec::new(),
            people: Vec::new(),
        }
    }

    /// Who the chats belong to.
    #[must_use]
    pub fn me(&self) -> &PersonName {
        &self.me
    }

    /// Every channel, joined or not.
    #[must_use]
    pub fn channels(&self) -> &[ChannelInfo] {
        &self.channels
    }

    /// Everyone who has joined.
    #[must_use]
    pub fn people(&self) -> &[Person] {
        &self.people
    }

    /// Replaces the channel list.
    pub fn set_channels(&mut self, channels: Vec<ChannelInfo>) {
        self.channels = channels;
    }

    /// Replaces the people list.
    pub fn set_people(&mut self, people: Vec<Person>) {
        self.people = people;
    }

    /// Records someone arriving or leaving.
    pub fn presence(&mut self, presence: &Presence) {
        if let Some(person) = self
            .people
            .iter_mut()
            .find(|person| person.name == presence.name)
        {
            person.here = presence.here;
        }
    }

    /// The chats to show: the person's channels, then a direct chat with everyone else.
    #[must_use]
    pub fn mine(&self) -> Vec<ChatId> {
        let channels = self
            .channels
            .iter()
            .filter(|channel| channel.members.contains(&self.me))
            .map(|channel| ChatId::Channel(channel.name.clone()));
        let directs = self
            .people
            .iter()
            .filter_map(|person| ChatId::direct(self.me.clone(), person.name.clone()).ok());
        channels.chain(directs).collect()
    }

    /// Whether the desk has loaded anything of a chat yet.
    #[must_use]
    pub fn has_loaded(&self, chat: &ChatId) -> bool {
        self.events.contains_key(chat)
    }

    /// Adds a page fetched from the hub. `last` says the hub has nothing older. A chat seen for
    /// the first time counts as read up to its newest event, so that joining does not greet the
    /// person with everything ever said.
    pub fn add_page(&mut self, chat: &ChatId, page: Vec<Event>, last: bool) {
        let newest = page.last().map(|event| event.id);
        let known = self.events.entry(chat.clone()).or_default();
        for event in page {
            known.insert(event.id, event);
        }
        if last {
            self.complete.insert(chat.clone());
        }
        if let Some(newest) = newest {
            self.read.entry(chat.clone()).or_insert(newest);
        }
    }

    /// Adds one event, as the stream or a post brought it.
    pub fn apply(&mut self, event: Event) {
        if event.author == self.me {
            // What the person wrote, they have read.
            let mark = self.read.entry(event.chat.clone()).or_insert(event.id);
            *mark = (*mark).max(event.id);
        }
        self.events
            .entry(event.chat.clone())
            .or_default()
            .insert(event.id, event);
    }

    /// The newest event the desk has seen, in any chat.
    #[must_use]
    pub fn last_id(&self) -> Option<EventId> {
        self.events
            .values()
            .filter_map(|events| events.keys().next_back().copied())
            .max()
    }

    /// The oldest event the desk has of a chat.
    #[must_use]
    pub fn oldest(&self, chat: &ChatId) -> Option<EventId> {
        self.events
            .get(chat)
            .and_then(|events| events.keys().next().copied())
    }

    /// Marks a chat read up to its newest event.
    pub fn mark_read(&mut self, chat: &ChatId) {
        if let Some(newest) = self
            .events
            .get(chat)
            .and_then(|events| events.keys().next_back())
        {
            self.read.insert(chat.clone(), *newest);
        }
    }

    /// How far each chat has been read, to keep across restarts.
    #[must_use]
    pub fn read_marks(&self) -> &BTreeMap<ChatId, EventId> {
        &self.read
    }

    fn messages<'a>(&'a self, chat: &ChatId) -> impl Iterator<Item = (&'a Event, &'a Message)> {
        self.events
            .get(chat)
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter_map(|event| match &event.body {
                Body::Message(message) => Some((event, message)),
                _ => None,
            })
    }

    fn is_unread(&self, event: &Event) -> bool {
        event.author != self.me
            && self
                .read
                .get(&event.chat)
                .is_none_or(|mark| event.id > *mark)
    }

    fn title(&self, chat: &ChatId) -> String {
        match chat {
            ChatId::Channel(name) => format!("#{name}"),
            ChatId::Direct(one, other) => if one == &self.me { other } else { one }.to_string(),
        }
    }

    /// The chat list.
    #[must_use]
    pub fn summaries(&self) -> Vec<ChatSummary> {
        self.mine()
            .into_iter()
            .map(|chat| {
                let unread = self
                    .messages(&chat)
                    .filter(|(event, _)| self.is_unread(event))
                    .count();
                let here = match &chat {
                    ChatId::Channel(_) => None,
                    ChatId::Direct(..) => {
                        let title = self.title(&chat);
                        self.people
                            .iter()
                            .find(|person| person.name.as_str() == title)
                            .map(|person| person.here)
                    }
                };
                ChatSummary {
                    title: self.title(&chat),
                    unread: u32::try_from(unread).unwrap_or(u32::MAX),
                    here,
                    chat,
                }
            })
            .collect()
    }

    /// One chat's top level, or one thread of it.
    #[must_use]
    pub fn view(&self, chat: &ChatId, thread: Option<EventId>) -> ChatView {
        let mut replies: BTreeMap<EventId, u32> = BTreeMap::new();
        for (event, _) in self.messages(chat) {
            if let Some(root) = event.thread {
                *replies.entry(root).or_default() += 1;
            }
        }
        let shown: Vec<(&Event, &Message)> = self
            .messages(chat)
            .filter(|(event, _)| match thread {
                None => event.thread.is_none(),
                Some(root) => event.id == root || event.thread == Some(root),
            })
            .collect();
        let first_unread = shown
            .iter()
            .find(|(event, _)| self.is_unread(event))
            .map(|(event, _)| event.id);
        let items = shown
            .into_iter()
            .map(|(event, message)| Item {
                id: event.id,
                author: event.author.clone(),
                ts: event.ts,
                text: message.text.clone(),
                tags: message.tags.clone(),
                replies: replies.get(&event.id).copied().unwrap_or_default(),
            })
            .collect();
        let members = match chat {
            ChatId::Channel(name) => self
                .channels
                .iter()
                .find(|channel| &channel.name == name)
                .map(|channel| channel.members.clone())
                .unwrap_or_default(),
            ChatId::Direct(one, other) => vec![one.clone(), other.clone()],
        };
        ChatView {
            chat: chat.clone(),
            title: self.title(chat),
            members,
            thread,
            items,
            has_older: !self.complete.contains(chat),
            first_unread,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use player_two_core::api::{ChannelInfo, Person, Presence};
    use player_two_core::event::{Body, Event, EventId, Message};
    use player_two_core::names::{ChannelName, ChatId, PersonName};

    use super::Chats;

    fn name(name: &str) -> PersonName {
        PersonName::new(name).expect("a name")
    }

    fn message(id: i64, chat: &ChatId, author: &str, thread: Option<i64>, text: &str) -> Event {
        Event {
            id: EventId(id),
            chat: chat.clone(),
            thread: thread.map(EventId),
            author: name(author),
            ts: id * 1000,
            body: Body::Message(Message {
                text: text.to_owned(),
                tags: vec![],
            }),
        }
    }

    fn person(who: &str) -> Person {
        Person {
            name: name(who),
            here: false,
            last_seen: None,
            places: vec![],
        }
    }

    fn chats() -> (Chats, ChatId, ChatId) {
        let mut chats = Chats::new(name("alice"), BTreeMap::new());
        let ops = ChannelName::new("ops").expect("a channel");
        chats.set_channels(vec![
            ChannelInfo {
                name: ops.clone(),
                members: vec![name("alice"), name("bob")],
            },
            ChannelInfo {
                name: ChannelName::new("other").expect("a channel"),
                members: vec![name("bob")],
            },
        ]);
        chats.set_people(vec![person("alice"), person("bob")]);
        let direct = ChatId::direct(name("alice"), name("bob")).expect("a chat");
        (chats, ChatId::Channel(ops), direct)
    }

    #[test]
    fn the_list_has_my_channels_then_everyone_else() {
        let (mut chats, ops, direct) = chats();
        assert_eq!(chats.mine(), [ops, direct.clone()]);
        chats.presence(&Presence {
            name: name("bob"),
            here: true,
        });
        let summaries = chats.summaries();
        assert_eq!(summaries[0].title, "#ops");
        assert_eq!(
            (summaries[1].title.as_str(), summaries[1].here),
            ("bob", Some(true))
        );
    }

    #[test]
    fn what_was_there_on_joining_counts_as_read_and_what_comes_after_does_not() {
        let (mut chats, ops, _) = chats();
        chats.add_page(&ops, vec![message(1, &ops, "bob", None, "old")], true);
        assert_eq!(chats.summaries()[0].unread, 0);
        chats.apply(message(2, &ops, "bob", None, "new"));
        chats.apply(message(3, &ops, "bob", Some(2), "a reply"));
        assert_eq!(chats.summaries()[0].unread, 2);
        assert_eq!(chats.view(&ops, None).first_unread, Some(EventId(2)));
        chats.apply(message(4, &ops, "alice", None, "mine"));
        assert_eq!(chats.summaries()[0].unread, 0, "writing in a chat reads it");
        chats.apply(message(5, &ops, "bob", None, "later"));
        chats.mark_read(&ops);
        assert_eq!(chats.summaries()[0].unread, 0);
        assert_eq!(chats.read_marks().get(&ops), Some(&EventId(5)));
    }

    #[test]
    fn a_chat_shows_its_top_level_and_a_thread_shows_its_replies() {
        let (mut chats, ops, _) = chats();
        chats.add_page(&ops, vec![message(1, &ops, "bob", None, "question")], false);
        chats.apply(message(2, &ops, "alice", Some(1), "answer"));
        chats.apply(message(3, &ops, "bob", None, "next"));
        let top = chats.view(&ops, None);
        let texts: Vec<&str> = top.items.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts, ["question", "next"]);
        assert_eq!(top.items[0].replies, 1);
        assert!(top.has_older);
        assert_eq!(top.members, [name("alice"), name("bob")]);
        let thread = chats.view(&ops, Some(EventId(1)));
        let texts: Vec<&str> = thread.items.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts, ["question", "answer"]);
        assert_eq!(chats.last_id(), Some(EventId(3)));
        assert_eq!(chats.oldest(&ops), Some(EventId(1)));
    }
}
