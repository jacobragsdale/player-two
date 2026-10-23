//! One person's side of Player Two: their hub client, places, agents, approvals and runs.
//!
//! The window asks the desk for things with [`Desk::handle`], and hears what to ask for again
//! from the receiver [`Desk::new`] gives back. [`Desk::run`] keeps the desk connected to the hub
//! for as long as the app runs.

pub mod chats;
pub mod files;
pub mod hub;
pub mod places;
pub mod system;
pub mod ui;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Context;
use chats::Chats;
use files::{Files, Identity};
use hub::{Hub, Incoming, NotJoined};
use places::{Folder, Settings};
use player_two_core::api::NewEvent;
use player_two_core::event::{Body, Event, EventId, Message};
use player_two_core::join_code::JoinCode;
use player_two_core::names::{ChannelName, ChatId, PersonName};
use tokio::sync::{Notify, mpsc};
use ui::{Connection, Overview, PlacesView, Request, Response};

/// How many events one page from the hub holds.
const PAGE: u32 = 100;
/// The longest wait between attempts to reconnect.
const MOST_DELAY: Duration = Duration::from_secs(30);
/// How often the people and channel lists are fetched again while connected.
const REFRESH: Duration = Duration::from_secs(60);
/// How long a desk that found the hub on another version waits before looking again.
const RECHECK: Duration = Duration::from_secs(300);

/// The hub runs another version than this app, which must change before anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WrongVersion(String);

impl std::fmt::Display for WrongVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the hub runs {} and this app {}",
            self.0,
            env!("CARGO_PKG_VERSION")
        )
    }
}

impl std::error::Error for WrongVersion {}

/// The desk. Cloning it gives another handle on the same desk.
#[derive(Debug, Clone)]
pub struct Desk {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    files: Files,
    http: reqwest::Client,
    events: mpsc::UnboundedSender<ui::Event>,
    state: Mutex<State>,
    /// Rung when the run loop should look again: the person joined, or asked it to.
    wake: Notify,
}

#[derive(Debug)]
struct State {
    connection: Connection,
    hub: Option<Hub>,
    chats: Option<Chats>,
}

impl Desk {
    /// A desk keeping its files in `folder`, and the receiver of what it tells the window.
    #[must_use]
    pub fn new(folder: PathBuf) -> (Self, mpsc::UnboundedReceiver<ui::Event>) {
        let (events, receiver) = mpsc::unbounded_channel();
        let inner = Inner {
            files: Files::new(folder),
            http: reqwest::Client::new(),
            events,
            state: Mutex::new(State {
                connection: Connection::Connecting,
                hub: None,
                chats: None,
            }),
            wake: Notify::new(),
        };
        (
            Self {
                inner: Arc::new(inner),
            },
            receiver,
        )
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Every change to the state is whole before the lock is let go.
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn tell(&self, event: ui::Event) {
        // A window that has gone away does not need telling.
        let _ = self.inner.events.send(event);
    }

    fn set_connection(&self, connection: Connection) {
        self.state().connection = connection;
        self.tell(ui::Event::Overview);
    }

    /// Answers one request from the window. A failure is said in words for the person.
    pub async fn handle(&self, request: Request) -> Result<Response, String> {
        self.answer(request)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn answer(&self, request: Request) -> anyhow::Result<Response> {
        match request {
            Request::Hello => Ok(Response::Hello {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            }),
            Request::Setup => Ok(Response::Setup {
                joined: self.inner.files.identity()?.map(|identity| identity.name),
                suggested_name: suggested_name().map(String::from),
            }),
            Request::Join { code, name } => {
                self.join(&code, &name).await?;
                Ok(Response::Done)
            }
            Request::Overview => Ok(Response::Overview(self.overview())),
            Request::Chat { chat, thread } => {
                let state = self.state();
                let chats = state
                    .chats
                    .as_ref()
                    .context("the chats have not loaded yet")?;
                Ok(Response::Chat(chats.view(&chat, thread)))
            }
            Request::Older { chat } => {
                self.older(&chat).await?;
                Ok(Response::Done)
            }
            Request::Send { chat, thread, text } => {
                self.send(chat, thread, text).await?;
                Ok(Response::Done)
            }
            Request::Read { chat } => {
                self.read(&chat)?;
                Ok(Response::Done)
            }
            Request::CreateChannel { name } => {
                let name = ChannelName::new(name.trim().trim_start_matches('#'))?;
                self.hub()?.create_channel(&name).await?;
                self.channels_changed(Some(ChatId::Channel(name))).await?;
                Ok(Response::Done)
            }
            Request::JoinChannel { name } => {
                let name = ChannelName::new(name)?;
                self.hub()?.membership(&name, true).await?;
                self.channels_changed(Some(ChatId::Channel(name))).await?;
                Ok(Response::Done)
            }
            Request::Update => anyhow::bail!("updates are installed by the app, not the desk"),
            Request::Places => Ok(Response::Places(PlacesView {
                machine_runs_in: self.settings()?.machine_runs_in,
                folders: self.settings()?.folders,
                distros: system::distros().await,
            })),
            Request::Suggestions => {
                let settings = self.settings()?;
                let folders = places::suggestions(&settings, &system::distros().await).await;
                Ok(Response::Suggestions { folders })
            }
            Request::AddFolder { path, runs_in } => {
                self.add_folder(path, runs_in).await?;
                Ok(Response::Done)
            }
            Request::RemoveFolder { name } => {
                let mut settings = self.settings()?;
                settings.folders.retain(|folder| folder.name != name);
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::SetMachineRunsIn { runs_in } => {
                let mut settings = self.settings()?;
                settings.machine_runs_in = runs_in;
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::LeaveChannel { name } => {
                let name = ChannelName::new(name)?;
                self.hub()?.membership(&name, false).await?;
                self.channels_changed(None).await?;
                Ok(Response::Done)
            }
        }
    }

    /// The person's settings, or the defaults until they change one.
    fn settings(&self) -> anyhow::Result<Settings> {
        Ok(self.inner.files.read("settings.json")?.unwrap_or_default())
    }

    /// Keeps the settings, and tells the hub the places in them.
    async fn save_settings(&self, settings: &Settings) -> anyhow::Result<()> {
        self.inner.files.write("settings.json", settings)?;
        if let Ok(hub) = self.hub() {
            hub.set_places(&settings.advertised()).await?;
        }
        self.tell(ui::Event::Overview);
        Ok(())
    }

    async fn add_folder(
        &self,
        path: String,
        runs_in: player_two_core::event::RunsIn,
    ) -> anyhow::Result<()> {
        let path = path.trim().to_owned();
        anyhow::ensure!(
            places::exists(&path, &runs_in).await,
            "there is no folder at {path}"
        );
        let mut settings = self.settings()?;
        anyhow::ensure!(
            !settings
                .folders
                .iter()
                .any(|folder| folder.path == path && folder.runs_in == runs_in),
            "{path} is shared already"
        );
        let remote = places::remote_of(&path, &runs_in).await;
        let taken: Vec<_> = settings
            .folders
            .iter()
            .map(|folder| folder.name.clone())
            .collect();
        let name = places::name_for(&path, remote.as_deref(), &taken)?;
        settings.folders.push(Folder {
            name,
            path,
            runs_in,
        });
        self.save_settings(&settings).await
    }

    /// The address of the hub this machine joined, if it has.
    pub fn hub_address(&self) -> anyhow::Result<Option<String>> {
        Ok(self.inner.files.identity()?.map(|identity| identity.hub))
    }

    fn hub(&self) -> anyhow::Result<Hub> {
        self.state()
            .hub
            .clone()
            .context("this machine has not joined a hub")
    }

    fn overview(&self) -> Overview {
        let state = self.state();
        let chats = state.chats.as_ref();
        Overview {
            me: chats.map(|chats| chats.me().clone()),
            connection: state.connection.clone(),
            chats: chats.map(Chats::summaries).unwrap_or_default(),
            people: chats
                .map(|chats| chats.people().to_vec())
                .unwrap_or_default(),
            channels: chats
                .map(|chats| chats.channels().to_vec())
                .unwrap_or_default(),
        }
    }

    async fn join(&self, code: &str, name: &str) -> anyhow::Result<()> {
        let code = JoinCode::decode(code)?;
        let name = PersonName::new(name.trim()).context(
            "a name is a lowercase letter, then lowercase letters, digits or -, up to 32 in all",
        )?;
        let token = hub::join(&self.inner.http, &code.hub, &code.token, &name).await?;
        self.inner.files.save_identity(&Identity {
            hub: code.hub,
            name,
            token,
        })?;
        self.inner.wake.notify_one();
        Ok(())
    }

    async fn older(&self, chat: &ChatId) -> anyhow::Result<()> {
        let hub = self.hub()?;
        let before = self
            .state()
            .chats
            .as_ref()
            .and_then(|chats| chats.oldest(chat));
        let page = hub.page(chat, before, PAGE).await?;
        let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
        if let Some(chats) = self.state().chats.as_mut() {
            chats.add_page(chat, page, last);
        }
        self.tell(ui::Event::Chat { chat: chat.clone() });
        Ok(())
    }

    async fn send(
        &self,
        chat: ChatId,
        thread: Option<EventId>,
        text: String,
    ) -> anyhow::Result<()> {
        let text = text.trim_end().to_owned();
        anyhow::ensure!(!text.trim().is_empty(), "there is nothing to send");
        let hub = self.hub()?;
        let body = Body::Message(Message {
            text,
            tags: Vec::new(),
        });
        let posted = hub
            .post(&NewEvent {
                chat: chat.clone(),
                thread,
                body: body.clone(),
            })
            .await?;
        // The stream brings it too; showing it now saves waiting for that.
        let mut state = self.state();
        if let Some(chats) = state.chats.as_mut() {
            let author = chats.me().clone();
            chats.apply(Event {
                id: posted.id,
                chat: chat.clone(),
                thread,
                author,
                ts: posted.ts,
                body,
            });
        }
        drop(state);
        self.tell(ui::Event::Chat { chat });
        self.tell(ui::Event::Overview);
        Ok(())
    }

    fn read(&self, chat: &ChatId) -> anyhow::Result<()> {
        let marks = {
            let mut state = self.state();
            let Some(chats) = state.chats.as_mut() else {
                return Ok(());
            };
            chats.mark_read(chat);
            chats.read_marks().clone()
        };
        self.inner.files.write("read.json", &marks)?;
        self.tell(ui::Event::Overview);
        Ok(())
    }

    /// Fetches the channel list again, and the first page of a chat just joined.
    async fn channels_changed(&self, joined: Option<ChatId>) -> anyhow::Result<()> {
        let hub = self.hub()?;
        let channels = hub.channels().await?;
        let page = match &joined {
            Some(chat) => Some(hub.page(chat, None, PAGE).await?),
            None => None,
        };
        if let Some(chats) = self.state().chats.as_mut() {
            chats.set_channels(channels);
            if let (Some(chat), Some(page)) = (joined, page) {
                let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
                chats.add_page(&chat, page, last);
            }
        }
        self.tell(ui::Event::Overview);
        Ok(())
    }

    /// Fetches the people and channel lists again.
    async fn refresh(&self, hub: &Hub) -> anyhow::Result<()> {
        let (channels, people) = (hub.channels().await?, hub.people().await?);
        if let Some(chats) = self.state().chats.as_mut() {
            chats.set_channels(channels);
            chats.set_people(people);
        }
        Ok(())
    }

    /// Keeps the desk connected to the hub, for as long as the app runs.
    pub async fn run(&self) {
        let mut delay = Duration::from_secs(1);
        loop {
            let identity = match self.inner.files.identity() {
                Ok(Some(identity)) => identity,
                Ok(None) => {
                    self.set_connection(Connection::NotJoined);
                    self.inner.wake.notified().await;
                    continue;
                }
                Err(error) => {
                    self.set_connection(Connection::Offline {
                        reason: format!("{error:#}"),
                    });
                    self.inner.wake.notified().await;
                    continue;
                }
            };
            self.set_connection(Connection::Connecting);
            let ended = self.session(&identity, &mut delay).await;
            match ended {
                Err(error) if error.downcast_ref::<NotJoined>().is_some() => {
                    // The token is dead: the person must join again, so the first-run page shows.
                    let _ = self.inner.files.forget_identity();
                    let mut state = self.state();
                    state.hub = None;
                    state.chats = None;
                    drop(state);
                    continue;
                }
                Err(error) if error.downcast_ref::<WrongVersion>().is_some() => {
                    let hub = error
                        .downcast_ref::<WrongVersion>()
                        .map(|wrong| wrong.0.clone())
                        .unwrap_or_default();
                    self.set_connection(Connection::WrongVersion { hub });
                    // Nothing else happens until the versions match: an update, or a new hub.
                    tokio::select! {
                        () = tokio::time::sleep(RECHECK) => {}
                        () = self.inner.wake.notified() => {}
                    }
                    continue;
                }
                Err(error) => self.set_connection(Connection::Offline {
                    reason: format!("{error:#}"),
                }),
                Ok(()) => self.set_connection(Connection::Offline {
                    reason: "the hub ended the stream".to_owned(),
                }),
            }
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = self.inner.wake.notified() => {}
            }
            delay = (delay * 2).min(MOST_DELAY);
        }
    }

    /// One connection: load what is missing, then follow the stream until it ends.
    async fn session(&self, identity: &Identity, delay: &mut Duration) -> anyhow::Result<()> {
        let hub = Hub::new(self.inner.http.clone(), identity);
        let version = hub.version().await?;
        if version != env!("CARGO_PKG_VERSION") {
            return Err(WrongVersion(version).into());
        }
        hub.set_places(&self.settings()?.advertised()).await?;
        let (channels, people) = (hub.channels().await?, hub.people().await?);
        {
            let mut state = self.state();
            state.hub = Some(hub.clone());
            let read: BTreeMap<ChatId, EventId> =
                self.inner.files.read("read.json")?.unwrap_or_default();
            let chats = state
                .chats
                .get_or_insert_with(|| Chats::new(identity.name.clone(), read));
            chats.set_channels(channels);
            chats.set_people(people);
        }
        let unloaded: Vec<ChatId> = {
            let state = self.state();
            let chats = state.chats.as_ref().context("the chats were just made")?;
            chats
                .mine()
                .into_iter()
                .filter(|chat| !chats.has_loaded(chat))
                .collect()
        };
        for chat in unloaded {
            let page = hub.page(&chat, None, PAGE).await?;
            let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
            if let Some(chats) = self.state().chats.as_mut() {
                chats.add_page(&chat, page, last);
            }
        }
        let after = self
            .state()
            .chats
            .as_ref()
            .and_then(Chats::last_id)
            .unwrap_or(EventId(0));
        let mut stream = hub.stream(Some(after)).await?;
        *delay = Duration::from_secs(1);
        self.set_connection(Connection::Connected);
        let mut refresh = tokio::time::interval(REFRESH);
        refresh.tick().await;
        loop {
            tokio::select! {
                incoming = stream.next() => match incoming? {
                    Some(Incoming::Event(event)) => {
                        let chat = event.chat.clone();
                        let notice = self.state().chats.as_mut().and_then(|chats| {
                            let notice = chats.notice(&event);
                            chats.apply(event);
                            notice
                        });
                        self.tell(ui::Event::Chat { chat: chat.clone() });
                        self.tell(ui::Event::Overview);
                        if let Some((title, body)) = notice {
                            self.tell(ui::Event::Notify { chat, title, body });
                        }
                    }
                    Some(Incoming::Presence(presence)) => {
                        if let Some(chats) = self.state().chats.as_mut() {
                            chats.presence(&presence);
                        }
                        // Someone arriving may be new, or may bring new places or channels.
                        if presence.here {
                            self.refresh(&hub).await?;
                        }
                        self.tell(ui::Event::Overview);
                    }
                    None => return Ok(()),
                },
                _ = refresh.tick() => {
                    self.refresh(&hub).await?;
                    self.tell(ui::Event::Overview);
                }
            }
        }
    }
}

/// The name to offer at first run, from the system's user name.
fn suggested_name() -> Option<PersonName> {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()?;
    PersonName::from_user_name(&user)
}
