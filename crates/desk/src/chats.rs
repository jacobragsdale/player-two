//! The chats as the desk knows them: every event it has seen, who is here, and what the person
//! has read. No network and no disk, so the window's views can be tested on their own.

use std::collections::{BTreeMap, HashSet};

use player_two_core::api::{ChannelInfo, Person, Presence};
use player_two_core::event::{
    Body, Ending, Event, EventId, Message, RunAccepted, RunDeclined, RunWithdrawn, StepsBy, Tag,
};
use player_two_core::names::{ChatId, PersonName, PlaceName};

use crate::ui::{ChatSummary, ChatView, Item, PromptView, RunState, RunView};

/// How many characters of a message a notification shows.
const NOTICE_LENGTH: usize = 200;

/// Something worth a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Who, and where.
    pub title: String,
    /// The start of what it says.
    pub body: String,
    /// The thread it is in, which clicking it opens.
    pub thread: Option<EventId>,
    /// For a follow-up, the thread and place of the run it follows: when keep going runs it,
    /// there is nothing to click, so nothing to notify.
    pub follows: Option<(EventId, PlaceName)>,
}

/// The start of a text, as a notification shows it.
fn shortened(text: &str) -> String {
    let mut short: String = text.chars().take(NOTICE_LENGTH).collect();
    if text.chars().count() > NOTICE_LENGTH {
        short.push('…');
    }
    short
}

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

    /// Records someone arriving or leaving. Someone leaving was last seen now, until the next
    /// fetch of the people says when by the hub's clock.
    pub fn presence(&mut self, presence: &Presence) {
        if let Some(person) = self
            .people
            .iter_mut()
            .find(|person| person.name == presence.name)
        {
            if person.here && !presence.here {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| {
                        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
                    });
                person.last_seen = Some(now);
            }
            person.here = presence.here;
        }
    }

    /// The chats to show: the person's channels, then a direct chat with everyone else. The
    /// people talked with come first, the latest first, then everyone else by name.
    #[must_use]
    pub fn mine(&self) -> Vec<ChatId> {
        let channels = self
            .channels
            .iter()
            .filter(|channel| channel.members.contains(&self.me))
            .map(|channel| ChatId::Channel(channel.name.clone()));
        let mut directs: Vec<(Option<EventId>, ChatId)> = self
            .people
            .iter()
            .filter_map(|person| ChatId::direct(self.me.clone(), person.name.clone()).ok())
            .map(|chat| {
                let latest = self.messages(&chat).map(|(event, _)| event.id).max();
                (latest, chat)
            })
            .collect();
        // Newest first; a chat with no messages sorts after every one with some.
        directs.sort_by(|(one, one_chat), (other, other_chat)| {
            other
                .cmp(one)
                .then_with(|| self.title(one_chat).cmp(&self.title(other_chat)))
        });
        channels
            .chain(directs.into_iter().map(|(_, chat)| chat))
            .collect()
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

    /// What to notify the person of, if anything: a direct message to them, a tag of them or
    /// their places, a follow-up waiting for their click, a step waiting for their yes, or the end
    /// of a run they asked for.
    #[must_use]
    pub fn notice(&self, event: &Event) -> Option<Notice> {
        match &event.body {
            Body::Message(message) => self.message_notice(event, message),
            Body::RunPrompt(prompt) => {
                let (owner, accepted, asker) = self.run_of(&event.chat, prompt.run)?;
                let answers = match accepted.steps_by {
                    StepsBy::Owner => &owner,
                    StepsBy::Asker => &asker,
                };
                (answers == &self.me).then(|| Notice {
                    title: format!("{} waits for your yes", self.whose_agent(&owner)),
                    body: shortened(&prompt.what),
                    thread: event.thread,
                    follows: None,
                })
            }
            Body::RunEnded(ended) => {
                let (owner, _, asker) = self.run_of(&event.chat, ended.run)?;
                if asker != self.me {
                    return None;
                }
                let how = match ended.how {
                    Ending::Done => "finished",
                    Ending::Stopped => "was stopped",
                    Ending::Failed => "failed",
                };
                let result =
                    self.events
                        .get(&event.chat)?
                        .values()
                        .find_map(|seen| match &seen.body {
                            Body::RunResult(result) if result.run == ended.run => {
                                Some(result.text.clone())
                            }
                            _ => None,
                        });
                Some(Notice {
                    title: format!("{} {how}", self.whose_agent(&owner)),
                    body: shortened(&result.or_else(|| ended.reason.clone()).unwrap_or_default()),
                    thread: event.thread,
                    follows: None,
                })
            }
            _ => None,
        }
    }

    /// What to notify of a message: one to the person directly, one tagging them, or a follow-up
    /// of a run of theirs, which waits for their click.
    fn message_notice(&self, event: &Event, message: &Message) -> Option<Notice> {
        if event.author == self.me {
            return None;
        }
        let direct = event.chat.is_between(&self.me);
        let tagged = message.tags.iter().any(|tag| tag.person == self.me);
        let follows = self
            .followed(event)
            .filter(|run| run.owner == self.me)
            .map(|run| (run.thread, run.place));
        if !direct && !tagged && follows.is_none() {
            return None;
        }
        let title = if direct {
            event.author.to_string()
        } else if tagged {
            format!("{} in {}", event.author, self.title(&event.chat))
        } else {
            format!("{} follows up in {}", event.author, self.title(&event.chat))
        };
        Some(Notice {
            title,
            body: shortened(&message.text),
            thread: event.thread,
            follows,
        })
    }

    /// A run's owner, how it was accepted, and who asked for it.
    fn run_of(&self, chat: &ChatId, run: EventId) -> Option<(PersonName, RunAccepted, PersonName)> {
        let events = self.events.get(chat)?;
        let accepted = events.get(&run)?;
        let Body::RunAccepted(body) = &accepted.body else {
            return None;
        };
        let asker = events.get(&body.request)?.author.clone();
        Some((accepted.author.clone(), body.clone(), asker))
    }

    /// "Your agent", or "bob's agent".
    fn whose_agent(&self, owner: &PersonName) -> String {
        if owner == &self.me {
            "Your agent".to_owned()
        } else {
            format!("{owner}'s agent")
        }
    }

    /// One event of a chat.
    #[must_use]
    pub fn event(&self, chat: &ChatId, id: EventId) -> Option<&Event> {
        self.events.get(chat)?.get(&id)
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

    /// Every run in a chat, by the request it answers, and the requests answered in any way:
    /// run, declined or taken back.
    fn runs(
        &self,
        chat: &ChatId,
    ) -> (
        BTreeMap<EventId, Vec<RunView>>,
        BTreeMap<EventId, Vec<PersonName>>,
    ) {
        let mut runs: BTreeMap<EventId, RunView> = BTreeMap::new();
        let mut requests: BTreeMap<EventId, EventId> = BTreeMap::new();
        let mut answered: BTreeMap<EventId, Vec<PersonName>> = BTreeMap::new();
        let events = self.events.get(chat).into_iter().flat_map(BTreeMap::values);
        for event in events {
            match &event.body {
                Body::RunAccepted(accepted) => {
                    let asker = self
                        .events
                        .get(chat)
                        .and_then(|events| events.get(&accepted.request))
                        .map(|request| request.author.clone());
                    let may_answer = event.author == self.me
                        || (accepted.steps_by == StepsBy::Asker
                            && asker.as_ref() == Some(&self.me));
                    requests.insert(event.id, accepted.request);
                    answered
                        .entry(accepted.request)
                        .or_default()
                        .push(event.author.clone());
                    runs.insert(
                        event.id,
                        RunView {
                            id: event.id,
                            thread: event.thread.unwrap_or(accepted.request),
                            owner: event.author.clone(),
                            place: accepted.place.clone(),
                            runs_in: accepted.runs_in.clone(),
                            mode: accepted.mode,
                            steps_by: accepted.steps_by,
                            state: RunState::Working,
                            reason: None,
                            activity: Vec::new(),
                            prompts: Vec::new(),
                            result: None,
                            may_answer,
                            mine: event.author == self.me,
                        },
                    );
                }
                Body::RunDeclined(RunDeclined { request }) => {
                    answered
                        .entry(*request)
                        .or_default()
                        .push(event.author.clone());
                }
                Body::RunWithdrawn(RunWithdrawn { request }) => {
                    // Taken back, it is answered for everyone it tagged.
                    let tagged: Vec<PersonName> = self
                        .events
                        .get(chat)
                        .and_then(|events| events.get(request))
                        .map(|asked| match &asked.body {
                            Body::Message(message) => {
                                message.tags.iter().map(|tag| tag.person.clone()).collect()
                            }
                            _ => Vec::new(),
                        })
                        .unwrap_or_default();
                    answered.entry(*request).or_default().extend(tagged);
                }
                Body::RunActivity(activity) => {
                    if let Some(run) = runs.get_mut(&activity.run) {
                        for item in &activity.items {
                            match run.activity.iter_mut().find(|kept| kept.id == item.id) {
                                Some(kept) => *kept = item.clone(),
                                None => run.activity.push(item.clone()),
                            }
                        }
                    }
                }
                Body::RunPrompt(prompt) => {
                    if let Some(run) = runs.get_mut(&prompt.run) {
                        run.prompts.push(PromptView {
                            id: prompt.prompt.clone(),
                            what: prompt.what.clone(),
                            allow: None,
                            by: None,
                        });
                    }
                }
                Body::RunAnswer(answer) => {
                    // The first answer counts; a later one finds the step answered.
                    if let Some(prompt) = runs.get_mut(&answer.run).and_then(|run| {
                        run.prompts
                            .iter_mut()
                            .find(|prompt| prompt.id == answer.prompt)
                    }) && prompt.allow.is_none()
                    {
                        prompt.allow = Some(answer.allow);
                        prompt.by = Some(event.author.clone());
                    }
                }
                Body::RunResult(result) => {
                    if let Some(run) = runs.get_mut(&result.run) {
                        run.result = Some(result.text.clone());
                    }
                }
                Body::RunEnded(ended) => {
                    if let Some(run) = runs.get_mut(&ended.run) {
                        run.state = match ended.how {
                            Ending::Done => RunState::Done,
                            Ending::Stopped => RunState::Stopped,
                            Ending::Failed => RunState::Failed,
                        };
                        run.reason.clone_from(&ended.reason);
                    }
                }
                Body::Message(_) => {}
            }
        }
        let mut by_request: BTreeMap<EventId, Vec<RunView>> = BTreeMap::new();
        for (id, run) in runs {
            if let Some(request) = requests.get(&id) {
                by_request.entry(*request).or_default().push(run);
            }
        }
        (by_request, answered)
    }

    /// The tags of a request that nobody has answered yet: not run, declined or taken back.
    #[must_use]
    pub fn unanswered(&self, chat: &ChatId, request: EventId) -> Vec<Tag> {
        let Some(Body::Message(message)) = self.event(chat, request).map(|event| &event.body)
        else {
            return Vec::new();
        };
        let (_, answered) = self.runs(chat);
        let answered_by = answered.get(&request).cloned().unwrap_or_default();
        message
            .tags
            .iter()
            .filter(|tag| !answered_by.contains(&tag.person))
            .cloned()
            .collect()
    }

    /// The runs this person's desk started that never said how they ended: a restart cut them
    /// off. Each is given by its chat, thread and id.
    #[must_use]
    pub fn unfinished(&self) -> Vec<(ChatId, EventId, EventId)> {
        let mut found = Vec::new();
        for chat in self.events.keys() {
            let (runs, _) = self.runs(chat);
            for run in runs.values().flatten() {
                if run.mine && run.state == RunState::Working {
                    found.push((chat.clone(), run.thread, run.id));
                }
            }
        }
        found
    }

    /// The thread so far, before `before`, as a fresh session is given it: who said what, and
    /// what the agent answered. Empty when there was nothing before.
    #[must_use]
    pub fn transcript(&self, chat: &ChatId, thread: EventId, before: EventId) -> String {
        let lines: Vec<String> = self
            .events
            .get(chat)
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter(|event| {
                event.id < before && (event.id == thread || event.thread == Some(thread))
            })
            .filter_map(|event| match &event.body {
                Body::Message(message) => Some(format!("{}: {}", event.author, message.text)),
                Body::RunResult(result) => Some(format!("You answered: {}", result.text)),
                _ => None,
            })
            .collect();
        if lines.is_empty() {
            String::new()
        } else {
            format!(
                "Earlier in this conversation:\n\n{}\n\nNow:\n\n",
                lines.join("\n\n")
            )
        }
    }

    /// The run a message follows up: a message with no tags, in a thread with a run, written by
    /// that run's asker, after it. The newest such run is the one followed.
    #[must_use]
    pub fn followed(&self, event: &Event) -> Option<RunView> {
        let (Body::Message(message), Some(thread)) = (&event.body, event.thread) else {
            return None;
        };
        if !message.tags.is_empty() {
            return None;
        }
        let events = self.events.get(&event.chat)?;
        let (runs, _) = self.runs(&event.chat);
        runs.into_iter()
            .flat_map(|(request, runs)| runs.into_iter().map(move |run| (request, run)))
            .filter(|(request, run)| {
                run.thread == thread
                    && run.id < event.id
                    && events
                        .get(request)
                        .is_some_and(|asked| asked.author == event.author)
            })
            .max_by_key(|(_, run)| run.id)
            .map(|(_, run)| run)
    }

    /// The `@` mentions in a message that name nobody who has joined, a place they do not
    /// advertise, or someone with no agent set up, each as a line to show under it.
    fn unknown(&self, message: &Message) -> Vec<String> {
        let mut lines = Vec::new();
        for word in message.text.split_whitespace() {
            let Some(mention) = word.strip_prefix('@') else {
                continue;
            };
            let mention = mention.trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
            let (name, place) = mention
                .split_once('/')
                .map_or((mention, None), |(name, place)| (name, Some(place)));
            if name.is_empty()
                || message.tags.iter().any(|tag| {
                    tag.person.as_str() == name
                        && place.is_none_or(|place| tag.place.as_str() == place)
                })
            {
                continue;
            }
            let line = match (
                self.people
                    .iter()
                    .find(|person| person.name.as_str() == name),
                place,
            ) {
                (None, _) => format!("nobody named {name} has joined"),
                // Someone advertising nothing has no agent: a machine with one is advertised.
                (Some(person), _) if person.places.is_empty() && person.name == self.me => {
                    "you have not set up an agent yet; Settings shows how".to_owned()
                }
                (Some(person), _) if person.places.is_empty() => {
                    format!("{name} has not set up an agent yet")
                }
                (Some(_), Some(place)) => format!("{name} does not share {place}"),
                (Some(_), None) => continue,
            };
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        lines
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
        let (mut runs, answered) = self.runs(chat);
        let items = shown
            .into_iter()
            .map(|(event, message)| {
                let answered_by = answered.get(&event.id).cloned().unwrap_or_default();
                let follows = if answered_by.is_empty() {
                    self.followed(event)
                } else {
                    None
                };
                Item {
                    id: event.id,
                    author: event.author.clone(),
                    ts: event.ts,
                    text: message.text.clone(),
                    tags: message.tags.clone(),
                    replies: replies.get(&event.id).copied().unwrap_or_default(),
                    follows,
                    unknown: self.unknown(message),
                    runs: runs.remove(&event.id).unwrap_or_default(),
                    waiting: message
                        .tags
                        .iter()
                        .filter(|tag| !answered_by.contains(&tag.person))
                        .cloned()
                        .collect(),
                }
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
        let person = match chat {
            ChatId::Channel(_) => None,
            ChatId::Direct(..) => {
                let title = self.title(chat);
                self.people
                    .iter()
                    .find(|person| person.name.as_str() == title)
                    .cloned()
            }
        };
        ChatView {
            chat: chat.clone(),
            title: self.title(chat),
            members,
            person,
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
    use player_two_core::event::{Body, Ending, Event, EventId, Message, RunAccepted, StepsBy};
    use player_two_core::names::{ChannelName, ChatId, PersonName, PlaceName};

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
    fn people_talked_with_come_first_the_latest_first() {
        let (mut chats, ops, bob) = chats();
        chats.set_people(vec![
            person("alice"),
            person("bob"),
            person("carol"),
            person("dave"),
        ]);
        let with = |who: &str| ChatId::direct(name("alice"), name(who)).expect("a chat");
        chats.apply(message(1, &with("dave"), "dave", None, "hi"));
        chats.apply(message(2, &bob, "alice", None, "hello"));
        assert_eq!(
            chats.mine(),
            [ops, bob.clone(), with("dave"), with("carol")]
        );
        let view = chats.view(&bob, None);
        assert_eq!(
            view.person.map(|person| person.name),
            Some(name("bob")),
            "a direct chat says who it is with"
        );
    }

    #[test]
    fn someone_leaving_was_last_seen_then() {
        let (mut chats, _, _) = chats();
        let arrive = |here| Presence {
            name: name("bob"),
            here,
        };
        chats.presence(&arrive(true));
        assert_eq!(chats.people()[1].last_seen, None);
        chats.presence(&arrive(false));
        assert!(chats.people()[1].last_seen.is_some());
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
    fn direct_messages_and_tags_of_me_are_worth_a_notice() {
        let (chats, ops, direct) = chats();
        assert_eq!(
            chats
                .notice(&message(1, &direct, "bob", None, "hi"))
                .map(|notice| (notice.title, notice.body)),
            Some(("bob".to_owned(), "hi".to_owned()))
        );
        assert_eq!(
            chats.notice(&message(2, &direct, "alice", None, "mine")),
            None
        );
        assert_eq!(
            chats.notice(&message(3, &ops, "bob", None, "chatter")),
            None
        );
        let mut tagged = message(4, &ops, "bob", None, &"x".repeat(250));
        if let Body::Message(text) = &mut tagged.body {
            text.tags.push(player_two_core::event::Tag {
                person: name("alice"),
                place: player_two_core::names::PlaceName::machine(),
            });
        }
        let notice = chats
            .notice(&tagged)
            .expect("a tag of alice is worth a notice");
        assert_eq!(notice.title, "bob in #ops");
        assert_eq!(notice.body.chars().count(), 201);
    }

    /// An event of a run: its kind's body, by `author`, in the thread of message 1.
    fn run_event(id: i64, chat: &ChatId, author: &str, body: Body) -> Event {
        Event {
            id: EventId(id),
            chat: chat.clone(),
            thread: Some(EventId(1)),
            author: name(author),
            ts: id * 1000,
            body,
        }
    }

    /// Bob asks alice's machine in #ops (message 1), and alice lets it run (event 2).
    fn a_run(steps_by: StepsBy) -> (Chats, ChatId) {
        let (mut chats, ops, _) = chats();
        let mut asked = message(1, &ops, "bob", None, "@alice fix pip");
        if let Body::Message(text) = &mut asked.body {
            text.tags.push(player_two_core::event::Tag {
                person: name("alice"),
                place: PlaceName::machine(),
            });
        }
        chats.apply(asked);
        chats.apply(run_event(
            2,
            &ops,
            "alice",
            Body::RunAccepted(RunAccepted {
                request: EventId(1),
                place: PlaceName::machine(),
                runs_in: player_two_core::event::RunsIn::Windows,
                mode: player_two_core::event::Mode::Ask,
                steps_by,
                keep_going: false,
            }),
        ));
        (chats, ops)
    }

    #[test]
    fn a_step_notifies_whoever_answers_it() {
        let prompt = |ops: &ChatId| {
            run_event(
                3,
                ops,
                "alice",
                Body::RunPrompt(player_two_core::event::RunPrompt {
                    run: EventId(2),
                    prompt: "p1".to_owned(),
                    what: "pip config set".to_owned(),
                }),
            )
        };
        let (chats, ops) = a_run(StepsBy::Owner);
        let notice = chats.notice(&prompt(&ops)).expect("alice answers it");
        assert_eq!(notice.title, "Your agent waits for your yes");
        assert_eq!(notice.thread, Some(EventId(1)));
        // Handed to bob, the step is his to answer, so alice's desk keeps quiet.
        let (chats, ops) = a_run(StepsBy::Asker);
        assert_eq!(chats.notice(&prompt(&ops)), None);
    }

    #[test]
    fn the_end_of_a_run_notifies_its_asker() {
        let (mut chats, ops) = a_run(StepsBy::Owner);
        chats.apply(run_event(
            3,
            &ops,
            "alice",
            Body::RunResult(player_two_core::event::RunResult {
                run: EventId(2),
                text: "Fixed the proxy.".to_owned(),
            }),
        ));
        let ended = run_event(
            4,
            &ops,
            "alice",
            Body::RunEnded(player_two_core::event::RunEnded {
                run: EventId(2),
                how: Ending::Done,
                reason: None,
            }),
        );
        assert_eq!(chats.notice(&ended), None, "alice did not ask");
        let mut bobs = Chats::new(name("bob"), BTreeMap::new());
        for id in 1..=3 {
            bobs.apply(chats.event(&ops, EventId(id)).expect("seen").clone());
        }
        let notice = bobs.notice(&ended).expect("bob asked");
        assert_eq!(
            (notice.title.as_str(), notice.body.as_str()),
            ("alice's agent finished", "Fixed the proxy.")
        );
    }

    #[test]
    fn a_follow_up_notifies_the_runs_owner() {
        let (mut chats, ops) = a_run(StepsBy::Owner);
        chats.apply(run_event(
            3,
            &ops,
            "alice",
            Body::RunEnded(player_two_core::event::RunEnded {
                run: EventId(2),
                how: Ending::Done,
                reason: None,
            }),
        ));
        let follow_up = message(4, &ops, "bob", Some(1), "and the other one");
        let notice = chats.notice(&follow_up).expect("alice decides it");
        assert_eq!(notice.title, "bob follows up in #ops");
        assert_eq!(notice.follows, Some((EventId(1), PlaceName::machine())));
    }

    #[test]
    fn a_tag_that_names_nothing_says_why() {
        let (mut chats, ops, _) = chats();
        let mut bob = person("bob");
        bob.places.push(player_two_core::api::PlaceInfo {
            name: player_two_core::names::PlaceName::machine(),
            kind: player_two_core::api::PlaceKind::Machine,
        });
        chats.set_people(vec![person("alice"), bob, person("carol")]);
        chats.apply(message(
            1,
            &ops,
            "alice",
            None,
            "@carol, @alice, @bob/billing and @dave: help",
        ));
        assert_eq!(
            chats.view(&ops, None).items[0].unknown,
            [
                "carol has not set up an agent yet",
                "you have not set up an agent yet; Settings shows how",
                "bob does not share billing",
                "nobody named dave has joined",
            ]
        );
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
